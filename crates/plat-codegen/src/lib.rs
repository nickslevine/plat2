/// Cranelift-based code generation for the Plat language
/// Generates native machine code from the Plat AST

use plat_ast::{self as ast, BinaryOp, Block, Expression, Function, Literal, MatchArm, Parameter, Pattern, Program, Statement, UnaryOp, EnumDecl, InterpolationPart};
use plat_ast::Type as AstType;
use cranelift_codegen::ir::types::*;
use std::os::raw::c_char;
use cranelift_codegen::ir::{
    AbiParam, Value, condcodes::IntCC, StackSlotData, StackSlotKind, MemFlags,
};
use cranelift_codegen::isa::CallConv;
use cranelift_codegen::settings::{self, Configurable};
use cranelift_codegen::Context;
use cranelift_codegen::ir::InstBuilder;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_module::{Linkage, Module, ModuleError, FuncId, DataDescription};
use cranelift_object::{ObjectBuilder, ObjectModule};
use std::collections::HashMap;

/// Track the original Plat types of variables for better codegen decisions
#[derive(Debug, Clone, PartialEq)]
pub enum VariableType {
    Bool,
    I32,
    I64,
    String,
    Array,
    Dict,
    Set,
    Class(String), // class name
    Enum(String), // enum name
}

pub struct CodeGenerator {
    module: ObjectModule,
    context: Context,
    functions: HashMap<String, FuncId>,
    string_counter: usize,
}

impl CodeGenerator {
    /// Determine the variable type that a match expression returns
    fn determine_match_return_type(arms: &[MatchArm], _variable_types: &HashMap<String, VariableType>) -> VariableType {
        if arms.is_empty() {
            return VariableType::I32;
        }

        // Check all arms to determine if we have mixed types requiring unified handling
        let mut has_string_literal = false;
        let mut has_integer_literal = false;
        let mut has_pattern_binding = false;

        for arm in arms {
            match &arm.body {
                Expression::Literal(Literal::String(_, _)) => has_string_literal = true,
                Expression::Literal(Literal::InterpolatedString(_, _)) => has_string_literal = true,
                Expression::Literal(Literal::Integer(_, _)) => has_integer_literal = true,
                Expression::Identifier { .. } => has_pattern_binding = true,
                _ => {}
            }
        }

        // If we have string literals OR pattern bindings mixed with other types, use String (I64)
        // If we only have integer literals and integer pattern bindings, use I32
        if has_string_literal || (has_pattern_binding && has_string_literal) {
            return VariableType::String;
        }

        // For pure integer cases (integer literals + integer pattern bindings), use I32
        if has_integer_literal || has_pattern_binding {
            return VariableType::I32;
        }

        // Fallback to specific type detection
        match &arms[0].body {
            Expression::Literal(Literal::Bool(_, _)) => VariableType::Bool,
            Expression::Literal(Literal::Array(_, _)) => VariableType::Array,
            Expression::Literal(Literal::Dict(_, _)) => VariableType::Dict,
            Expression::Literal(Literal::Set(_, _)) => VariableType::Set,
            Expression::EnumConstructor { enum_name, .. } => VariableType::Enum(enum_name.clone()),
            Expression::ConstructorCall { class_name, .. } => VariableType::Class(class_name.clone()),
            _ => VariableType::I32,
        }
    }
    pub fn new() -> Result<Self, CodegenError> {
        // Create ISA for the target platform
        let mut flag_builder = settings::builder();
        flag_builder.set("use_colocated_libcalls", "false")?;
        flag_builder.set("is_pic", "true")?;  // Enable position-independent code for macOS
        let isa_builder = cranelift_codegen::isa::lookup(target_lexicon::HOST)
            .map_err(|_| CodegenError::UnsupportedTarget)?;
        let isa = isa_builder
            .finish(settings::Flags::new(flag_builder))
            .map_err(|_| CodegenError::IsaCreationFailed)?;

        // Create object module
        let object_builder = ObjectBuilder::new(
            isa,
            "plat_program",
            cranelift_module::default_libcall_names(),
        ).map_err(CodegenError::ModuleError)?;
        let module = ObjectModule::new(object_builder);

        Ok(Self {
            module,
            context: Context::new(),
            functions: HashMap::new(),
            string_counter: 0,
        })
    }

    pub fn generate_code(mut self, program: &Program) -> Result<Vec<u8>, CodegenError> {
        // First pass: declare all functions (including enum methods)
        for function in &program.functions {
            self.declare_function(function)?;
        }

        // Declare enum methods
        for enum_decl in &program.enums {
            for method in &enum_decl.methods {
                let method_name = format!("{}::{}", enum_decl.name, method.name);
                self.declare_function_with_name(&method_name, method)?;
            }
        }

        // Second pass: generate code for all functions
        for function in &program.functions {
            self.generate_function(function)?;
        }

        // Generate code for enum methods
        for enum_decl in &program.enums {
            for method in &enum_decl.methods {
                let method_name = format!("{}::{}", enum_decl.name, method.name);
                self.generate_function_with_name(&method_name, method)?;
            }
        }

        // Finalize the module and return object code
        let object_product = self.module.finish();
        Ok(object_product.emit().map_err(CodegenError::ObjectEmitError)?)
    }

    fn declare_function(&mut self, function: &ast::Function) -> Result<(), CodegenError> {
        self.declare_function_with_name(&function.name, function)
    }

    fn declare_function_with_name(&mut self, name: &str, function: &ast::Function) -> Result<(), CodegenError> {
        let mut sig = self.module.make_signature();

        // Set calling convention
        sig.call_conv = CallConv::SystemV;

        // Add implicit self parameter for enum methods
        if name.contains("::") {
            // This is an enum method, add self parameter (represented as i64 for enum value)
            sig.params.push(AbiParam::new(I64));
        }

        // Add parameters
        for _param in &function.params {
            // For now, treat all parameters as i32
            sig.params.push(AbiParam::new(I32));
        }

        // Add return type
        if let Some(_return_type) = &function.return_type {
            // For now, treat all returns as i32
            sig.returns.push(AbiParam::new(I32));
        } else if function.name == "main" || name == "main" {
            // Main function always returns i32 (exit code) even if not specified
            sig.returns.push(AbiParam::new(I32));
        }

        let func_id = self.module.declare_function(name, Linkage::Export, &sig)
            .map_err(CodegenError::ModuleError)?;

        self.functions.insert(name.to_string(), func_id);

        Ok(())
    }

    fn generate_function(&mut self, function: &ast::Function) -> Result<(), CodegenError> {
        self.generate_function_with_name(&function.name, function)
    }

    fn generate_function_with_name(&mut self, name: &str, function: &ast::Function) -> Result<(), CodegenError> {
        let func_id = self.functions[name];

        // Get function signature
        let sig = self.module.declarations().get_function_decl(func_id).signature.clone();

        // Create the function in Cranelift IR
        self.context.func.signature = sig;

        // Create entry block
        let entry_block = self.context.func.dfg.make_block();

        // Create function builder
        let mut builder_context = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut self.context.func, &mut builder_context);
        builder.append_block_params_for_function_params(entry_block);
        builder.switch_to_block(entry_block);
        builder.seal_block(entry_block);

        // Create local variables map for this function
        let mut variables = HashMap::new();
        let mut variable_types = HashMap::new(); // Track original variable types
        let mut variable_counter = 0u32;

        // Add function parameters as variables
        let params = builder.block_params(entry_block).to_vec();
        for (i, param) in function.params.iter().enumerate() {
            let var = Variable::from_u32(variable_counter);
            variable_counter += 1;
            builder.declare_var(var, I32);
            builder.def_var(var, params[i]);
            variables.insert(param.name.clone(), var);

            // Track parameter type (for now, assume all params are I32)
            // TODO: Get actual parameter type from function signature
            variable_types.insert(param.name.clone(), VariableType::I32);
        }

        // Generate function body - we need to avoid borrowing conflicts
        // Extract the functions HashMap to avoid borrowing self while builder exists
        let functions_copy = self.functions.clone();
        let mut has_return = false;
        for statement in &function.body.statements {
            has_return |= Self::generate_statement_helper(
                &mut builder,
                statement,
                &mut variables,
                &mut variable_types,
                &mut variable_counter,
                &functions_copy,
                &mut self.module,
                &mut self.string_counter
            )?;
        }

        // If no explicit return, add default return
        if !has_return {
            if function.return_type.is_some() || function.name == "main" {
                // Return 0 as default for functions that should return a value
                // Main always needs to return an exit code even if no return type is specified
                let zero = builder.ins().iconst(I32, 0);
                builder.ins().return_(&[zero]);
            } else {
                builder.ins().return_(&[]);
            }
        }

        builder.finalize();

        // Define the function
        self.module.define_function(func_id, &mut self.context)
            .map_err(CodegenError::ModuleError)?;

        // Clear for next function
        self.context.clear();

        Ok(())
    }

    fn generate_statement_helper(
        builder: &mut FunctionBuilder,
        statement: &Statement,
        variables: &mut HashMap<String, Variable>,
        variable_types: &mut HashMap<String, VariableType>,
        variable_counter: &mut u32,
        functions: &HashMap<String, FuncId>,
        module: &mut ObjectModule,
        string_counter: &mut usize
    ) -> Result<bool, CodegenError> {
        match statement {
            Statement::Let { name, ty, value, .. } => {
                let val = Self::generate_expression_with_expected_type(builder, value, ty.as_ref(), variables, variable_types, functions, module, string_counter, variable_counter)?;
                let var = Variable::from_u32(*variable_counter);
                *variable_counter += 1;

                // Determine Cranelift type and Plat type based on expression
                let (cranelift_type, plat_type) = match value {
                    Expression::Literal(Literal::String(_, _)) => (I64, VariableType::String),
                    Expression::Literal(Literal::InterpolatedString(_, _)) => (I64, VariableType::String),
                    Expression::Literal(Literal::Array(_, _)) => (I64, VariableType::Array),
                    Expression::Literal(Literal::Dict(_, _)) => (I64, VariableType::Dict),
                    Expression::Literal(Literal::Set(_, _)) => (I64, VariableType::Set),
                    Expression::Index { .. } => (I32, VariableType::I32), // Array indexing returns i32 elements
                    Expression::MethodCall { object, method, .. } => {
                        // Check if this is a Dict method
                        if Self::is_dict_type(object, variable_types) {
                            match method.as_str() {
                                "get" | "get_or" => (I64, VariableType::I64), // Returns value type
                                "set" | "has_key" | "has_value" => (I32, VariableType::Bool),
                                "remove" => (I64, VariableType::I64), // Returns removed value
                                "clear" | "merge" => (I32, VariableType::I32), // Void
                                "length" => (I32, VariableType::I32),
                                "keys" | "values" => (I64, VariableType::Array),
                                _ => (I32, VariableType::I32),
                            }
                        } else {
                            match method.as_str() {
                                "len" | "length" | "count" => (I32, VariableType::I32),
                                "concat" | "trim" | "trim_left" | "trim_right" | "replace" | "replace_all" => (I64, VariableType::String),
                                "split" | "slice" => (I64, VariableType::Array),
                                "contains" | "starts_with" | "ends_with" | "is_alpha" | "is_numeric" | "is_alphanumeric" | "all" | "any" => (I32, VariableType::Bool),
                                "get" | "remove_at" | "index_of" => (I64, VariableType::Enum("Option".to_string())), // Returns Option<T>
                                "set" | "append" | "insert_at" | "clear" => (I32, VariableType::I32), // Returns unit/void, represented as i32
                                _ => (I32, VariableType::I32), // default fallback
                            }
                        }
                    }
                    Expression::Literal(Literal::Bool(_, _)) => (I32, VariableType::Bool),
                    Expression::EnumConstructor { enum_name, .. } => (I64, VariableType::Enum(enum_name.clone())),
                    Expression::ConstructorCall { class_name, .. } => (I64, VariableType::Class(class_name.clone())),
                    Expression::Match { arms, .. } => {
                        let match_type = Self::determine_match_return_type(arms, variable_types);
                        let cranelift_type = match match_type {
                            VariableType::String | VariableType::Array | VariableType::Enum(_) | VariableType::Class(_) => I64,
                            _ => I32,
                        };
                        (cranelift_type, match_type)
                    }
                    _ => (I32, VariableType::I32),
                };

                builder.declare_var(var, cranelift_type);
                builder.def_var(var, val);
                variables.insert(name.clone(), var);
                variable_types.insert(name.clone(), plat_type);
                Ok(false)
            }
            Statement::Var { name, ty, value, .. } => {
                let val = Self::generate_expression_with_expected_type(builder, value, ty.as_ref(), variables, variable_types, functions, module, string_counter, variable_counter)?;
                let var = Variable::from_u32(*variable_counter);
                *variable_counter += 1;

                // Determine Cranelift type and Plat type based on expression
                let (cranelift_type, plat_type) = match value {
                    Expression::Literal(Literal::String(_, _)) => (I64, VariableType::String),
                    Expression::Literal(Literal::InterpolatedString(_, _)) => (I64, VariableType::String),
                    Expression::Literal(Literal::Array(_, _)) => (I64, VariableType::Array),
                    Expression::Literal(Literal::Dict(_, _)) => (I64, VariableType::Dict),
                    Expression::Literal(Literal::Set(_, _)) => (I64, VariableType::Set),
                    Expression::Index { .. } => (I32, VariableType::I32), // Array indexing returns i32 elements
                    Expression::MethodCall { object, method, .. } => {
                        // Check if this is a Dict method
                        if Self::is_dict_type(object, variable_types) {
                            match method.as_str() {
                                "get" | "get_or" => (I64, VariableType::I64), // Returns value type
                                "set" | "has_key" | "has_value" => (I32, VariableType::Bool),
                                "remove" => (I64, VariableType::I64), // Returns removed value
                                "clear" | "merge" => (I32, VariableType::I32), // Void
                                "length" => (I32, VariableType::I32),
                                "keys" | "values" => (I64, VariableType::Array),
                                _ => (I32, VariableType::I32),
                            }
                        } else {
                            match method.as_str() {
                                "len" | "length" | "count" => (I32, VariableType::I32),
                                "concat" | "trim" | "trim_left" | "trim_right" | "replace" | "replace_all" => (I64, VariableType::String),
                                "split" | "slice" => (I64, VariableType::Array),
                                "contains" | "starts_with" | "ends_with" | "is_alpha" | "is_numeric" | "is_alphanumeric" | "all" | "any" => (I32, VariableType::Bool),
                                "get" | "remove_at" | "index_of" => (I64, VariableType::Enum("Option".to_string())), // Returns Option<T>
                                "set" | "append" | "insert_at" | "clear" => (I32, VariableType::I32), // Returns unit/void, represented as i32
                                _ => (I32, VariableType::I32), // default fallback
                            }
                        }
                    }
                    Expression::Literal(Literal::Bool(_, _)) => (I32, VariableType::Bool),
                    Expression::EnumConstructor { enum_name, .. } => (I64, VariableType::Enum(enum_name.clone())),
                    Expression::ConstructorCall { class_name, .. } => (I64, VariableType::Class(class_name.clone())),
                    Expression::Match { arms, .. } => {
                        let match_type = Self::determine_match_return_type(arms, variable_types);
                        let cranelift_type = match match_type {
                            VariableType::String | VariableType::Array | VariableType::Enum(_) | VariableType::Class(_) => I64,
                            _ => I32,
                        };
                        (cranelift_type, match_type)
                    }
                    _ => (I32, VariableType::I32),
                };

                builder.declare_var(var, cranelift_type);
                builder.def_var(var, val);
                variables.insert(name.clone(), var);
                variable_types.insert(name.clone(), plat_type);
                Ok(false)
            }
            Statement::Return { value, .. } => {
                if let Some(expr) = value {
                    let val = Self::generate_expression_helper(builder, expr, variables, variable_types, functions, module, string_counter, variable_counter)?;
                    builder.ins().return_(&[val]);
                } else {
                    builder.ins().return_(&[]);
                }
                Ok(true)
            }
            Statement::Expression(expr) => {
                Self::generate_expression_helper(builder, expr, variables, variable_types, functions, module, string_counter, variable_counter)?;
                Ok(false)
            }
            Statement::Print { value, .. } => {
                // Generate the value to print
                let val = Self::generate_expression_helper(builder, value, variables, variable_types, functions, module, string_counter, variable_counter)?;

                // Call the print runtime function
                // For now, we need to declare the print function if it's not already declared
                let print_func_name = "plat_print";
                let print_func_id = if let Some(&func_id) = functions.get(print_func_name) {
                    func_id
                } else {
                    // Declare the print function
                    let mut sig = module.make_signature();
                    sig.call_conv = CallConv::SystemV;
                    sig.params.push(AbiParam::new(I64)); // String pointer
                    // print returns void

                    let func_id = module.declare_function(print_func_name, Linkage::Import, &sig)
                        .map_err(CodegenError::ModuleError)?;
                    func_id
                };

                // Get function reference and call it
                let func_ref = module.declare_func_in_func(print_func_id, builder.func);
                builder.ins().call(func_ref, &[val]);

                Ok(false)
            }
            Statement::If { condition, then_branch, else_branch, .. } => {
                // Evaluate condition
                let condition_val = Self::generate_expression_helper(builder, condition, variables, variable_types, functions, module, string_counter, variable_counter)?;

                // Convert condition to boolean (non-zero = true)
                let _zero = builder.ins().iconst(I32, 0);
                let condition_bool = builder.ins().icmp_imm(IntCC::NotEqual, condition_val, 0);

                // Create blocks
                let then_block = builder.create_block();
                let else_block = builder.create_block();
                let merge_block = builder.create_block();

                // Branch based on condition
                builder.ins().brif(condition_bool, then_block, &[], else_block, &[]);

                // Generate then branch
                builder.switch_to_block(then_block);
                builder.seal_block(then_block);
                let mut then_has_return = false;
                for stmt in &then_branch.statements {
                    then_has_return |= Self::generate_statement_helper(
                        builder, stmt, variables, variable_types, variable_counter,
                        functions, module, string_counter
                    )?;
                }
                if !then_has_return {
                    builder.ins().jump(merge_block, &[]);
                }

                // Generate else branch
                builder.switch_to_block(else_block);
                builder.seal_block(else_block);
                let mut else_has_return = false;
                if let Some(else_block_ast) = else_branch {
                    for stmt in &else_block_ast.statements {
                        else_has_return |= Self::generate_statement_helper(
                            builder, stmt, variables, variable_types, variable_counter,
                            functions, module, string_counter
                        )?;
                    }
                }
                if !else_has_return {
                    builder.ins().jump(merge_block, &[]);
                }

                // Continue with merge block
                builder.switch_to_block(merge_block);
                builder.seal_block(merge_block);

                Ok(then_has_return && else_has_return)
            }
            Statement::While { condition, body, .. } => {
                // Create blocks
                let loop_header = builder.create_block();
                let loop_body = builder.create_block();
                let loop_exit = builder.create_block();

                // Jump to loop header
                builder.ins().jump(loop_header, &[]);

                // Loop header: evaluate condition
                builder.switch_to_block(loop_header);
                let condition_val = Self::generate_expression_helper(builder, condition, variables, variable_types, functions, module, string_counter, variable_counter)?;
                let _zero = builder.ins().iconst(I32, 0);
                let condition_bool = builder.ins().icmp_imm(IntCC::NotEqual, condition_val, 0);
                builder.ins().brif(condition_bool, loop_body, &[], loop_exit, &[]);

                // Loop body
                builder.switch_to_block(loop_body);
                let mut body_has_return = false;
                for stmt in &body.statements {
                    body_has_return |= Self::generate_statement_helper(
                        builder, stmt, variables, variable_types, variable_counter,
                        functions, module, string_counter
                    )?;
                }
                if !body_has_return {
                    builder.ins().jump(loop_header, &[]);
                }

                // Seal blocks after all predecessors are known
                builder.seal_block(loop_header);
                builder.seal_block(loop_body);

                // Loop exit
                builder.switch_to_block(loop_exit);
                builder.seal_block(loop_exit);

                Ok(false) // while loops don't guarantee return
            }
            Statement::For { variable, iterable, body, .. } => {
                // Evaluate iterable
                let array_val = Self::generate_expression_helper(builder, iterable, variables, variable_types, functions, module, string_counter, variable_counter)?;

                // Get array length
                let len_sig = {
                    let mut sig = module.make_signature();
                    sig.call_conv = CallConv::SystemV;
                    sig.params.push(AbiParam::new(I64)); // array pointer
                    sig.returns.push(AbiParam::new(I64)); // length
                    sig
                };

                let len_id = module.declare_function("plat_array_len", Linkage::Import, &len_sig)
                    .map_err(CodegenError::ModuleError)?;
                let len_ref = module.declare_func_in_func(len_id, builder.func);

                let call = builder.ins().call(len_ref, &[array_val]);
                let array_len = builder.inst_results(call)[0];
                let array_len_i32 = builder.ins().ireduce(I32, array_len);

                // Create loop variable for index
                let index_var = Variable::from_u32(*variable_counter);
                *variable_counter += 1;
                builder.declare_var(index_var, I32);
                let zero = builder.ins().iconst(I32, 0);
                builder.def_var(index_var, zero);

                // Create loop variable for element
                let element_var = Variable::from_u32(*variable_counter);
                *variable_counter += 1;
                builder.declare_var(element_var, I32);

                // Store in variables map
                let old_variable = variables.insert(variable.clone(), element_var);
                let old_type = variable_types.insert(variable.clone(), VariableType::I32);

                // Create blocks
                let loop_header = builder.create_block();
                let loop_body = builder.create_block();
                let loop_exit = builder.create_block();

                // Jump to loop header
                builder.ins().jump(loop_header, &[]);

                // Loop header: check if index < length
                builder.switch_to_block(loop_header);
                let current_index = builder.use_var(index_var);
                let condition = builder.ins().icmp(IntCC::SignedLessThan, current_index, array_len_i32);
                builder.ins().brif(condition, loop_body, &[], loop_exit, &[]);

                // Loop body: get array element and execute statements
                builder.switch_to_block(loop_body);

                // Get array element at current index
                let get_sig = {
                    let mut sig = module.make_signature();
                    sig.call_conv = CallConv::SystemV;
                    sig.params.push(AbiParam::new(I64)); // array pointer
                    sig.params.push(AbiParam::new(I64)); // index
                    sig.returns.push(AbiParam::new(I64)); // element value (now i64 for all types)
                    sig
                };

                let get_id = module.declare_function("plat_array_get", Linkage::Import, &get_sig)
                    .map_err(CodegenError::ModuleError)?;
                let get_ref = module.declare_func_in_func(get_id, builder.func);

                let index_i64 = builder.ins().uextend(I64, current_index);
                let call = builder.ins().call(get_ref, &[array_val, index_i64]);
                let element_val_i64 = builder.inst_results(call)[0];

                // For now, convert back to i32 for compatibility
                // TODO: Make this type-aware based on array element type
                let element_val = builder.ins().ireduce(I32, element_val_i64);

                // Set loop variable to current element
                builder.def_var(element_var, element_val);

                // Execute loop body statements
                let mut body_has_return = false;
                for stmt in &body.statements {
                    body_has_return |= Self::generate_statement_helper(
                        builder, stmt, variables, variable_types, variable_counter,
                        functions, module, string_counter
                    )?;
                }

                // Increment index
                if !body_has_return {
                    let one = builder.ins().iconst(I32, 1);
                    let next_index = builder.ins().iadd(current_index, one);
                    builder.def_var(index_var, next_index);
                    builder.ins().jump(loop_header, &[]);
                }

                // Seal blocks after all predecessors are known
                builder.seal_block(loop_header);
                builder.seal_block(loop_body);

                // Loop exit
                builder.switch_to_block(loop_exit);
                builder.seal_block(loop_exit);

                // Restore old variable binding if it existed
                if let Some(old_var) = old_variable {
                    variables.insert(variable.clone(), old_var);
                } else {
                    variables.remove(variable);
                }
                if let Some(old_typ) = old_type {
                    variable_types.insert(variable.clone(), old_typ);
                } else {
                    variable_types.remove(variable);
                }

                Ok(false) // for loops don't guarantee return
            }
        }
    }

    fn generate_expression_with_expected_type(
        builder: &mut FunctionBuilder,
        expr: &Expression,
        expected_type: Option<&AstType>,
        variables: &HashMap<String, Variable>,
        variable_types: &HashMap<String, VariableType>,
        functions: &HashMap<String, FuncId>,
        module: &mut ObjectModule,
        string_counter: &mut usize,
        variable_counter: &mut u32
    ) -> Result<Value, CodegenError> {
        match expr {
            Expression::Literal(Literal::Array(elements, _)) => {
                // Use expected type information for array generation
                Self::generate_typed_array_literal(builder, elements, expected_type, variables, variable_types, functions, module, string_counter, variable_counter)
            }
            Expression::Literal(Literal::Dict(pairs, _)) => {
                // Use expected type information for dict generation
                Self::generate_typed_dict_literal(builder, pairs, expected_type, variables, variable_types, functions, module, string_counter, variable_counter)
            }
            Expression::Literal(Literal::Set(elements, _)) => {
                // Use expected type information for set generation
                Self::generate_typed_set_literal(builder, elements, expected_type, variables, variable_types, functions, module, string_counter, variable_counter)
            }
            _ => {
                // For non-array expressions, use the regular helper
                Self::generate_expression_helper(builder, expr, variables, variable_types, functions, module, string_counter, variable_counter)
            }
        }
    }

    fn generate_typed_dict_literal(
        builder: &mut FunctionBuilder,
        pairs: &[(Expression, Expression)],
        expected_type: Option<&AstType>,
        variables: &HashMap<String, Variable>,
        variable_types: &HashMap<String, VariableType>,
        functions: &HashMap<String, FuncId>,
        module: &mut ObjectModule,
        string_counter: &mut usize,
        variable_counter: &mut u32
    ) -> Result<Value, CodegenError> {
        if pairs.is_empty() {
            // For empty dicts, determine type from annotation or default to string->i32
            let (_key_type, _value_type) = if let Some(AstType::Dict(key_type, value_type)) = expected_type {
                (key_type.as_ref(), value_type.as_ref())
            } else {
                (&AstType::String, &AstType::I32) // default
            };

            // Create empty dict
            let create_sig = {
                let mut sig = module.make_signature();
                sig.call_conv = CallConv::SystemV;
                sig.params.push(AbiParam::new(I64)); // keys pointer (null)
                sig.params.push(AbiParam::new(I64)); // values pointer (null)
                sig.params.push(AbiParam::new(I64)); // value_types pointer (null)
                sig.params.push(AbiParam::new(I64)); // count (0)
                sig.returns.push(AbiParam::new(I64)); // dict pointer
                sig
            };

            let create_id = module.declare_function("plat_dict_create", Linkage::Import, &create_sig)
                .map_err(CodegenError::ModuleError)?;
            let create_ref = module.declare_func_in_func(create_id, builder.func);

            let null_ptr = builder.ins().iconst(I64, 0);
            let count_val = builder.ins().iconst(I64, 0);
            let call = builder.ins().call(create_ref, &[null_ptr, null_ptr, null_ptr, count_val]);
            return Ok(builder.inst_results(call)[0]);
        }

        // Generate arrays for keys, values, and value types
        let mut keys = Vec::new();
        let mut values = Vec::new();
        let mut value_types = Vec::new();

        for (key_expr, value_expr) in pairs {
            // Evaluate key (must be string)
            let key_val = Self::generate_expression_helper(builder, key_expr, variables, variable_types, functions, module, string_counter, variable_counter)?;
            keys.push(key_val);

            // Evaluate value
            let value_val = Self::generate_expression_helper(builder, value_expr, variables, variable_types, functions, module, string_counter, variable_counter)?;
            values.push(value_val);

            // Determine value type
            let type_val = match value_expr {
                Expression::Literal(Literal::Bool(_, _)) => 2u8, // DICT_VALUE_TYPE_BOOL
                Expression::Literal(Literal::Integer(val, _)) => {
                    if *val > i32::MAX as i64 || *val < i32::MIN as i64 {
                        1u8 // DICT_VALUE_TYPE_I64
                    } else {
                        0u8 // DICT_VALUE_TYPE_I32
                    }
                }
                Expression::Literal(Literal::String(_, _)) => 3u8, // DICT_VALUE_TYPE_STRING
                Expression::Literal(Literal::InterpolatedString(_, _)) => 3u8,
                _ => 0u8, // default to i32
            };
            value_types.push(type_val);
        }

        let count = pairs.len() as i64;

        // Create temporary arrays on stack for keys, values, and types
        let keys_size = count * 8; // i64 pointers
        let values_size = count * 8; // i64 values
        let types_size = count * 1; // u8 types

        let keys_slot = builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, keys_size as u32, 8));
        let values_slot = builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, values_size as u32, 8));
        let types_slot = builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, types_size as u32, 1));

        // Store keys, values, and types
        for (i, ((key_val, value_val), type_val)) in keys.iter().zip(values.iter()).zip(value_types.iter()).enumerate() {
            let offset = (i * 8) as i32;
            builder.ins().stack_store(*key_val, keys_slot, offset);
            builder.ins().stack_store(*value_val, values_slot, offset);

            let type_offset = i as i32;
            let type_const = builder.ins().iconst(I32, *type_val as i64);
            builder.ins().stack_store(type_const, types_slot, type_offset);
        }

        // Get stack addresses
        let keys_addr = builder.ins().stack_addr(I64, keys_slot, 0);
        let values_addr = builder.ins().stack_addr(I64, values_slot, 0);
        let types_addr = builder.ins().stack_addr(I64, types_slot, 0);

        // Call plat_dict_create
        let create_sig = {
            let mut sig = module.make_signature();
            sig.call_conv = CallConv::SystemV;
            sig.params.push(AbiParam::new(I64)); // keys pointer
            sig.params.push(AbiParam::new(I64)); // values pointer
            sig.params.push(AbiParam::new(I64)); // value_types pointer
            sig.params.push(AbiParam::new(I64)); // count
            sig.returns.push(AbiParam::new(I64)); // dict pointer
            sig
        };

        let create_id = module.declare_function("plat_dict_create", Linkage::Import, &create_sig)
            .map_err(CodegenError::ModuleError)?;
        let create_ref = module.declare_func_in_func(create_id, builder.func);

        let count_val = builder.ins().iconst(I64, count);
        let call = builder.ins().call(create_ref, &[keys_addr, values_addr, types_addr, count_val]);
        let dict_ptr = builder.inst_results(call)[0];

        Ok(dict_ptr)
    }

    fn generate_typed_set_literal(
        builder: &mut FunctionBuilder,
        elements: &[Expression],
        expected_type: Option<&AstType>,
        variables: &HashMap<String, Variable>,
        variable_types: &HashMap<String, VariableType>,
        functions: &HashMap<String, FuncId>,
        module: &mut ObjectModule,
        string_counter: &mut usize,
        variable_counter: &mut u32
    ) -> Result<Value, CodegenError> {
        if elements.is_empty() {
            // For empty sets, determine type from annotation or default to i32
            let _element_type = if let Some(AstType::Set(element_type)) = expected_type {
                element_type.as_ref()
            } else {
                &AstType::I32 // default
            };

            // Create empty set
            let create_sig = {
                let mut sig = module.make_signature();
                sig.call_conv = CallConv::SystemV;
                sig.params.push(AbiParam::new(I64)); // values pointer (null)
                sig.params.push(AbiParam::new(I64)); // value_types pointer (null)
                sig.params.push(AbiParam::new(I64)); // count (0)
                sig.returns.push(AbiParam::new(I64)); // set pointer
                sig
            };

            let create_id = module.declare_function("plat_set_create", Linkage::Import, &create_sig)
                .map_err(CodegenError::ModuleError)?;
            let create_ref = module.declare_func_in_func(create_id, builder.func);

            let null_ptr = builder.ins().iconst(I64, 0);
            let count_val = builder.ins().iconst(I64, 0);
            let call = builder.ins().call(create_ref, &[null_ptr, null_ptr, count_val]);
            return Ok(builder.inst_results(call)[0]);
        }

        // Generate arrays for values and value types
        let mut values = Vec::new();
        let mut value_types = Vec::new();

        for element_expr in elements {
            // Evaluate element
            let value_val = Self::generate_expression_helper(builder, element_expr, variables, variable_types, functions, module, string_counter, variable_counter)?;
            values.push(value_val);

            // Determine value type
            let type_val = match element_expr {
                Expression::Literal(Literal::Bool(_, _)) => 2u8, // SET_VALUE_TYPE_BOOL
                Expression::Literal(Literal::Integer(val, _)) => {
                    if *val > i32::MAX as i64 || *val < i32::MIN as i64 {
                        1u8 // SET_VALUE_TYPE_I64
                    } else {
                        0u8 // SET_VALUE_TYPE_I32
                    }
                }
                Expression::Literal(Literal::String(_, _)) => 3u8, // SET_VALUE_TYPE_STRING
                Expression::Literal(Literal::InterpolatedString(_, _)) => 3u8,
                _ => 0u8, // default to i32
            };
            value_types.push(type_val);
        }

        let count = elements.len() as i64;

        // Create temporary arrays on stack for values and types
        let values_size = count * 8; // i64 values
        let types_size = count * 1; // u8 types

        let values_slot = builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, values_size as u32, 8));
        let types_slot = builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, types_size as u32, 1));

        // Store values and types
        for (i, (value_val, type_val)) in values.iter().zip(value_types.iter()).enumerate() {
            let offset = (i * 8) as i32;
            builder.ins().stack_store(*value_val, values_slot, offset);

            let type_offset = i as i32;
            let type_const = builder.ins().iconst(I32, *type_val as i64);
            builder.ins().stack_store(type_const, types_slot, type_offset);
        }

        // Get stack addresses
        let values_addr = builder.ins().stack_addr(I64, values_slot, 0);
        let types_addr = builder.ins().stack_addr(I64, types_slot, 0);

        // Call plat_set_create
        let create_sig = {
            let mut sig = module.make_signature();
            sig.call_conv = CallConv::SystemV;
            sig.params.push(AbiParam::new(I64)); // values pointer
            sig.params.push(AbiParam::new(I64)); // value_types pointer
            sig.params.push(AbiParam::new(I64)); // count
            sig.returns.push(AbiParam::new(I64)); // set pointer
            sig
        };

        let create_id = module.declare_function("plat_set_create", Linkage::Import, &create_sig)
            .map_err(CodegenError::ModuleError)?;
        let create_ref = module.declare_func_in_func(create_id, builder.func);

        let count_val = builder.ins().iconst(I64, count);
        let call = builder.ins().call(create_ref, &[values_addr, types_addr, count_val]);
        let set_ptr = builder.inst_results(call)[0];

        Ok(set_ptr)
    }

    fn generate_expression_helper(
        builder: &mut FunctionBuilder,
        expr: &Expression,
        variables: &HashMap<String, Variable>,
        variable_types: &HashMap<String, VariableType>,
        functions: &HashMap<String, FuncId>,
        module: &mut ObjectModule,
        string_counter: &mut usize,
        variable_counter: &mut u32
    ) -> Result<Value, CodegenError> {
        match expr {
            Expression::Literal(literal) => {
                Self::generate_literal(builder, literal, variables, variable_types, functions, module, string_counter, variable_counter)
            }
            Expression::Identifier { name, .. } => {
                if let Some(&var) = variables.get(name) {
                    Ok(builder.use_var(var))
                } else {
                    Err(CodegenError::UndefinedVariable(name.clone()))
                }
            }
            Expression::Binary { left, op, right, .. } => {
                match op {
                    // For non-short-circuit operators, evaluate both operands first
                    BinaryOp::Add | BinaryOp::Subtract | BinaryOp::Multiply |
                    BinaryOp::Divide | BinaryOp::Modulo | BinaryOp::Equal |
                    BinaryOp::NotEqual | BinaryOp::Less | BinaryOp::LessEqual |
                    BinaryOp::Greater | BinaryOp::GreaterEqual => {
                        let left_val = Self::generate_expression_helper(builder, left, variables, variable_types, functions, module, string_counter, variable_counter)?;
                        let right_val = Self::generate_expression_helper(builder, right, variables, variable_types, functions, module, string_counter, variable_counter)?;

                        match op {
                            BinaryOp::Add => Ok(builder.ins().iadd(left_val, right_val)),
                            BinaryOp::Subtract => Ok(builder.ins().isub(left_val, right_val)),
                            BinaryOp::Multiply => Ok(builder.ins().imul(left_val, right_val)),
                            BinaryOp::Divide => Ok(builder.ins().sdiv(left_val, right_val)),
                            BinaryOp::Modulo => Ok(builder.ins().srem(left_val, right_val)),
                            BinaryOp::Equal => {
                                let cmp = builder.ins().icmp(IntCC::Equal, left_val, right_val);
                                Ok(builder.ins().uextend(I32, cmp))
                            }
                            BinaryOp::NotEqual => {
                                let cmp = builder.ins().icmp(IntCC::NotEqual, left_val, right_val);
                                Ok(builder.ins().uextend(I32, cmp))
                            }
                            BinaryOp::Less => {
                                let cmp = builder.ins().icmp(IntCC::SignedLessThan, left_val, right_val);
                                Ok(builder.ins().uextend(I32, cmp))
                            }
                            BinaryOp::LessEqual => {
                                let cmp = builder.ins().icmp(IntCC::SignedLessThanOrEqual, left_val, right_val);
                                Ok(builder.ins().uextend(I32, cmp))
                            }
                            BinaryOp::Greater => {
                                let cmp = builder.ins().icmp(IntCC::SignedGreaterThan, left_val, right_val);
                                Ok(builder.ins().uextend(I32, cmp))
                            }
                            BinaryOp::GreaterEqual => {
                                let cmp = builder.ins().icmp(IntCC::SignedGreaterThanOrEqual, left_val, right_val);
                                Ok(builder.ins().uextend(I32, cmp))
                            }
                            _ => unreachable!()
                        }
                    }
                    BinaryOp::And => {
                        // Short-circuit AND: evaluate left first
                        let left_val = Self::generate_expression_helper(builder, left, variables, variable_types, functions, module, string_counter, variable_counter)?;

                        // If left is false, don't evaluate right
                        let zero = builder.ins().iconst(I32, 0);
                        let left_is_true = builder.ins().icmp_imm(IntCC::NotEqual, left_val, 0);

                        // Create blocks for short-circuit evaluation
                        let eval_right_block = builder.create_block();
                        let merge_block = builder.create_block();

                        // Add block parameter to merge block to receive the result
                        builder.append_block_param(merge_block, I32);

                        // If left is true, evaluate right; otherwise, short-circuit to false
                        builder.ins().brif(left_is_true, eval_right_block, &[], merge_block, &[zero]);

                        // Evaluate right expression
                        builder.switch_to_block(eval_right_block);
                        builder.seal_block(eval_right_block);

                        // Now evaluate the right operand
                        let right_val = Self::generate_expression_helper(builder, right, variables, variable_types, functions, module, string_counter, variable_counter)?;
                        let right_is_true = builder.ins().icmp_imm(IntCC::NotEqual, right_val, 0);
                        let right_as_i32 = builder.ins().uextend(I32, right_is_true);
                        builder.ins().jump(merge_block, &[right_as_i32]);

                        // Merge block contains the final result
                        builder.switch_to_block(merge_block);
                        builder.seal_block(merge_block);

                        Ok(builder.block_params(merge_block)[0])
                    }
                    BinaryOp::Or => {
                        // Short-circuit OR: evaluate left first
                        let left_val = Self::generate_expression_helper(builder, left, variables, variable_types, functions, module, string_counter, variable_counter)?;

                        // If left is true, don't evaluate right
                        let one = builder.ins().iconst(I32, 1);
                        let left_is_false = builder.ins().icmp_imm(IntCC::Equal, left_val, 0);

                        // Create blocks for short-circuit evaluation
                        let eval_right_block = builder.create_block();
                        let merge_block = builder.create_block();

                        // Add block parameter to merge block to receive the result
                        builder.append_block_param(merge_block, I32);

                        // If left is false, evaluate right; otherwise, short-circuit to true
                        builder.ins().brif(left_is_false, eval_right_block, &[], merge_block, &[one]);

                        // Evaluate right expression
                        builder.switch_to_block(eval_right_block);
                        builder.seal_block(eval_right_block);

                        // Now evaluate the right operand
                        let right_val = Self::generate_expression_helper(builder, right, variables, variable_types, functions, module, string_counter, variable_counter)?;
                        let right_is_true = builder.ins().icmp_imm(IntCC::NotEqual, right_val, 0);
                        let right_as_i32 = builder.ins().uextend(I32, right_is_true);
                        builder.ins().jump(merge_block, &[right_as_i32]);

                        // Merge block contains the final result
                        builder.switch_to_block(merge_block);
                        builder.seal_block(merge_block);

                        Ok(builder.block_params(merge_block)[0])
                    }
                }
            }
            Expression::Unary { op, operand, .. } => {
                let operand_val = Self::generate_expression_helper(builder, operand, variables, variable_types, functions, module, string_counter, variable_counter)?;

                match op {
                    UnaryOp::Negate => Ok(builder.ins().ineg(operand_val)),
                    UnaryOp::Not => {
                        // Convert to boolean (0 = false, non-zero = true), then flip
                        let zero = builder.ins().iconst(I32, 0);
                        let is_zero = builder.ins().icmp(IntCC::Equal, operand_val, zero);
                        Ok(builder.ins().uextend(I32, is_zero))
                    }
                }
            }
            Expression::Assignment { target, value, .. } => {
                let val = Self::generate_expression_helper(builder, value, variables, variable_types, functions, module, string_counter, variable_counter)?;

                match target.as_ref() {
                    Expression::Identifier { name, .. } => {
                        if let Some(&var) = variables.get(name) {
                            builder.def_var(var, val);
                            Ok(val)
                        } else {
                            Err(CodegenError::UndefinedVariable(name.clone()))
                        }
                    }
                    Expression::MemberAccess { object, member, .. } => {
                        // For member access assignment, we'll generate code to assign to the field
                        // This is more complex as it involves struct/class field assignment
                        // For now, let's return an error since class codegen isn't implemented yet
                        Err(CodegenError::UnsupportedFeature(
                            "Member access assignment not yet implemented in codegen".to_string()
                        ))
                    }
                    _ => {
                        Err(CodegenError::UnsupportedFeature(
                            "Invalid assignment target in codegen".to_string()
                        ))
                    }
                }
            }
            Expression::Call { function, args, .. } => {
                // Look up the function ID
                let func_id = match functions.get(function) {
                    Some(&id) => id,
                    None => return Err(CodegenError::UndefinedFunction(function.clone())),
                };

                // Get function reference for calling
                let func_ref = module.declare_func_in_func(func_id, builder.func);

                // Evaluate arguments
                let mut arg_values = Vec::new();
                for arg in args {
                    let arg_val = Self::generate_expression_helper(builder, arg, variables, variable_types, functions, module, string_counter, variable_counter)?;
                    arg_values.push(arg_val);
                }

                // Make the function call
                let call = builder.ins().call(func_ref, &arg_values);
                let results = builder.inst_results(call);

                // Return the first result (or unit if no results)
                if results.is_empty() {
                    // Function returns void, return 0
                    Ok(builder.ins().iconst(I32, 0))
                } else {
                    Ok(results[0])
                }
            }
            Expression::Index { object, index, .. } => {
                let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter)?;
                let index_val = Self::generate_expression_helper(builder, index, variables, variable_types, functions, module, string_counter, variable_counter)?;

                // Declare plat_array_get function
                let get_sig = {
                    let mut sig = module.make_signature();
                    sig.call_conv = CallConv::SystemV;
                    sig.params.push(AbiParam::new(I64)); // array pointer
                    sig.params.push(AbiParam::new(I64)); // index (we'll convert i32 to usize)
                    sig.returns.push(AbiParam::new(I64)); // element value (now i64 for all types)
                    sig
                };

                let get_id = module.declare_function("plat_array_get", Linkage::Import, &get_sig)
                    .map_err(CodegenError::ModuleError)?;
                let get_ref = module.declare_func_in_func(get_id, builder.func);

                // Convert i32 index to i64 for function call
                let index_i64 = builder.ins().uextend(I64, index_val);

                // Call plat_array_get
                let call = builder.ins().call(get_ref, &[object_val, index_i64]);
                let result_i64 = builder.inst_results(call)[0];

                // For now, convert back to i32 for compatibility
                // TODO: Make this type-aware based on array element type
                let result = builder.ins().ireduce(I32, result_i64);

                Ok(result)
            }
            Expression::MethodCall { object, method, args, .. } => {
                match method.as_str() {
                    "len" => {
                        if !args.is_empty() {
                            return Err(CodegenError::UnsupportedFeature("len() method takes no arguments".to_string()));
                        }

                        let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter)?;

                        // Declare plat_array_len function
                        let len_sig = {
                            let mut sig = module.make_signature();
                            sig.call_conv = CallConv::SystemV;
                            sig.params.push(AbiParam::new(I64)); // array pointer
                            sig.returns.push(AbiParam::new(I64)); // length
                            sig
                        };

                        let len_id = module.declare_function("plat_array_len", Linkage::Import, &len_sig)
                            .map_err(CodegenError::ModuleError)?;
                        let len_ref = module.declare_func_in_func(len_id, builder.func);

                        // Call plat_array_len
                        let call = builder.ins().call(len_ref, &[object_val]);
                        let len_i64 = builder.inst_results(call)[0];

                        // Convert length from i64 to i32 for consistency
                        let len_i32 = builder.ins().ireduce(I32, len_i64);

                        Ok(len_i32)
                    }
                    // Type-dispatched methods
                    "length" => {
                        if !args.is_empty() {
                            return Err(CodegenError::UnsupportedFeature("length() method takes no arguments".to_string()));
                        }

                        let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter)?;

                        // Determine object type for dispatch
                        let is_set = Self::is_set_type(object, variable_types);
                        let is_list = Self::is_list_type(object, variable_types);

                        if is_set {
                            // Set length
                            let func_sig = {
                                let mut sig = module.make_signature();
                                sig.call_conv = CallConv::SystemV;
                                sig.params.push(AbiParam::new(I64)); // set pointer
                                sig.returns.push(AbiParam::new(I32)); // length as i32
                                sig
                            };

                            let func_id = module.declare_function("plat_set_length", Linkage::Import, &func_sig)
                                .map_err(CodegenError::ModuleError)?;
                            let func_ref = module.declare_func_in_func(func_id, builder.func);

                            let call = builder.ins().call(func_ref, &[object_val]);
                            Ok(builder.inst_results(call)[0])
                        } else if is_list {
                            // Array length
                            let func_sig = {
                                let mut sig = module.make_signature();
                                sig.call_conv = CallConv::SystemV;
                                sig.params.push(AbiParam::new(I64)); // array pointer
                                sig.returns.push(AbiParam::new(I64)); // length
                                sig
                            };

                            let func_id = module.declare_function("plat_array_len", Linkage::Import, &func_sig)
                                .map_err(CodegenError::ModuleError)?;
                            let func_ref = module.declare_func_in_func(func_id, builder.func);

                            let call = builder.ins().call(func_ref, &[object_val]);
                            let len_i64 = builder.inst_results(call)[0];

                            // Convert length from i64 to i32 for consistency
                            let len_i32 = builder.ins().ireduce(I32, len_i64);
                            Ok(len_i32)
                        } else {
                            // String length (default case)
                            let func_sig = {
                                let mut sig = module.make_signature();
                                sig.call_conv = CallConv::SystemV;
                                sig.params.push(AbiParam::new(I64)); // string pointer
                                sig.returns.push(AbiParam::new(I32)); // character count as i32
                                sig
                            };

                            let func_id = module.declare_function("plat_string_length", Linkage::Import, &func_sig)
                                .map_err(CodegenError::ModuleError)?;
                            let func_ref = module.declare_func_in_func(func_id, builder.func);

                            let call = builder.ins().call(func_ref, &[object_val]);
                            Ok(builder.inst_results(call)[0])
                        }
                    }
                    "concat" => {
                        if args.len() != 1 {
                            return Err(CodegenError::UnsupportedFeature("concat() method takes exactly one argument".to_string()));
                        }

                        let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter)?;
                        let arg_val = Self::generate_expression_helper(builder, &args[0], variables, variable_types, functions, module, string_counter, variable_counter)?;

                        let func_sig = {
                            let mut sig = module.make_signature();
                            sig.call_conv = CallConv::SystemV;
                            sig.params.push(AbiParam::new(I64)); // string1 pointer
                            sig.params.push(AbiParam::new(I64)); // string2 pointer
                            sig.returns.push(AbiParam::new(I64)); // result string pointer
                            sig
                        };

                        let func_id = module.declare_function("plat_string_concat", Linkage::Import, &func_sig)
                            .map_err(CodegenError::ModuleError)?;
                        let func_ref = module.declare_func_in_func(func_id, builder.func);

                        let call = builder.ins().call(func_ref, &[object_val, arg_val]);
                        Ok(builder.inst_results(call)[0])
                    }
                    "contains" => {
                        if args.len() != 1 {
                            return Err(CodegenError::UnsupportedFeature("contains() method takes exactly one argument".to_string()));
                        }

                        let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter)?;
                        let arg_val = Self::generate_expression_helper(builder, &args[0], variables, variable_types, functions, module, string_counter, variable_counter)?;

                        // Determine object type for dispatch
                        let is_set = Self::is_set_type(object, variable_types);

                        if is_set {
                            // Set contains
                            let value_type = Self::get_set_value_type(&args[0], variable_types);

                            let func_sig = {
                                let mut sig = module.make_signature();
                                sig.call_conv = CallConv::SystemV;
                                sig.params.push(AbiParam::new(I64)); // set pointer
                                sig.params.push(AbiParam::new(I64)); // value (as i64)
                                sig.params.push(AbiParam::new(I32)); // value type
                                sig.returns.push(AbiParam::new(I32)); // bool as i32
                                sig
                            };

                            let func_id = module.declare_function("plat_set_contains", Linkage::Import, &func_sig)
                                .map_err(CodegenError::ModuleError)?;
                            let func_ref = module.declare_func_in_func(func_id, builder.func);

                            // Convert value to i64 if needed
                            let value_64 = if builder.func.dfg.value_type(arg_val) == I32 {
                                builder.ins().uextend(I64, arg_val)
                            } else {
                                arg_val
                            };

                            let value_type_const = builder.ins().iconst(I32, value_type as i64);
                            let call = builder.ins().call(func_ref, &[object_val, value_64, value_type_const]);
                            Ok(builder.inst_results(call)[0])
                        } else {
                            // String contains (default case)
                            let func_sig = {
                                let mut sig = module.make_signature();
                                sig.call_conv = CallConv::SystemV;
                                sig.params.push(AbiParam::new(I64)); // string pointer
                                sig.params.push(AbiParam::new(I64)); // substring pointer
                                sig.returns.push(AbiParam::new(I32)); // bool as i32
                                sig
                            };

                            let func_id = module.declare_function("plat_string_contains", Linkage::Import, &func_sig)
                                .map_err(CodegenError::ModuleError)?;
                            let func_ref = module.declare_func_in_func(func_id, builder.func);

                            let call = builder.ins().call(func_ref, &[object_val, arg_val]);
                            Ok(builder.inst_results(call)[0])
                        }
                    }
                    "starts_with" | "ends_with" => {
                        if args.len() != 1 {
                            return Err(CodegenError::UnsupportedFeature(format!("{}() method takes exactly one argument", method)));
                        }

                        let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter)?;
                        let arg_val = Self::generate_expression_helper(builder, &args[0], variables, variable_types, functions, module, string_counter, variable_counter)?;

                        let func_sig = {
                            let mut sig = module.make_signature();
                            sig.call_conv = CallConv::SystemV;
                            sig.params.push(AbiParam::new(I64)); // string pointer
                            sig.params.push(AbiParam::new(I64)); // substring pointer
                            sig.returns.push(AbiParam::new(I32)); // bool as i32
                            sig
                        };

                        let func_name = format!("plat_string_{}", method);
                        let func_id = module.declare_function(&func_name, Linkage::Import, &func_sig)
                            .map_err(CodegenError::ModuleError)?;
                        let func_ref = module.declare_func_in_func(func_id, builder.func);

                        let call = builder.ins().call(func_ref, &[object_val, arg_val]);
                        Ok(builder.inst_results(call)[0])
                    }
                    "trim" | "trim_left" | "trim_right" => {
                        if !args.is_empty() {
                            return Err(CodegenError::UnsupportedFeature(format!("{}() method takes no arguments", method)));
                        }

                        let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter)?;

                        let func_sig = {
                            let mut sig = module.make_signature();
                            sig.call_conv = CallConv::SystemV;
                            sig.params.push(AbiParam::new(I64)); // string pointer
                            sig.returns.push(AbiParam::new(I64)); // result string pointer
                            sig
                        };

                        let func_name = format!("plat_string_{}", method);
                        let func_id = module.declare_function(&func_name, Linkage::Import, &func_sig)
                            .map_err(CodegenError::ModuleError)?;
                        let func_ref = module.declare_func_in_func(func_id, builder.func);

                        let call = builder.ins().call(func_ref, &[object_val]);
                        Ok(builder.inst_results(call)[0])
                    }
                    "replace" | "replace_all" => {
                        if args.len() != 2 {
                            return Err(CodegenError::UnsupportedFeature(format!("{}() method takes exactly two arguments", method)));
                        }

                        let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter)?;
                        let from_val = Self::generate_expression_helper(builder, &args[0], variables, variable_types, functions, module, string_counter, variable_counter)?;
                        let to_val = Self::generate_expression_helper(builder, &args[1], variables, variable_types, functions, module, string_counter, variable_counter)?;

                        let func_sig = {
                            let mut sig = module.make_signature();
                            sig.call_conv = CallConv::SystemV;
                            sig.params.push(AbiParam::new(I64)); // string pointer
                            sig.params.push(AbiParam::new(I64)); // from string pointer
                            sig.params.push(AbiParam::new(I64)); // to string pointer
                            sig.returns.push(AbiParam::new(I64)); // result string pointer
                            sig
                        };

                        let func_name = format!("plat_string_{}", method);
                        let func_id = module.declare_function(&func_name, Linkage::Import, &func_sig)
                            .map_err(CodegenError::ModuleError)?;
                        let func_ref = module.declare_func_in_func(func_id, builder.func);

                        let call = builder.ins().call(func_ref, &[object_val, from_val, to_val]);
                        Ok(builder.inst_results(call)[0])
                    }
                    "split" => {
                        if args.len() != 1 {
                            return Err(CodegenError::UnsupportedFeature("split() method takes exactly one argument".to_string()));
                        }

                        let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter)?;
                        let delimiter_val = Self::generate_expression_helper(builder, &args[0], variables, variable_types, functions, module, string_counter, variable_counter)?;

                        let func_sig = {
                            let mut sig = module.make_signature();
                            sig.call_conv = CallConv::SystemV;
                            sig.params.push(AbiParam::new(I64)); // string pointer
                            sig.params.push(AbiParam::new(I64)); // delimiter string pointer
                            sig.returns.push(AbiParam::new(I64)); // result array pointer
                            sig
                        };

                        let func_id = module.declare_function("plat_string_split", Linkage::Import, &func_sig)
                            .map_err(CodegenError::ModuleError)?;
                        let func_ref = module.declare_func_in_func(func_id, builder.func);

                        let call = builder.ins().call(func_ref, &[object_val, delimiter_val]);
                        Ok(builder.inst_results(call)[0])
                    }
                    "is_alpha" | "is_numeric" | "is_alphanumeric" => {
                        if !args.is_empty() {
                            return Err(CodegenError::UnsupportedFeature(format!("{}() method takes no arguments", method)));
                        }

                        let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter)?;

                        let func_sig = {
                            let mut sig = module.make_signature();
                            sig.call_conv = CallConv::SystemV;
                            sig.params.push(AbiParam::new(I64)); // string pointer
                            sig.returns.push(AbiParam::new(I32)); // bool as i32
                            sig
                        };

                        let func_name = format!("plat_string_{}", method);
                        let func_id = module.declare_function(&func_name, Linkage::Import, &func_sig)
                            .map_err(CodegenError::ModuleError)?;
                        let func_ref = module.declare_func_in_func(func_id, builder.func);

                        let call = builder.ins().call(func_ref, &[object_val]);
                        Ok(builder.inst_results(call)[0])
                    }
                    // Array methods
                    "get" => {
                        if args.len() != 1 {
                            return Err(CodegenError::UnsupportedFeature("get() method takes exactly one argument".to_string()));
                        }

                        let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter)?;
                        let index_val = Self::generate_expression_helper(builder, &args[0], variables, variable_types, functions, module, string_counter, variable_counter)?;

                        let func_sig = {
                            let mut sig = module.make_signature();
                            sig.call_conv = CallConv::SystemV;
                            sig.params.push(AbiParam::new(I64)); // array pointer
                            sig.params.push(AbiParam::new(I32)); // index
                            sig.returns.push(AbiParam::new(I32)); // found (bool)
                            sig.returns.push(AbiParam::new(I64)); // value
                            sig
                        };

                        let func_id = module.declare_function("plat_array_get_safe", Linkage::Import, &func_sig)
                            .map_err(CodegenError::ModuleError)?;
                        let func_ref = module.declare_func_in_func(func_id, builder.func);

                        let call = builder.ins().call(func_ref, &[object_val, index_val]);
                        let results = builder.inst_results(call);

                        // For now, return packed Option<T> as i64 (found in high bit, value in low bits)
                        // found is i32 (0 or 1), value is i64
                        let found = results[0];
                        let value = results[1];
                        let found_64 = builder.ins().uextend(I64, found);
                        let found_shifted = builder.ins().ishl_imm(found_64, 63);
                        let result = builder.ins().bor(found_shifted, value);
                        Ok(result)
                    }
                    "set" => {
                        if args.len() != 2 {
                            return Err(CodegenError::UnsupportedFeature("set() method takes exactly two arguments".to_string()));
                        }

                        let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter)?;
                        let index_val = Self::generate_expression_helper(builder, &args[0], variables, variable_types, functions, module, string_counter, variable_counter)?;
                        let value_val = Self::generate_expression_helper(builder, &args[1], variables, variable_types, functions, module, string_counter, variable_counter)?;

                        // Convert value to i64 if needed
                        let value_64 = if builder.func.dfg.value_type(value_val) == I32 {
                            builder.ins().uextend(I64, value_val)
                        } else {
                            value_val
                        };

                        let func_sig = {
                            let mut sig = module.make_signature();
                            sig.call_conv = CallConv::SystemV;
                            sig.params.push(AbiParam::new(I64)); // array pointer (mutable)
                            sig.params.push(AbiParam::new(I32)); // index
                            sig.params.push(AbiParam::new(I64)); // value
                            sig.returns.push(AbiParam::new(I32)); // success (bool)
                            sig
                        };

                        let func_id = module.declare_function("plat_array_set", Linkage::Import, &func_sig)
                            .map_err(CodegenError::ModuleError)?;
                        let func_ref = module.declare_func_in_func(func_id, builder.func);

                        let call = builder.ins().call(func_ref, &[object_val, index_val, value_64]);
                        // Returns success as i32, but we're treating this as void operation for now
                        let zero = builder.ins().iconst(I32, 0);
                        Ok(zero)
                    }
                    "append" => {
                        if args.len() != 1 {
                            return Err(CodegenError::UnsupportedFeature("append() method takes exactly one argument".to_string()));
                        }

                        let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter)?;
                        let value_val = Self::generate_expression_helper(builder, &args[0], variables, variable_types, functions, module, string_counter, variable_counter)?;

                        // Convert value to i64 if needed
                        let value_64 = if builder.func.dfg.value_type(value_val) == I32 {
                            builder.ins().uextend(I64, value_val)
                        } else {
                            value_val
                        };

                        let func_sig = {
                            let mut sig = module.make_signature();
                            sig.call_conv = CallConv::SystemV;
                            sig.params.push(AbiParam::new(I64)); // array pointer (mutable)
                            sig.params.push(AbiParam::new(I64)); // value
                            sig.returns.push(AbiParam::new(I32)); // success (bool)
                            sig
                        };

                        let func_id = module.declare_function("plat_array_append", Linkage::Import, &func_sig)
                            .map_err(CodegenError::ModuleError)?;
                        let func_ref = module.declare_func_in_func(func_id, builder.func);

                        let call = builder.ins().call(func_ref, &[object_val, value_64]);
                        // Returns success as i32, but we're treating this as void operation for now
                        let zero = builder.ins().iconst(I32, 0);
                        Ok(zero)
                    }
                    "insert_at" => {
                        if args.len() != 2 {
                            return Err(CodegenError::UnsupportedFeature("insert_at() method takes exactly two arguments".to_string()));
                        }

                        let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter)?;
                        let index_val = Self::generate_expression_helper(builder, &args[0], variables, variable_types, functions, module, string_counter, variable_counter)?;
                        let value_val = Self::generate_expression_helper(builder, &args[1], variables, variable_types, functions, module, string_counter, variable_counter)?;

                        // Convert value to i64 if needed
                        let value_64 = if builder.func.dfg.value_type(value_val) == I32 {
                            builder.ins().uextend(I64, value_val)
                        } else {
                            value_val
                        };

                        let func_sig = {
                            let mut sig = module.make_signature();
                            sig.call_conv = CallConv::SystemV;
                            sig.params.push(AbiParam::new(I64)); // array pointer (mutable)
                            sig.params.push(AbiParam::new(I32)); // index
                            sig.params.push(AbiParam::new(I64)); // value
                            sig.returns.push(AbiParam::new(I32)); // success (bool)
                            sig
                        };

                        let func_id = module.declare_function("plat_array_insert_at", Linkage::Import, &func_sig)
                            .map_err(CodegenError::ModuleError)?;
                        let func_ref = module.declare_func_in_func(func_id, builder.func);

                        let call = builder.ins().call(func_ref, &[object_val, index_val, value_64]);
                        // Returns success as i32, but we're treating this as void operation for now
                        let zero = builder.ins().iconst(I32, 0);
                        Ok(zero)
                    }
                    "remove_at" => {
                        if args.len() != 1 {
                            return Err(CodegenError::UnsupportedFeature("remove_at() method takes exactly one argument".to_string()));
                        }

                        let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter)?;
                        let index_val = Self::generate_expression_helper(builder, &args[0], variables, variable_types, functions, module, string_counter, variable_counter)?;

                        let func_sig = {
                            let mut sig = module.make_signature();
                            sig.call_conv = CallConv::SystemV;
                            sig.params.push(AbiParam::new(I64)); // array pointer (mutable)
                            sig.params.push(AbiParam::new(I32)); // index
                            sig.returns.push(AbiParam::new(I32)); // found (bool)
                            sig.returns.push(AbiParam::new(I64)); // value
                            sig
                        };

                        let func_id = module.declare_function("plat_array_remove_at", Linkage::Import, &func_sig)
                            .map_err(CodegenError::ModuleError)?;
                        let func_ref = module.declare_func_in_func(func_id, builder.func);

                        let call = builder.ins().call(func_ref, &[object_val, index_val]);
                        let results = builder.inst_results(call);

                        // Return packed Option<T> as i64 (found in high bit, value in low bits)
                        let found = results[0];
                        let value = results[1];
                        let found_64 = builder.ins().uextend(I64, found);
                        let found_shifted = builder.ins().ishl_imm(found_64, 63);
                        let result = builder.ins().bor(found_shifted, value);
                        Ok(result)
                    }
                    "clear" => {
                        if !args.is_empty() {
                            return Err(CodegenError::UnsupportedFeature("clear() method takes no arguments".to_string()));
                        }

                        let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter)?;

                        // Determine object type for dispatch
                        let is_set = Self::is_set_type(object, variable_types);
                        let is_dict = Self::is_dict_type(object, variable_types);

                        if is_set {
                            // Set clear
                            let func_sig = {
                                let mut sig = module.make_signature();
                                sig.call_conv = CallConv::SystemV;
                                sig.params.push(AbiParam::new(I64)); // set pointer
                                sig
                            };

                            let func_id = module.declare_function("plat_set_clear", Linkage::Import, &func_sig)
                                .map_err(CodegenError::ModuleError)?;
                            let func_ref = module.declare_func_in_func(func_id, builder.func);

                            builder.ins().call(func_ref, &[object_val]);
                            let zero = builder.ins().iconst(I32, 0);
                            Ok(zero) // Unit type represented as 0
                        } else if is_dict {
                            // Dict clear
                            let func_sig = {
                                let mut sig = module.make_signature();
                                sig.call_conv = CallConv::SystemV;
                                sig.params.push(AbiParam::new(I64)); // dict pointer
                                sig
                            };

                            let func_id = module.declare_function("plat_dict_clear", Linkage::Import, &func_sig)
                                .map_err(CodegenError::ModuleError)?;
                            let func_ref = module.declare_func_in_func(func_id, builder.func);

                            builder.ins().call(func_ref, &[object_val]);
                            let zero = builder.ins().iconst(I32, 0);
                            Ok(zero) // Unit type represented as 0
                        } else {
                            // Array clear (default case)
                            let func_sig = {
                                let mut sig = module.make_signature();
                                sig.call_conv = CallConv::SystemV;
                                sig.params.push(AbiParam::new(I64)); // array pointer (mutable)
                                sig.returns.push(AbiParam::new(I32)); // success (bool)
                                sig
                            };

                            let func_id = module.declare_function("plat_array_clear", Linkage::Import, &func_sig)
                                .map_err(CodegenError::ModuleError)?;
                            let func_ref = module.declare_func_in_func(func_id, builder.func);

                            let call = builder.ins().call(func_ref, &[object_val]);
                            // Returns success as i32, but we're treating this as void operation for now
                            let zero = builder.ins().iconst(I32, 0);
                            Ok(zero)
                        }
                    }
                    "index_of" => {
                        if args.len() != 1 {
                            return Err(CodegenError::UnsupportedFeature("index_of() method takes exactly one argument".to_string()));
                        }

                        let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter)?;
                        let value_val = Self::generate_expression_helper(builder, &args[0], variables, variable_types, functions, module, string_counter, variable_counter)?;

                        // Convert value to i64 if needed
                        let value_64 = if builder.func.dfg.value_type(value_val) == I32 {
                            builder.ins().uextend(I64, value_val)
                        } else {
                            value_val
                        };

                        let func_sig = {
                            let mut sig = module.make_signature();
                            sig.call_conv = CallConv::SystemV;
                            sig.params.push(AbiParam::new(I64)); // array pointer
                            sig.params.push(AbiParam::new(I64)); // value to find
                            sig.returns.push(AbiParam::new(I32)); // found (bool)
                            sig.returns.push(AbiParam::new(I32)); // index
                            sig
                        };

                        let func_id = module.declare_function("plat_array_index_of", Linkage::Import, &func_sig)
                            .map_err(CodegenError::ModuleError)?;
                        let func_ref = module.declare_func_in_func(func_id, builder.func);

                        let call = builder.ins().call(func_ref, &[object_val, value_64]);
                        let results = builder.inst_results(call);

                        // Return packed Option<i32> as i64 (found in high bit, index in low bits)
                        let found = results[0];
                        let index = results[1];
                        let found_64 = builder.ins().uextend(I64, found);
                        let index_64 = builder.ins().uextend(I64, index);
                        let found_shifted = builder.ins().ishl_imm(found_64, 63);
                        let result = builder.ins().bor(found_shifted, index_64);
                        Ok(result)
                    }
                    "count" => {
                        if args.len() != 1 {
                            return Err(CodegenError::UnsupportedFeature("count() method takes exactly one argument".to_string()));
                        }

                        let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter)?;
                        let value_val = Self::generate_expression_helper(builder, &args[0], variables, variable_types, functions, module, string_counter, variable_counter)?;

                        // Convert value to i64 if needed
                        let value_64 = if builder.func.dfg.value_type(value_val) == I32 {
                            builder.ins().uextend(I64, value_val)
                        } else {
                            value_val
                        };

                        let func_sig = {
                            let mut sig = module.make_signature();
                            sig.call_conv = CallConv::SystemV;
                            sig.params.push(AbiParam::new(I64)); // array pointer
                            sig.params.push(AbiParam::new(I64)); // value to count
                            sig.returns.push(AbiParam::new(I32)); // count
                            sig
                        };

                        let func_id = module.declare_function("plat_array_count", Linkage::Import, &func_sig)
                            .map_err(CodegenError::ModuleError)?;
                        let func_ref = module.declare_func_in_func(func_id, builder.func);

                        let call = builder.ins().call(func_ref, &[object_val, value_64]);
                        Ok(builder.inst_results(call)[0])
                    }
                    "slice" => {
                        if args.len() != 2 {
                            return Err(CodegenError::UnsupportedFeature("slice() method takes exactly two arguments".to_string()));
                        }

                        let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter)?;
                        let start_val = Self::generate_expression_helper(builder, &args[0], variables, variable_types, functions, module, string_counter, variable_counter)?;
                        let end_val = Self::generate_expression_helper(builder, &args[1], variables, variable_types, functions, module, string_counter, variable_counter)?;

                        let func_sig = {
                            let mut sig = module.make_signature();
                            sig.call_conv = CallConv::SystemV;
                            sig.params.push(AbiParam::new(I64)); // array pointer
                            sig.params.push(AbiParam::new(I32)); // start index
                            sig.params.push(AbiParam::new(I32)); // end index
                            sig.returns.push(AbiParam::new(I64)); // new array pointer
                            sig
                        };

                        let func_id = module.declare_function("plat_array_slice", Linkage::Import, &func_sig)
                            .map_err(CodegenError::ModuleError)?;
                        let func_ref = module.declare_func_in_func(func_id, builder.func);

                        let call = builder.ins().call(func_ref, &[object_val, start_val, end_val]);
                        Ok(builder.inst_results(call)[0])
                    }
                    "all" => {
                        if args.len() != 1 {
                            return Err(CodegenError::UnsupportedFeature("all() method takes exactly one argument".to_string()));
                        }

                        let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter)?;

                        // For now, use simplified version that checks if all elements are truthy
                        let func_sig = {
                            let mut sig = module.make_signature();
                            sig.call_conv = CallConv::SystemV;
                            sig.params.push(AbiParam::new(I64)); // array pointer
                            sig.returns.push(AbiParam::new(I32)); // all are truthy (bool)
                            sig
                        };

                        let func_id = module.declare_function("plat_array_all_truthy", Linkage::Import, &func_sig)
                            .map_err(CodegenError::ModuleError)?;
                        let func_ref = module.declare_func_in_func(func_id, builder.func);

                        let call = builder.ins().call(func_ref, &[object_val]);
                        Ok(builder.inst_results(call)[0])
                    }
                    "any" => {
                        if args.len() != 1 {
                            return Err(CodegenError::UnsupportedFeature("any() method takes exactly one argument".to_string()));
                        }

                        let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter)?;

                        // For now, use simplified version that checks if any element is truthy
                        let func_sig = {
                            let mut sig = module.make_signature();
                            sig.call_conv = CallConv::SystemV;
                            sig.params.push(AbiParam::new(I64)); // array pointer
                            sig.returns.push(AbiParam::new(I32)); // any are truthy (bool)
                            sig
                        };

                        let func_id = module.declare_function("plat_array_any_truthy", Linkage::Import, &func_sig)
                            .map_err(CodegenError::ModuleError)?;
                        let func_ref = module.declare_func_in_func(func_id, builder.func);

                        let call = builder.ins().call(func_ref, &[object_val]);
                        Ok(builder.inst_results(call)[0])
                    }
                    // Dict-specific methods
                    method_name if Self::is_dict_type(object, variable_types) => {
                        match method_name {
                            "get" => {
                                if args.len() != 1 {
                                    return Err(CodegenError::UnsupportedFeature("Dict.get() method takes exactly one argument".to_string()));
                                }

                                let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter)?;
                                let key_val = Self::generate_expression_helper(builder, &args[0], variables, variable_types, functions, module, string_counter, variable_counter)?;

                                let func_sig = {
                                    let mut sig = module.make_signature();
                                    sig.call_conv = CallConv::SystemV;
                                    sig.params.push(AbiParam::new(I64)); // dict pointer
                                    sig.params.push(AbiParam::new(I64)); // key pointer
                                    sig.returns.push(AbiParam::new(I64)); // value
                                    sig
                                };

                                let func_id = module.declare_function("plat_dict_get", Linkage::Import, &func_sig)
                                    .map_err(CodegenError::ModuleError)?;
                                let func_ref = module.declare_func_in_func(func_id, builder.func);

                                let call = builder.ins().call(func_ref, &[object_val, key_val]);
                                Ok(builder.inst_results(call)[0])
                            }
                            "set" => {
                                if args.len() != 2 {
                                    return Err(CodegenError::UnsupportedFeature("Dict.set() method takes exactly two arguments".to_string()));
                                }

                                let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter)?;
                                let key_val = Self::generate_expression_helper(builder, &args[0], variables, variable_types, functions, module, string_counter, variable_counter)?;
                                let value_val = Self::generate_expression_helper(builder, &args[1], variables, variable_types, functions, module, string_counter, variable_counter)?;

                                // Determine value type
                                let value_type = Self::get_dict_value_type(&args[1], variable_types);

                                let func_sig = {
                                    let mut sig = module.make_signature();
                                    sig.call_conv = CallConv::SystemV;
                                    sig.params.push(AbiParam::new(I64)); // dict pointer
                                    sig.params.push(AbiParam::new(I64)); // key pointer
                                    sig.params.push(AbiParam::new(I64)); // value
                                    sig.params.push(AbiParam::new(I32)); // value type
                                    sig.returns.push(AbiParam::new(I32)); // success
                                    sig
                                };

                                let func_id = module.declare_function("plat_dict_set", Linkage::Import, &func_sig)
                                    .map_err(CodegenError::ModuleError)?;
                                let func_ref = module.declare_func_in_func(func_id, builder.func);

                                let value_type_const = builder.ins().iconst(I32, value_type as i64);
                                let call = builder.ins().call(func_ref, &[object_val, key_val, value_val, value_type_const]);
                                Ok(builder.inst_results(call)[0])
                            }
                            "remove" => {
                                if args.len() != 1 {
                                    return Err(CodegenError::UnsupportedFeature("Dict.remove() method takes exactly one argument".to_string()));
                                }

                                let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter)?;
                                let key_val = Self::generate_expression_helper(builder, &args[0], variables, variable_types, functions, module, string_counter, variable_counter)?;

                                let func_sig = {
                                    let mut sig = module.make_signature();
                                    sig.call_conv = CallConv::SystemV;
                                    sig.params.push(AbiParam::new(I64)); // dict pointer
                                    sig.params.push(AbiParam::new(I64)); // key pointer
                                    sig.returns.push(AbiParam::new(I64)); // removed value or 0
                                    sig
                                };

                                let func_id = module.declare_function("plat_dict_remove", Linkage::Import, &func_sig)
                                    .map_err(CodegenError::ModuleError)?;
                                let func_ref = module.declare_func_in_func(func_id, builder.func);

                                let call = builder.ins().call(func_ref, &[object_val, key_val]);
                                Ok(builder.inst_results(call)[0])
                            }
                            "clear" => {
                                if !args.is_empty() {
                                    return Err(CodegenError::UnsupportedFeature("Dict.clear() method takes no arguments".to_string()));
                                }

                                let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter)?;

                                let func_sig = {
                                    let mut sig = module.make_signature();
                                    sig.call_conv = CallConv::SystemV;
                                    sig.params.push(AbiParam::new(I64)); // dict pointer
                                    sig
                                };

                                let func_id = module.declare_function("plat_dict_clear", Linkage::Import, &func_sig)
                                    .map_err(CodegenError::ModuleError)?;
                                let func_ref = module.declare_func_in_func(func_id, builder.func);

                                builder.ins().call(func_ref, &[object_val]);
                                Ok(builder.ins().iconst(I32, 0)) // Return void as 0
                            }
                            "length" => {
                                if !args.is_empty() {
                                    return Err(CodegenError::UnsupportedFeature("Dict.length() method takes no arguments".to_string()));
                                }

                                let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter)?;

                                let func_sig = {
                                    let mut sig = module.make_signature();
                                    sig.call_conv = CallConv::SystemV;
                                    sig.params.push(AbiParam::new(I64)); // dict pointer
                                    sig.returns.push(AbiParam::new(I32)); // length as i32
                                    sig
                                };

                                let func_id = module.declare_function("plat_dict_len", Linkage::Import, &func_sig)
                                    .map_err(CodegenError::ModuleError)?;
                                let func_ref = module.declare_func_in_func(func_id, builder.func);

                                let call = builder.ins().call(func_ref, &[object_val]);
                                Ok(builder.inst_results(call)[0])
                            }
                            "keys" => {
                                if !args.is_empty() {
                                    return Err(CodegenError::UnsupportedFeature("Dict.keys() method takes no arguments".to_string()));
                                }

                                let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter)?;

                                let func_sig = {
                                    let mut sig = module.make_signature();
                                    sig.call_conv = CallConv::SystemV;
                                    sig.params.push(AbiParam::new(I64)); // dict pointer
                                    sig.returns.push(AbiParam::new(I64)); // array pointer
                                    sig
                                };

                                let func_id = module.declare_function("plat_dict_keys", Linkage::Import, &func_sig)
                                    .map_err(CodegenError::ModuleError)?;
                                let func_ref = module.declare_func_in_func(func_id, builder.func);

                                let call = builder.ins().call(func_ref, &[object_val]);
                                Ok(builder.inst_results(call)[0])
                            }
                            "values" => {
                                if !args.is_empty() {
                                    return Err(CodegenError::UnsupportedFeature("Dict.values() method takes no arguments".to_string()));
                                }

                                let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter)?;

                                let func_sig = {
                                    let mut sig = module.make_signature();
                                    sig.call_conv = CallConv::SystemV;
                                    sig.params.push(AbiParam::new(I64)); // dict pointer
                                    sig.returns.push(AbiParam::new(I64)); // array pointer
                                    sig
                                };

                                let func_id = module.declare_function("plat_dict_values", Linkage::Import, &func_sig)
                                    .map_err(CodegenError::ModuleError)?;
                                let func_ref = module.declare_func_in_func(func_id, builder.func);

                                let call = builder.ins().call(func_ref, &[object_val]);
                                Ok(builder.inst_results(call)[0])
                            }
                            "has_key" => {
                                if args.len() != 1 {
                                    return Err(CodegenError::UnsupportedFeature("Dict.has_key() method takes exactly one argument".to_string()));
                                }

                                let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter)?;
                                let key_val = Self::generate_expression_helper(builder, &args[0], variables, variable_types, functions, module, string_counter, variable_counter)?;

                                let func_sig = {
                                    let mut sig = module.make_signature();
                                    sig.call_conv = CallConv::SystemV;
                                    sig.params.push(AbiParam::new(I64)); // dict pointer
                                    sig.params.push(AbiParam::new(I64)); // key pointer
                                    sig.returns.push(AbiParam::new(I32)); // bool
                                    sig
                                };

                                let func_id = module.declare_function("plat_dict_has_key", Linkage::Import, &func_sig)
                                    .map_err(CodegenError::ModuleError)?;
                                let func_ref = module.declare_func_in_func(func_id, builder.func);

                                let call = builder.ins().call(func_ref, &[object_val, key_val]);
                                Ok(builder.inst_results(call)[0])
                            }
                            "has_value" => {
                                if args.len() != 1 {
                                    return Err(CodegenError::UnsupportedFeature("Dict.has_value() method takes exactly one argument".to_string()));
                                }

                                let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter)?;
                                let value_val = Self::generate_expression_helper(builder, &args[0], variables, variable_types, functions, module, string_counter, variable_counter)?;

                                // Determine value type
                                let value_type = Self::get_dict_value_type(&args[0], variable_types);

                                let func_sig = {
                                    let mut sig = module.make_signature();
                                    sig.call_conv = CallConv::SystemV;
                                    sig.params.push(AbiParam::new(I64)); // dict pointer
                                    sig.params.push(AbiParam::new(I64)); // value
                                    sig.params.push(AbiParam::new(I32)); // value type
                                    sig.returns.push(AbiParam::new(I32)); // bool
                                    sig
                                };

                                let func_id = module.declare_function("plat_dict_has_value", Linkage::Import, &func_sig)
                                    .map_err(CodegenError::ModuleError)?;
                                let func_ref = module.declare_func_in_func(func_id, builder.func);

                                let value_type_const = builder.ins().iconst(I32, value_type as i64);
                                let call = builder.ins().call(func_ref, &[object_val, value_val, value_type_const]);
                                Ok(builder.inst_results(call)[0])
                            }
                            "merge" => {
                                if args.len() != 1 {
                                    return Err(CodegenError::UnsupportedFeature("Dict.merge() method takes exactly one argument".to_string()));
                                }

                                let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter)?;
                                let other_val = Self::generate_expression_helper(builder, &args[0], variables, variable_types, functions, module, string_counter, variable_counter)?;

                                let func_sig = {
                                    let mut sig = module.make_signature();
                                    sig.call_conv = CallConv::SystemV;
                                    sig.params.push(AbiParam::new(I64)); // dict pointer
                                    sig.params.push(AbiParam::new(I64)); // other dict pointer
                                    sig
                                };

                                let func_id = module.declare_function("plat_dict_merge", Linkage::Import, &func_sig)
                                    .map_err(CodegenError::ModuleError)?;
                                let func_ref = module.declare_func_in_func(func_id, builder.func);

                                builder.ins().call(func_ref, &[object_val, other_val]);
                                Ok(builder.ins().iconst(I32, 0)) // Return void as 0
                            }
                            "get_or" => {
                                if args.len() != 2 {
                                    return Err(CodegenError::UnsupportedFeature("Dict.get_or() method takes exactly two arguments".to_string()));
                                }

                                let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter)?;
                                let key_val = Self::generate_expression_helper(builder, &args[0], variables, variable_types, functions, module, string_counter, variable_counter)?;
                                let default_val = Self::generate_expression_helper(builder, &args[1], variables, variable_types, functions, module, string_counter, variable_counter)?;

                                let func_sig = {
                                    let mut sig = module.make_signature();
                                    sig.call_conv = CallConv::SystemV;
                                    sig.params.push(AbiParam::new(I64)); // dict pointer
                                    sig.params.push(AbiParam::new(I64)); // key pointer
                                    sig.params.push(AbiParam::new(I64)); // default value
                                    sig.returns.push(AbiParam::new(I64)); // value or default
                                    sig
                                };

                                let func_id = module.declare_function("plat_dict_get_or", Linkage::Import, &func_sig)
                                    .map_err(CodegenError::ModuleError)?;
                                let func_ref = module.declare_func_in_func(func_id, builder.func);

                                let call = builder.ins().call(func_ref, &[object_val, key_val, default_val]);
                                Ok(builder.inst_results(call)[0])
                            }
                            _ => Err(CodegenError::UnsupportedFeature(format!("Dict method '{}' not implemented", method)))
                        }
                    }
                    // Set-only methods (not overlapping with other types)
                    "add" | "remove" | "union" | "intersection" | "difference" | "is_subset_of" | "is_superset_of" | "is_disjoint_from" => {
                        match method.as_str() {
                            "add" | "remove" => {
                                if args.len() != 1 {
                                    return Err(CodegenError::UnsupportedFeature(format!("Set.{}() method takes exactly one argument", method)));
                                }

                                let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter)?;
                                let value_val = Self::generate_expression_helper(builder, &args[0], variables, variable_types, functions, module, string_counter, variable_counter)?;

                                // Determine value type
                                let value_type = Self::get_set_value_type(&args[0], variable_types);

                                let func_sig = {
                                    let mut sig = module.make_signature();
                                    sig.call_conv = CallConv::SystemV;
                                    sig.params.push(AbiParam::new(I64)); // set pointer
                                    sig.params.push(AbiParam::new(I64)); // value (as i64)
                                    sig.params.push(AbiParam::new(I32)); // value type
                                    sig.returns.push(AbiParam::new(I32)); // bool as i32
                                    sig
                                };

                                let func_name = format!("plat_set_{}", method);
                                let func_id = module.declare_function(&func_name, Linkage::Import, &func_sig)
                                    .map_err(CodegenError::ModuleError)?;
                                let func_ref = module.declare_func_in_func(func_id, builder.func);

                                // Convert value to i64 if needed
                                let value_64 = if builder.func.dfg.value_type(value_val) == I32 {
                                    builder.ins().uextend(I64, value_val)
                                } else {
                                    value_val
                                };

                                let value_type_const = builder.ins().iconst(I32, value_type as i64);
                                let call = builder.ins().call(func_ref, &[object_val, value_64, value_type_const]);
                                Ok(builder.inst_results(call)[0])
                            }
                            "union" | "intersection" | "difference" => {
                                if args.len() != 1 {
                                    return Err(CodegenError::UnsupportedFeature(format!("Set.{}() method takes exactly one argument", method)));
                                }

                                let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter)?;
                                let other_val = Self::generate_expression_helper(builder, &args[0], variables, variable_types, functions, module, string_counter, variable_counter)?;

                                let func_sig = {
                                    let mut sig = module.make_signature();
                                    sig.call_conv = CallConv::SystemV;
                                    sig.params.push(AbiParam::new(I64)); // set1 pointer
                                    sig.params.push(AbiParam::new(I64)); // set2 pointer
                                    sig.returns.push(AbiParam::new(I64)); // new set pointer
                                    sig
                                };

                                let func_name = format!("plat_set_{}", method);
                                let func_id = module.declare_function(&func_name, Linkage::Import, &func_sig)
                                    .map_err(CodegenError::ModuleError)?;
                                let func_ref = module.declare_func_in_func(func_id, builder.func);

                                let call = builder.ins().call(func_ref, &[object_val, other_val]);
                                Ok(builder.inst_results(call)[0])
                            }
                            "is_subset_of" | "is_superset_of" | "is_disjoint_from" => {
                                if args.len() != 1 {
                                    return Err(CodegenError::UnsupportedFeature(format!("Set.{}() method takes exactly one argument", method)));
                                }

                                let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter)?;
                                let other_val = Self::generate_expression_helper(builder, &args[0], variables, variable_types, functions, module, string_counter, variable_counter)?;

                                let func_sig = {
                                    let mut sig = module.make_signature();
                                    sig.call_conv = CallConv::SystemV;
                                    sig.params.push(AbiParam::new(I64)); // set1 pointer
                                    sig.params.push(AbiParam::new(I64)); // set2 pointer
                                    sig.returns.push(AbiParam::new(I32)); // bool as i32
                                    sig
                                };

                                let func_name = format!("plat_set_{}", method);
                                let func_id = module.declare_function(&func_name, Linkage::Import, &func_sig)
                                    .map_err(CodegenError::ModuleError)?;
                                let func_ref = module.declare_func_in_func(func_id, builder.func);

                                let call = builder.ins().call(func_ref, &[object_val, other_val]);
                                Ok(builder.inst_results(call)[0])
                            }
                            _ => Err(CodegenError::UnsupportedFeature(format!("Set method '{}' not implemented", method)))
                        }
                    }
                    // Class methods
                    method_name if Self::is_class_type(object, variable_types) => {
                        let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter)?;
                        let class_name = Self::get_class_name(object, variable_types).unwrap_or_else(|| "Unknown".to_string());

                        // Generate function name for the class method: ClassName__method_name
                        let func_name = format!("{}__{}", class_name, method_name);

                        // Build method signature - first parameter is always the implicit 'self'
                        let func_sig = {
                            let mut sig = module.make_signature();
                            sig.call_conv = CallConv::SystemV;
                            sig.params.push(AbiParam::new(I64)); // self parameter (class instance pointer)

                            // Add parameters for method arguments
                            for _ in args {
                                sig.params.push(AbiParam::new(I64)); // All arguments as I64 for now
                            }

                            // Determine return type based on method
                            match method_name {
                                "change_name" => {
                                    // Void methods don't return anything
                                }
                                "get_magnitude" => {
                                    // Methods that return i32
                                    sig.returns.push(AbiParam::new(I32));
                                }
                                "add" => {
                                    // Methods that return class instances (Point objects)
                                    sig.returns.push(AbiParam::new(I64));
                                }
                                _ => {
                                    // Methods that return objects (class instances, etc.)
                                    sig.returns.push(AbiParam::new(I64));
                                }
                            }

                            sig
                        };

                        // Declare and call the method function
                        let func_id = module.declare_function(&func_name, Linkage::Import, &func_sig)
                            .map_err(CodegenError::ModuleError)?;
                        let func_ref = module.declare_func_in_func(func_id, builder.func);

                        // Generate arguments
                        let mut call_args = vec![object_val]; // Start with self
                        for arg in args {
                            let arg_val = Self::generate_expression_helper(builder, arg, variables, variable_types, functions, module, string_counter, variable_counter)?;
                            call_args.push(arg_val);
                        }

                        // Call the method
                        let call = builder.ins().call(func_ref, &call_args);

                        // Return result or unit value
                        if func_sig.returns.is_empty() {
                            // Void method - return unit (0) as I32
                            Ok(builder.ins().iconst(I32, 0))
                        } else {
                            // Method with return value - convert from I64 to I32 if needed
                            let result = builder.inst_results(call)[0];
                            if builder.func.dfg.value_type(result) == I64 {
                                // Convert I64 result to I32 for compatibility
                                Ok(builder.ins().ireduce(I32, result))
                            } else {
                                Ok(result)
                            }
                        }
                    }
                    _ => Err(CodegenError::UnsupportedFeature(format!("Method '{}' not implemented", method)))
                }
            }
            Expression::EnumConstructor { enum_name, variant, args, .. } => {
                let discriminant = Self::variant_discriminant(enum_name, variant);

                if args.is_empty() {
                    // Unit variant - just the discriminant in high 32 bits
                    let disc_val = builder.ins().iconst(I64, discriminant as i64);
                    let disc_shifted = builder.ins().ishl_imm(disc_val, 32);
                    Ok(disc_shifted)
                } else if args.len() == 1 {
                    // Check if the argument is a pointer type (String, Array, etc.)
                    // that cannot be packed into 32 bits
                    let arg_val = Self::generate_expression_helper(builder, &args[0], variables, variable_types, functions, module, string_counter, variable_counter)?;

                    // Determine if we need heap allocation based on the argument type
                    let needs_heap = match &args[0] {
                        Expression::Literal(Literal::String(_, _)) => true,
                        Expression::Literal(Literal::InterpolatedString(_, _)) => true,
                        Expression::Literal(Literal::Array(_, _)) => true,
                        Expression::Literal(Literal::Dict(_, _)) => true,
                        Expression::Literal(Literal::Set(_, _)) => true,
                        Expression::Identifier { name, .. } => {
                            matches!(variable_types.get(name), Some(VariableType::String) | Some(VariableType::Array) | Some(VariableType::Dict) | Some(VariableType::Set) | Some(VariableType::Class(_)))
                        }
                        _ => false,
                    };

                    if needs_heap {
                        // Use heap allocation for pointer types
                        // Declare GC allocation function
                        let gc_alloc_name = "plat_gc_alloc";
                        let gc_alloc_sig = {
                            let mut sig = module.make_signature();
                            sig.call_conv = CallConv::SystemV;
                            sig.params.push(AbiParam::new(I64)); // size parameter
                            sig.returns.push(AbiParam::new(I64)); // returns pointer
                            sig
                        };

                        let gc_alloc_id = module.declare_function(gc_alloc_name, Linkage::Import, &gc_alloc_sig)
                            .map_err(CodegenError::ModuleError)?;
                        let gc_alloc_ref = module.declare_func_in_func(gc_alloc_id, builder.func);

                        // Allocate space for discriminant (4 bytes) + pointer (8 bytes)
                        let size_val = builder.ins().iconst(I64, 12);
                        let call_inst = builder.ins().call(gc_alloc_ref, &[size_val]);
                        let ptr = builder.inst_results(call_inst)[0];

                        // Store discriminant at offset 0
                        let disc_val = builder.ins().iconst(I32, discriminant as i64);
                        builder.ins().store(MemFlags::new(), disc_val, ptr, 0);

                        // Store pointer at offset 4
                        builder.ins().store(MemFlags::new(), arg_val, ptr, 4);

                        Ok(ptr)
                    } else {
                        // Pack discriminant and value for i32 types
                        let disc_val = builder.ins().iconst(I64, discriminant as i64);
                        let disc_shifted = builder.ins().ishl_imm(disc_val, 32);
                        let arg_extended = builder.ins().uextend(I64, arg_val);
                        let packed = builder.ins().bor(disc_shifted, arg_extended);
                        Ok(packed)
                    }
                } else {
                    // Multiple fields - allocate struct on GC heap
                    // Layout: [discriminant:i32][field1][field2]...[fieldN]

                    // Declare GC allocation function
                    let gc_alloc_name = "plat_gc_alloc";
                    let gc_alloc_sig = {
                        let mut sig = module.make_signature();
                        sig.call_conv = CallConv::SystemV;
                        sig.params.push(AbiParam::new(I64)); // size parameter
                        sig.returns.push(AbiParam::new(I64)); // returns pointer
                        sig
                    };

                    let gc_alloc_id = module.declare_function(gc_alloc_name, Linkage::Import, &gc_alloc_sig)
                        .map_err(CodegenError::ModuleError)?;
                    let gc_alloc_ref = module.declare_func_in_func(gc_alloc_id, builder.func);

                    // Calculate size needed: discriminant (4 bytes) + args.len() * 4 bytes (assuming i32)
                    let total_size = 4 + args.len() * 4;
                    let size_val = builder.ins().iconst(I64, total_size as i64);

                    // Allocate memory
                    let call_inst = builder.ins().call(gc_alloc_ref, &[size_val]);
                    let ptr = builder.inst_results(call_inst)[0];

                    // Store discriminant at offset 0
                    let disc_val = builder.ins().iconst(I32, discriminant as i64);
                    builder.ins().store(MemFlags::new(), disc_val, ptr, 0);

                    // Store each field
                    for (i, arg) in args.iter().enumerate() {
                        let arg_val = Self::generate_expression_helper(builder, arg, variables, variable_types, functions, module, string_counter, variable_counter)?;
                        let offset = 4 + (i * 4) as i32; // discriminant + field index * field_size
                        builder.ins().store(MemFlags::new(), arg_val, ptr, offset);
                    }

                    Ok(ptr)
                }
            }
            Expression::Match { value, arms, .. } => {
                let value_val = Self::generate_expression_helper(builder, value, variables, variable_types, functions, module, string_counter, variable_counter)?;

                if arms.is_empty() {
                    return Err(CodegenError::UnsupportedFeature(
                        "Empty match expressions not supported".to_string()
                    ));
                }

                // For enum values, try packed format first (works for unit and i32 data variants)
                let disc_i32 = {
                    // Try packed format first - discriminant in high 32 bits
                    let packed_disc = builder.ins().ushr_imm(value_val, 32);
                    let packed_disc_i32 = builder.ins().ireduce(I32, packed_disc);

                    // Only use heap format if packed discriminant is 0 AND value looks like a pointer
                    let zero_const = builder.ins().iconst(I32, 0);
                    let disc_is_zero = builder.ins().icmp(cranelift_codegen::ir::condcodes::IntCC::Equal, packed_disc_i32, zero_const);

                    let min_addr = builder.ins().iconst(I64, 0x1000);
                    let looks_like_pointer = builder.ins().icmp(cranelift_codegen::ir::condcodes::IntCC::UnsignedGreaterThan, value_val, min_addr);
                    let max_addr = builder.ins().iconst(I64, 0x800000000000); // Reasonable upper bound for pointers
                    let not_too_big = builder.ins().icmp(cranelift_codegen::ir::condcodes::IntCC::UnsignedLessThan, value_val, max_addr);
                    let pointer_range = builder.ins().band(looks_like_pointer, not_too_big);

                    let use_heap = builder.ins().band(disc_is_zero, pointer_range);

                    let packed_block = builder.create_block();
                    let heap_block = builder.create_block();
                    let done_block = builder.create_block();
                    builder.append_block_param(done_block, I32);

                    builder.ins().brif(use_heap, heap_block, &[], packed_block, &[]);

                    // Packed format: use extracted discriminant
                    builder.switch_to_block(packed_block);
                    builder.seal_block(packed_block);
                    builder.ins().jump(done_block, &[packed_disc_i32]);

                    // Heap format: load discriminant from memory
                    builder.switch_to_block(heap_block);
                    builder.seal_block(heap_block);
                    let heap_disc = builder.ins().load(I32, MemFlags::new(), value_val, 0);
                    builder.ins().jump(done_block, &[heap_disc]);

                    builder.switch_to_block(done_block);
                    builder.seal_block(done_block);

                    builder.block_params(done_block)[0]
                };

                // Determine the return type for the match expression early
                let match_return_type = Self::determine_match_return_type(arms, variable_types);
                let cont_param_type = match match_return_type {
                    VariableType::String | VariableType::Array | VariableType::Enum(_) | VariableType::Class(_) => I64,
                    _ => I32,
                };

                // Create blocks for each arm and continuation
                let mut arm_blocks = Vec::new();
                for _ in 0..arms.len() {
                    arm_blocks.push(builder.create_block());
                }
                let cont_block = builder.create_block();

                // Generate cascade of conditional branches
                let initial_block = builder.current_block().unwrap();
                let mut current_block = initial_block;
                let mut sealed_blocks = Vec::new();

                for (i, arm) in arms.iter().enumerate() {
                    let arm_disc = if let Pattern::EnumVariant { variant, .. } = &arm.pattern {
                        Self::variant_discriminant("", variant)
                    } else {
                        return Err(CodegenError::UnsupportedFeature("Non-enum patterns not supported".to_string()));
                    };

                    if i == arms.len() - 1 {
                        // Last arm - unconditional jump (exhaustiveness guaranteed by HIR)
                        builder.ins().jump(arm_blocks[i], &[]);
                    } else {
                        // Check if discriminant matches this arm
                        let expected = builder.ins().iconst(I32, arm_disc as i64);
                        let is_match = builder.ins().icmp(cranelift_codegen::ir::condcodes::IntCC::Equal, disc_i32, expected);

                        // Create next comparison block for remaining arms
                        let next_block = builder.create_block();
                        builder.ins().brif(is_match, arm_blocks[i], &[], next_block, &[]);

                        // Switch to next comparison block
                        builder.switch_to_block(next_block);
                        // Only seal if it's not the initial block
                        if current_block != initial_block {
                            builder.seal_block(current_block);
                        }
                        sealed_blocks.push(current_block);
                        current_block = next_block;
                    }
                }

                // Generate code for each arm
                for (i, arm) in arms.iter().enumerate() {
                    builder.switch_to_block(arm_blocks[i]);
                    let mut arm_variables = variables.clone();
                    let mut arm_variable_types = variable_types.clone();

                    // Handle pattern bindings for this arm
                    if let Pattern::EnumVariant { bindings, .. } = &arm.pattern {
                        for (binding_idx, binding_name) in bindings.iter().enumerate() {
                            if !binding_name.is_empty() {
                                // For now, assume all single-field data variants use packed format
                                // and all multi-field variants use heap format
                                let (field_val, var_type, cranelift_type) = if bindings.len() == 1 {
                                    // Single field: assume packed format (discriminant in high, value in low)
                                    let packed_val = builder.ins().ireduce(I32, value_val);
                                    (packed_val, VariableType::I32, I32)
                                } else {
                                    // Multi-field: assume heap format, load from offset
                                    let offset = 4 + (binding_idx * 4) as i32; // 4-byte alignment for i32
                                    let loaded = builder.ins().load(I32, MemFlags::new(), value_val, offset);
                                    (loaded, VariableType::I32, I32)
                                };

                                let var = Variable::from_u32(*variable_counter);
                                *variable_counter += 1;
                                builder.declare_var(var, cranelift_type);
                                builder.def_var(var, field_val);
                                arm_variables.insert(binding_name.clone(), var);
                                arm_variable_types.insert(binding_name.clone(), var_type);
                            }
                        }
                    }

                    let arm_result = Self::generate_expression_helper(builder, &arm.body, &arm_variables, &arm_variable_types, functions, module, string_counter, variable_counter)?;

                    // Convert arm result to match the expected continuation block type
                    let converted_result = {
                        let arm_result_type = builder.func.dfg.value_type(arm_result);
                        if arm_result_type != cont_param_type {
                            // Convert between types if needed
                            match (arm_result_type, cont_param_type) {
                                (I64, I32) => builder.ins().ireduce(I32, arm_result),
                                (I32, I64) => builder.ins().uextend(I64, arm_result),
                                _ => arm_result, // Same type or unsupported conversion
                            }
                        } else {
                            arm_result
                        }
                    };

                    builder.ins().jump(cont_block, &[converted_result]);
                }

                // Continuation block
                builder.append_block_param(cont_block, cont_param_type);
                builder.switch_to_block(cont_block);

                // Seal all blocks
                for arm_block in arm_blocks {
                    builder.seal_block(arm_block);
                }
                builder.seal_block(cont_block);
                // Seal the last comparison block if it hasn't been sealed yet
                // and if it's not the initial block (which may be sealed elsewhere)
                if arms.len() > 1 && current_block != initial_block && !sealed_blocks.contains(&current_block) {
                    builder.seal_block(current_block);
                }

                let result = builder.block_params(cont_block)[0];
                Ok(result)
            }
            Expression::Try { expression, .. } => {
                // Generate code for the expression
                let expr_val = Self::generate_expression_helper(builder, expression, variables, variable_types, functions, module, string_counter, variable_counter)?;

                // The ? operator desugars to:
                // match expr {
                //     Option::Some(x) -> x,
                //     Option::None -> return Option::None,
                //     Result::Ok(x) -> x,
                //     Result::Err(e) -> return Result::Err(e),
                // }

                // For now, implement a simplified version that assumes the happy path
                // In a complete implementation, we would:
                // 1. Check the discriminant
                // 2. If it's None/Err, generate an early return
                // 3. If it's Some/Ok, extract the value

                // Simplified implementation: assume packed format and extract value
                // This works for simple cases like Option<i32>
                let extracted_val = builder.ins().ireduce(I32, expr_val);
                Ok(extracted_val)
            }
            Expression::MemberAccess { object, member, .. } => {
                // Generate code for reading a field from a class instance

                // First, evaluate the object expression to get the class pointer
                let object_val = Self::generate_expression_helper(
                    builder, object, variables, variable_types, functions, module, string_counter, variable_counter
                )?;

                // Create field name string
                let field_name_str = format!("field_access_{}", variable_counter);
                let field_name_id = module.declare_data(&field_name_str, Linkage::Local, false, false)
                    .map_err(CodegenError::ModuleError)?;
                let mut field_name_desc = DataDescription::new();
                let field_name_bytes = [member.as_bytes(), &[0]].concat();
                field_name_desc.define(field_name_bytes.into_boxed_slice());
                module.define_data(field_name_id, &field_name_desc)
                    .map_err(CodegenError::ModuleError)?;

                let field_name_addr = module.declare_data_in_func(field_name_id, builder.func);
                let field_name_val = builder.ins().symbol_value(I64, field_name_addr);

                *variable_counter += 1;

                // Determine field type based on field name and make appropriate runtime call
                // This is a temporary approach - we should improve this with proper type information
                match member.as_str() {
                    "name" => {
                        // String field - use plat_class_get_field_string
                        let get_field_sig = {
                            let mut sig = module.make_signature();
                            sig.call_conv = CallConv::SystemV;
                            sig.params.push(AbiParam::new(I64)); // class pointer
                            sig.params.push(AbiParam::new(I64)); // field name pointer
                            sig.returns.push(AbiParam::new(I64)); // string pointer
                            sig
                        };

                        let get_field_id = module.declare_function("plat_class_get_field_string", Linkage::Import, &get_field_sig)
                            .map_err(CodegenError::ModuleError)?;
                        let get_field_ref = module.declare_func_in_func(get_field_id, builder.func);

                        // Call plat_class_get_field_string
                        let call = builder.ins().call(get_field_ref, &[object_val, field_name_val]);
                        let field_value = builder.inst_results(call)[0];
                        Ok(field_value)
                    }
                    _ => {
                        // Assume integer field - use plat_class_get_field_i32
                        let get_field_sig = {
                            let mut sig = module.make_signature();
                            sig.call_conv = CallConv::SystemV;
                            sig.params.push(AbiParam::new(I64)); // class pointer
                            sig.params.push(AbiParam::new(I64)); // field name pointer
                            sig.returns.push(AbiParam::new(I32)); // field value (i32)
                            sig
                        };

                        let get_field_id = module.declare_function("plat_class_get_field_i32", Linkage::Import, &get_field_sig)
                            .map_err(CodegenError::ModuleError)?;
                        let get_field_ref = module.declare_func_in_func(get_field_id, builder.func);

                        // Call plat_class_get_field_i32
                        let call = builder.ins().call(get_field_ref, &[object_val, field_name_val]);
                        let field_value = builder.inst_results(call)[0];
                        Ok(field_value)
                    }
                }
            }
            Expression::ConstructorCall { class_name, args, .. } => {
                // Create a new class instance

                // First, create the class name string
                let class_name_cstring = std::ffi::CString::new(class_name.as_str()).unwrap();
                let class_name_ptr = class_name_cstring.as_ptr();

                // Declare plat_class_create function
                let create_sig = {
                    let mut sig = module.make_signature();
                    sig.call_conv = CallConv::SystemV;
                    sig.params.push(AbiParam::new(I64)); // class name string pointer
                    sig.returns.push(AbiParam::new(I64)); // class instance pointer
                    sig
                };

                let create_id = module.declare_function("plat_class_create", Linkage::Import, &create_sig)
                    .map_err(CodegenError::ModuleError)?;
                let create_ref = module.declare_func_in_func(create_id, builder.func);

                // Create a global data for the class name string
                let class_name_str = format!("class_name_{}", variable_counter);
                let class_name_id = module.declare_data(&class_name_str, Linkage::Local, false, false)
                    .map_err(CodegenError::ModuleError)?;
                let mut class_name_desc = DataDescription::new();
                let class_name_bytes = [class_name.as_bytes(), &[0]].concat();
                class_name_desc.define(class_name_bytes.into_boxed_slice());
                module.define_data(class_name_id, &class_name_desc)
                    .map_err(CodegenError::ModuleError)?;

                let class_name_addr = module.declare_data_in_func(class_name_id, builder.func);
                let class_name_val = builder.ins().symbol_value(I64, class_name_addr);

                *variable_counter += 1;

                // Call plat_class_create
                let call = builder.ins().call(create_ref, &[class_name_val]);
                let class_ptr = builder.inst_results(call)[0];

                // Set each field from the named arguments
                for arg in args {
                    let field_name = &arg.name;
                    let field_value_expr = &arg.value;

                    // Evaluate the field value
                    let field_value = Self::generate_expression_helper(
                        builder, field_value_expr, variables, variable_types, functions, module, string_counter, variable_counter
                    )?;

                    // Create field name string
                    let field_name_str = format!("field_name_{}", variable_counter);
                    let field_name_id = module.declare_data(&field_name_str, Linkage::Local, false, false)
                        .map_err(CodegenError::ModuleError)?;
                    let mut field_name_desc = DataDescription::new();
                    let field_name_bytes = [field_name.as_bytes(), &[0]].concat();
                    field_name_desc.define(field_name_bytes.into_boxed_slice());
                    module.define_data(field_name_id, &field_name_desc)
                        .map_err(CodegenError::ModuleError)?;

                    let field_name_addr = module.declare_data_in_func(field_name_id, builder.func);
                    let field_name_val = builder.ins().symbol_value(I64, field_name_addr);

                    *variable_counter += 1;

                    // Determine field type and call appropriate setter
                    // Check if this is a string literal
                    match field_value_expr {
                        Expression::Literal(Literal::String(_, _)) |
                        Expression::Literal(Literal::InterpolatedString(_, _)) => {
                            // String field
                            let set_field_sig = {
                                let mut sig = module.make_signature();
                                sig.call_conv = CallConv::SystemV;
                                sig.params.push(AbiParam::new(I64)); // class pointer
                                sig.params.push(AbiParam::new(I64)); // field name pointer
                                sig.params.push(AbiParam::new(I64)); // field value (string pointer)
                                sig
                            };

                            let set_field_id = module.declare_function("plat_class_set_field_string", Linkage::Import, &set_field_sig)
                                .map_err(CodegenError::ModuleError)?;
                            let set_field_ref = module.declare_func_in_func(set_field_id, builder.func);

                            // Call plat_class_set_field_string
                            builder.ins().call(set_field_ref, &[class_ptr, field_name_val, field_value]);
                        }
                        _ => {
                            // Integer field (default case)
                            let set_field_sig = {
                                let mut sig = module.make_signature();
                                sig.call_conv = CallConv::SystemV;
                                sig.params.push(AbiParam::new(I64)); // class pointer
                                sig.params.push(AbiParam::new(I64)); // field name pointer
                                sig.params.push(AbiParam::new(I32)); // field value (i32)
                                sig
                            };

                            let set_field_id = module.declare_function("plat_class_set_field_i32", Linkage::Import, &set_field_sig)
                                .map_err(CodegenError::ModuleError)?;
                            let set_field_ref = module.declare_func_in_func(set_field_id, builder.func);

                            // For now, assume field value is already i32
                            let field_value_i32 = field_value;

                            // Call plat_class_set_field_i32
                            builder.ins().call(set_field_ref, &[class_ptr, field_name_val, field_value_i32]);
                        }
                    }
                }

                // Return the class pointer
                Ok(class_ptr)
            }
            Expression::Self_ { .. } => {
                // For now, return an error since we need to implement self parameter
                Err(CodegenError::UnsupportedFeature("Self references not yet implemented".to_string()))
            }
            Expression::Block(block) => {
                // For now, return an error since we need to implement block expressions
                Err(CodegenError::UnsupportedFeature("Block expressions not yet implemented".to_string()))
            }
            _ => {
                // TODO: Implement any remaining expressions
                Err(CodegenError::UnsupportedFeature("Complex expressions not yet implemented".to_string()))
            }
        }
    }

    fn generate_typed_array_literal(
        builder: &mut FunctionBuilder,
        elements: &[Expression],
        expected_type: Option<&AstType>,
        variables: &HashMap<String, Variable>,
        variable_types: &HashMap<String, VariableType>,
        functions: &HashMap<String, FuncId>,
        module: &mut ObjectModule,
        string_counter: &mut usize,
        variable_counter: &mut u32
    ) -> Result<Value, CodegenError> {
        if elements.is_empty() {
            // For empty arrays, determine type from annotation or default to i32
            let element_type = if let Some(AstType::List(element_type)) = expected_type {
                element_type.as_ref()
            } else {
                &AstType::I32 // default
            };

            let function_name = match element_type {
                AstType::Bool => "plat_array_create_bool",
                AstType::I32 => "plat_array_create_i32",
                AstType::I64 => "plat_array_create_i64",
                AstType::String => "plat_array_create_string",
                _ => "plat_array_create_i32", // fallback
            };

            let create_sig = {
                let mut sig = module.make_signature();
                sig.call_conv = CallConv::SystemV;
                sig.params.push(AbiParam::new(I64)); // elements pointer
                sig.params.push(AbiParam::new(I64)); // count
                sig.returns.push(AbiParam::new(I64)); // array pointer
                sig
            };

            let create_id = module.declare_function(function_name, Linkage::Import, &create_sig)
                .map_err(CodegenError::ModuleError)?;
            let create_ref = module.declare_func_in_func(create_id, builder.func);

            let count_val = builder.ins().iconst(I64, 0);
            let null_ptr = builder.ins().iconst(I64, 0);
            let call = builder.ins().call(create_ref, &[null_ptr, count_val]);
            let array_ptr = builder.inst_results(call)[0];
            return Ok(array_ptr);
        }

        // Determine element type from annotation or infer from first element
        let element_type = if let Some(AstType::List(element_type)) = expected_type {
            element_type.as_ref()
        } else {
            // Fallback to inference from first element
            match &elements[0] {
                Expression::Literal(Literal::Bool(_, _)) => &AstType::Bool,
                Expression::Literal(Literal::String(_, _)) => &AstType::String,
                Expression::Literal(Literal::InterpolatedString(_, _)) => &AstType::String,
                Expression::Literal(Literal::Integer(value, _)) => {
                    if *value > i32::MAX as i64 || *value < i32::MIN as i64 {
                        &AstType::I64
                    } else {
                        &AstType::I32
                    }
                },
                _ => &AstType::I32,
            }
        };

        let (element_size, function_name) = match element_type {
            AstType::Bool => (std::mem::size_of::<bool>(), "plat_array_create_bool"),
            AstType::I32 => (std::mem::size_of::<i32>(), "plat_array_create_i32"),
            AstType::I64 => (std::mem::size_of::<i64>(), "plat_array_create_i64"),
            AstType::String => (std::mem::size_of::<*const u8>(), "plat_array_create_string"),
            _ => (std::mem::size_of::<i32>(), "plat_array_create_i32"), // fallback
        };

        // Generate all element values
        let mut element_values = Vec::new();
        for element in elements {
            let element_val = Self::generate_expression_helper(builder, element, variables, variable_types, functions, module, string_counter, variable_counter)?;
            element_values.push(element_val);
        }

        // Create array literal on stack temporarily
        let count = elements.len() as i64;
        let element_size_i64 = element_size as i64;
        let total_size = count * element_size_i64;

        // Allocate stack space for temporary array data
        let stack_slot = builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, total_size as u32, 8));

        // Store each element to the stack array
        for (i, &element_val) in element_values.iter().enumerate() {
            let offset = (i as i64) * element_size_i64;
            let addr = builder.ins().stack_addr(I64, stack_slot, offset as i32);
            builder.ins().store(MemFlags::new(), element_val, addr, 0);
        }

        // Get pointer to stack array data
        let stack_addr = builder.ins().stack_addr(I64, stack_slot, 0);

        // Declare type-specific plat_array_create function
        let create_sig = {
            let mut sig = module.make_signature();
            sig.call_conv = CallConv::SystemV;
            sig.params.push(AbiParam::new(I64)); // elements pointer
            sig.params.push(AbiParam::new(I64)); // count
            sig.returns.push(AbiParam::new(I64)); // array pointer
            sig
        };

        let create_id = module.declare_function(function_name, Linkage::Import, &create_sig)
            .map_err(CodegenError::ModuleError)?;
        let create_ref = module.declare_func_in_func(create_id, builder.func);

        // Call type-specific plat_array_create with stack data and count
        let count_val = builder.ins().iconst(I64, count);
        let call = builder.ins().call(create_ref, &[stack_addr, count_val]);
        let array_ptr = builder.inst_results(call)[0];

        Ok(array_ptr)
    }

    fn generate_literal(
        builder: &mut FunctionBuilder,
        literal: &Literal,
        variables: &HashMap<String, Variable>,
        variable_types: &HashMap<String, VariableType>,
        functions: &HashMap<String, FuncId>,
        module: &mut ObjectModule,
        string_counter: &mut usize,
        variable_counter: &mut u32
    ) -> Result<Value, CodegenError> {
        match literal {
            Literal::Bool(b, _) => {
                let val = if *b { 1i64 } else { 0i64 };
                Ok(builder.ins().iconst(I32, val))
            }
            Literal::Integer(i, _) => {
                Ok(builder.ins().iconst(I32, *i))
            }
            Literal::String(s, _) => {
                // Allocate string on GC heap

                // First, declare the plat_gc_alloc function if not already declared
                let gc_alloc_name = "plat_gc_alloc";
                let gc_alloc_sig = {
                    let mut sig = module.make_signature();
                    sig.call_conv = CallConv::SystemV;
                    sig.params.push(AbiParam::new(I64)); // size parameter
                    sig.returns.push(AbiParam::new(I64)); // returns pointer
                    sig
                };

                let gc_alloc_id = module.declare_function(gc_alloc_name, Linkage::Import, &gc_alloc_sig)
                    .map_err(CodegenError::ModuleError)?;
                let gc_alloc_ref = module.declare_func_in_func(gc_alloc_id, builder.func);

                // Calculate string size (including null terminator)
                let string_size = s.len() + 1;
                let size_val = builder.ins().iconst(I64, string_size as i64);

                // Call plat_gc_alloc to allocate memory
                let call = builder.ins().call(gc_alloc_ref, &[size_val]);
                let string_ptr = builder.inst_results(call)[0];

                // Now we need to copy the string data to the allocated memory
                // For this, we'll create a static string and use memcpy

                // Create a unique string constant name for the source data
                let string_name = format!("str_{}", *string_counter);
                *string_counter += 1;

                // Create string data (null-terminated for C compatibility)
                let mut string_data = s.as_bytes().to_vec();
                string_data.push(0); // null terminator

                // Declare data object for the source string
                let string_id = module.declare_data(&string_name, Linkage::Local, false, false)
                    .map_err(CodegenError::ModuleError)?;

                // Define the string data
                let mut data_desc = DataDescription::new();
                data_desc.define(string_data.into_boxed_slice());
                module.define_data(string_id, &data_desc)
                    .map_err(CodegenError::ModuleError)?;

                // Get a reference to the source string data
                let string_ref = module.declare_data_in_func(string_id, builder.func);
                let source_ptr = builder.ins().symbol_value(I64, string_ref);

                // Declare memcpy function
                let memcpy_sig = {
                    let mut sig = module.make_signature();
                    sig.call_conv = CallConv::SystemV;
                    sig.params.push(AbiParam::new(I64)); // dest
                    sig.params.push(AbiParam::new(I64)); // src
                    sig.params.push(AbiParam::new(I64)); // size
                    sig.returns.push(AbiParam::new(I64)); // returns dest
                    sig
                };

                let memcpy_id = module.declare_function("memcpy", Linkage::Import, &memcpy_sig)
                    .map_err(CodegenError::ModuleError)?;
                let memcpy_ref = module.declare_func_in_func(memcpy_id, builder.func);

                // Call memcpy to copy string data to GC memory
                builder.ins().call(memcpy_ref, &[string_ptr, source_ptr, size_val]);

                Ok(string_ptr)
            }
            Literal::InterpolatedString(parts, _) => {
                if parts.is_empty() {
                    // Empty interpolated string - create empty string constant
                    let string_name = format!("str_{}", *string_counter);
                    *string_counter += 1;

                    let string_data = vec![0u8]; // Just null terminator
                    let string_id = module.declare_data(&string_name, Linkage::Local, false, false)
                        .map_err(CodegenError::ModuleError)?;
                    let mut data_desc = DataDescription::new();
                    data_desc.define(string_data.into_boxed_slice());
                    module.define_data(string_id, &data_desc)
                        .map_err(CodegenError::ModuleError)?;

                    let string_ref = module.declare_data_in_func(string_id, builder.func);
                    return Ok(builder.ins().symbol_value(I64, string_ref));
                }

                // Build template with ${N} placeholders and collect expression values with their types
                let mut template = String::new();
                let mut expression_data = Vec::new(); // Store (value, expression) pairs
                let mut placeholder_count = 0;

                for part in parts {
                    match part {
                        ast::InterpolationPart::Text(text) => {
                            template.push_str(text);
                        }
                        ast::InterpolationPart::Expression(expr) => {
                            template.push_str(&format!("${{{}}}", placeholder_count));
                            placeholder_count += 1;

                            // Generate the expression value
                            let expr_val = Self::generate_expression_helper(
                                builder, expr, variables, variable_types, functions, module, string_counter, variable_counter
                            )?;
                            expression_data.push((expr_val, expr.as_ref()));
                        }
                    }
                }

                // Create template string constant
                let template_name = format!("str_{}", *string_counter);
                *string_counter += 1;
                let mut template_data = template.as_bytes().to_vec();
                template_data.push(0); // null terminator

                let template_id = module.declare_data(&template_name, Linkage::Local, false, false)
                    .map_err(CodegenError::ModuleError)?;
                let mut template_desc = DataDescription::new();
                template_desc.define(template_data.into_boxed_slice());
                module.define_data(template_id, &template_desc)
                    .map_err(CodegenError::ModuleError)?;

                let template_ref = module.declare_data_in_func(template_id, builder.func);
                let template_ptr = builder.ins().symbol_value(I64, template_ref);

                // Convert expression values to strings based on their original types
                let mut string_values = Vec::new();
                for (expr_val, expr) in expression_data {
                    let string_val = match expr {
                        // String literals and variables are already string pointers
                        Expression::Literal(Literal::String(_, _)) => expr_val,
                        Expression::Literal(Literal::InterpolatedString(_, _)) => expr_val,
                        Expression::Identifier { name, .. } => {
                            // Use the variable type information to determine conversion
                            match variable_types.get(name) {
                                Some(VariableType::String) => {
                                    // String variable, use directly
                                    expr_val
                                }
                                Some(VariableType::Array) => {
                                    // Array variable, convert to string representation
                                    let convert_sig = {
                                        let mut sig = module.make_signature();
                                        sig.call_conv = CallConv::SystemV;
                                        sig.params.push(AbiParam::new(I64));
                                        sig.returns.push(AbiParam::new(I64));
                                        sig
                                    };
                                    let convert_id = module.declare_function("plat_array_to_string", Linkage::Import, &convert_sig)
                                        .map_err(CodegenError::ModuleError)?;
                                    let convert_ref = module.declare_func_in_func(convert_id, builder.func);
                                    let call = builder.ins().call(convert_ref, &[expr_val]);
                                    builder.inst_results(call)[0]
                                }
                                Some(VariableType::Dict) => {
                                    // Dict variable, convert to string representation
                                    let convert_sig = {
                                        let mut sig = module.make_signature();
                                        sig.call_conv = CallConv::SystemV;
                                        sig.params.push(AbiParam::new(I64));
                                        sig.returns.push(AbiParam::new(I64));
                                        sig
                                    };
                                    let convert_id = module.declare_function("plat_dict_to_string", Linkage::Import, &convert_sig)
                                        .map_err(CodegenError::ModuleError)?;
                                    let convert_ref = module.declare_func_in_func(convert_id, builder.func);
                                    let call = builder.ins().call(convert_ref, &[expr_val]);
                                    builder.inst_results(call)[0]
                                }
                                Some(VariableType::Set) => {
                                    // Set variable, convert to string representation
                                    let convert_sig = {
                                        let mut sig = module.make_signature();
                                        sig.call_conv = CallConv::SystemV;
                                        sig.params.push(AbiParam::new(I64));
                                        sig.returns.push(AbiParam::new(I64));
                                        sig
                                    };
                                    let convert_id = module.declare_function("plat_set_to_string", Linkage::Import, &convert_sig)
                                        .map_err(CodegenError::ModuleError)?;
                                    let convert_ref = module.declare_func_in_func(convert_id, builder.func);
                                    let call = builder.ins().call(convert_ref, &[expr_val]);
                                    builder.inst_results(call)[0]
                                }
                                Some(VariableType::I32) | Some(VariableType::Bool) => {
                                    // I32/boolean variable, convert to string
                                    let convert_sig = {
                                        let mut sig = module.make_signature();
                                        sig.call_conv = CallConv::SystemV;
                                        sig.params.push(AbiParam::new(I32));
                                        sig.returns.push(AbiParam::new(I64));
                                        sig
                                    };
                                    let convert_id = module.declare_function("plat_i32_to_string", Linkage::Import, &convert_sig)
                                        .map_err(CodegenError::ModuleError)?;
                                    let convert_ref = module.declare_func_in_func(convert_id, builder.func);
                                    let call = builder.ins().call(convert_ref, &[expr_val]);
                                    builder.inst_results(call)[0]
                                }
                                Some(VariableType::I64) => {
                                    // I64 variable, convert to string
                                    let convert_sig = {
                                        let mut sig = module.make_signature();
                                        sig.call_conv = CallConv::SystemV;
                                        sig.params.push(AbiParam::new(I64));
                                        sig.returns.push(AbiParam::new(I64));
                                        sig
                                    };
                                    let convert_id = module.declare_function("plat_i64_to_string", Linkage::Import, &convert_sig)
                                        .map_err(CodegenError::ModuleError)?;
                                    let convert_ref = module.declare_func_in_func(convert_id, builder.func);
                                    let call = builder.ins().call(convert_ref, &[expr_val]);
                                    builder.inst_results(call)[0]
                                }
                                Some(VariableType::Enum(_)) => {
                                    // Enum variable, convert to string representation
                                    let convert_sig = {
                                        let mut sig = module.make_signature();
                                        sig.call_conv = CallConv::SystemV;
                                        sig.params.push(AbiParam::new(I64));
                                        sig.returns.push(AbiParam::new(I64));
                                        sig
                                    };
                                    let convert_id = module.declare_function("plat_enum_to_string", Linkage::Import, &convert_sig)
                                        .map_err(CodegenError::ModuleError)?;
                                    let convert_ref = module.declare_func_in_func(convert_id, builder.func);
                                    let call = builder.ins().call(convert_ref, &[expr_val]);
                                    builder.inst_results(call)[0]
                                }
                                Some(VariableType::Class(_)) => {
                                    // Class variable, convert to string representation
                                    let convert_sig = {
                                        let mut sig = module.make_signature();
                                        sig.call_conv = CallConv::SystemV;
                                        sig.params.push(AbiParam::new(I64));
                                        sig.returns.push(AbiParam::new(I64));
                                        sig
                                    };
                                    let convert_id = module.declare_function("plat_class_to_string", Linkage::Import, &convert_sig)
                                        .map_err(CodegenError::ModuleError)?;
                                    let convert_ref = module.declare_func_in_func(convert_id, builder.func);
                                    let call = builder.ins().call(convert_ref, &[expr_val]);
                                    builder.inst_results(call)[0]
                                }
                                None => {
                                    // Unknown variable type, fall back to runtime type detection
                                    let val_type = builder.func.dfg.value_type(expr_val);
                                    if val_type == I64 {
                                        // Assume it's a string pointer
                                        expr_val
                                    } else {
                                        // I32 value, convert to string
                                        let convert_sig = {
                                            let mut sig = module.make_signature();
                                            sig.call_conv = CallConv::SystemV;
                                            sig.params.push(AbiParam::new(I32));
                                            sig.returns.push(AbiParam::new(I64));
                                            sig
                                        };
                                        let convert_id = module.declare_function("plat_i32_to_string", Linkage::Import, &convert_sig)
                                            .map_err(CodegenError::ModuleError)?;
                                        let convert_ref = module.declare_func_in_func(convert_id, builder.func);
                                        let call = builder.ins().call(convert_ref, &[expr_val]);
                                        builder.inst_results(call)[0]
                                    }
                                }
                            }
                        }
                        // Array, Dict, and Set expressions need to be converted to strings
                        Expression::Literal(Literal::Array(_, _)) |
                        Expression::Literal(Literal::Dict(_, _)) |
                        Expression::Literal(Literal::Set(_, _)) |
                        Expression::Index { .. } => {
                            // Arrays, dicts, sets and indexing results - convert arrays/dicts/sets to strings, but indexing gives i32
                            let val_type = builder.func.dfg.value_type(expr_val);
                            if val_type == I64 {
                                // This is an array/dict/set pointer, convert to string
                                let convert_sig = {
                                    let mut sig = module.make_signature();
                                    sig.call_conv = CallConv::SystemV;
                                    sig.params.push(AbiParam::new(I64));
                                    sig.returns.push(AbiParam::new(I64));
                                    sig
                                };

                                // Choose the right conversion function based on expression type
                                let function_name = match expr {
                                    Expression::Literal(Literal::Dict(_, _)) => "plat_dict_to_string",
                                    Expression::Literal(Literal::Set(_, _)) => "plat_set_to_string",
                                    _ => "plat_array_to_string", // Arrays and other expressions
                                };

                                let convert_id = module.declare_function(function_name, Linkage::Import, &convert_sig)
                                    .map_err(CodegenError::ModuleError)?;
                                let convert_ref = module.declare_func_in_func(convert_id, builder.func);
                                let call = builder.ins().call(convert_ref, &[expr_val]);
                                builder.inst_results(call)[0]
                            } else {
                                // This is an i32 (from indexing), convert to string
                                let convert_sig = {
                                    let mut sig = module.make_signature();
                                    sig.call_conv = CallConv::SystemV;
                                    sig.params.push(AbiParam::new(I32));
                                    sig.returns.push(AbiParam::new(I64));
                                    sig
                                };
                                let convert_id = module.declare_function("plat_i32_to_string", Linkage::Import, &convert_sig)
                                    .map_err(CodegenError::ModuleError)?;
                                let convert_ref = module.declare_func_in_func(convert_id, builder.func);
                                let call = builder.ins().call(convert_ref, &[expr_val]);
                                builder.inst_results(call)[0]
                            }
                        }
                        _ => {
                            // For other expressions, check the runtime type
                            let val_type = builder.func.dfg.value_type(expr_val);
                            if val_type == I64 {
                                // Assume it's a string pointer
                                expr_val
                            } else {
                                // I32 value, convert to string
                                let convert_sig = {
                                    let mut sig = module.make_signature();
                                    sig.call_conv = CallConv::SystemV;
                                    sig.params.push(AbiParam::new(I32));
                                    sig.returns.push(AbiParam::new(I64));
                                    sig
                                };
                                let convert_id = module.declare_function("plat_i32_to_string", Linkage::Import, &convert_sig)
                                    .map_err(CodegenError::ModuleError)?;
                                let convert_ref = module.declare_func_in_func(convert_id, builder.func);
                                let call = builder.ins().call(convert_ref, &[expr_val]);
                                builder.inst_results(call)[0]
                            }
                        }
                    };
                    string_values.push(string_val);
                }

                if string_values.is_empty() {
                    // No expressions to interpolate, just return template
                    return Ok(template_ptr);
                }

                // Allocate array for string value pointers
                let ptr_size = std::mem::size_of::<*const c_char>();
                let array_size = ptr_size * string_values.len();
                let gc_alloc_sig = {
                    let mut sig = module.make_signature();
                    sig.call_conv = CallConv::SystemV;
                    sig.params.push(AbiParam::new(I64)); // size
                    sig.returns.push(AbiParam::new(I64)); // pointer
                    sig
                };

                let gc_alloc_id = module.declare_function("plat_gc_alloc", Linkage::Import, &gc_alloc_sig)
                    .map_err(CodegenError::ModuleError)?;
                let gc_alloc_ref = module.declare_func_in_func(gc_alloc_id, builder.func);

                let array_size_val = builder.ins().iconst(I64, array_size as i64);
                let call = builder.ins().call(gc_alloc_ref, &[array_size_val]);
                let values_array = builder.inst_results(call)[0];

                // Store string pointers in array
                for (i, string_val) in string_values.iter().enumerate() {
                    let offset = (i * ptr_size) as i64;
                    let ptr_addr = builder.ins().iadd_imm(values_array, offset);
                    builder.ins().store(cranelift_codegen::ir::MemFlags::new(), *string_val, ptr_addr, 0);
                }

                // Call string interpolation function
                let interpolate_sig = {
                    let mut sig = module.make_signature();
                    sig.call_conv = CallConv::SystemV;
                    sig.params.push(AbiParam::new(I64)); // template_ptr
                    sig.params.push(AbiParam::new(I64)); // values_ptr
                    sig.params.push(AbiParam::new(I64)); // values_count
                    sig.returns.push(AbiParam::new(I64)); // result string
                    sig
                };

                let interpolate_id = module.declare_function("plat_string_interpolate", Linkage::Import, &interpolate_sig)
                    .map_err(CodegenError::ModuleError)?;
                let interpolate_ref = module.declare_func_in_func(interpolate_id, builder.func);

                let count_val = builder.ins().iconst(I64, string_values.len() as i64);
                let call = builder.ins().call(interpolate_ref, &[template_ptr, values_array, count_val]);
                let result = builder.inst_results(call)[0];

                Ok(result)
            }
            Literal::Array(elements, _) => {
                // First, evaluate all elements
                let mut element_values = Vec::new();
                for element in elements {
                    let element_val = Self::generate_expression_helper(builder, element, variables, variable_types, functions, module, string_counter, variable_counter)?;
                    element_values.push(element_val);
                }

                // Create array literal on stack temporarily
                let count = elements.len() as i64;
                let element_size = std::mem::size_of::<i32>() as i64;
                let total_size = count * element_size;

                // Allocate stack space for temporary array data
                let stack_slot = builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, total_size as u32, 4));

                // Store each element to the stack array
                for (i, &element_val) in element_values.iter().enumerate() {
                    let offset = (i as i64) * element_size;
                    let addr = builder.ins().stack_addr(I64, stack_slot, offset as i32);
                    builder.ins().store(MemFlags::new(), element_val, addr, 0);
                }

                // Get pointer to stack array data
                let stack_addr = builder.ins().stack_addr(I64, stack_slot, 0);

                // Declare plat_array_create function
                let create_sig = {
                    let mut sig = module.make_signature();
                    sig.call_conv = CallConv::SystemV;
                    sig.params.push(AbiParam::new(I64)); // elements pointer
                    sig.params.push(AbiParam::new(I64)); // count
                    sig.returns.push(AbiParam::new(I64)); // array pointer
                    sig
                };

                let create_id = module.declare_function("plat_array_create", Linkage::Import, &create_sig)
                    .map_err(CodegenError::ModuleError)?;
                let create_ref = module.declare_func_in_func(create_id, builder.func);

                // Call plat_array_create with stack data and count
                let count_val = builder.ins().iconst(I64, count);
                let call = builder.ins().call(create_ref, &[stack_addr, count_val]);
                let array_ptr = builder.inst_results(call)[0];

                Ok(array_ptr)
            }
            Literal::Dict(pairs, _) => {
                // Process dict literal: {"key": value, "key2": value2}
                let count = pairs.len() as i64;

                if count == 0 {
                    // Empty dict
                    let create_sig = {
                        let mut sig = module.make_signature();
                        sig.call_conv = CallConv::SystemV;
                        sig.params.push(AbiParam::new(I64)); // keys pointer (null)
                        sig.params.push(AbiParam::new(I64)); // values pointer (null)
                        sig.params.push(AbiParam::new(I64)); // value_types pointer (null)
                        sig.params.push(AbiParam::new(I64)); // count (0)
                        sig.returns.push(AbiParam::new(I64)); // dict pointer
                        sig
                    };

                    let create_id = module.declare_function("plat_dict_create", Linkage::Import, &create_sig)
                        .map_err(CodegenError::ModuleError)?;
                    let create_ref = module.declare_func_in_func(create_id, builder.func);

                    let null_ptr = builder.ins().iconst(I64, 0);
                    let count_val = builder.ins().iconst(I64, 0);
                    let call = builder.ins().call(create_ref, &[null_ptr, null_ptr, null_ptr, count_val]);
                    return Ok(builder.inst_results(call)[0]);
                }

                // Generate arrays for keys, values, and value types
                let mut keys = Vec::new();
                let mut values = Vec::new();
                let mut value_types = Vec::new();

                for (key_expr, value_expr) in pairs {
                    // Evaluate key (must be string)
                    let key_val = Self::generate_expression_helper(builder, key_expr, variables, variable_types, functions, module, string_counter, variable_counter)?;
                    keys.push(key_val);

                    // Evaluate value
                    let value_val = Self::generate_expression_helper(builder, value_expr, variables, variable_types, functions, module, string_counter, variable_counter)?;
                    values.push(value_val);

                    // Determine value type (simplified - assuming i32 values for now)
                    let type_val = match value_expr {
                        Expression::Literal(Literal::Bool(_, _)) => 2u8, // DICT_VALUE_TYPE_BOOL
                        Expression::Literal(Literal::Integer(val, _)) => {
                            if *val > i32::MAX as i64 || *val < i32::MIN as i64 {
                                1u8 // DICT_VALUE_TYPE_I64
                            } else {
                                0u8 // DICT_VALUE_TYPE_I32
                            }
                        }
                        Expression::Literal(Literal::String(_, _)) => 3u8, // DICT_VALUE_TYPE_STRING
                        Expression::Literal(Literal::InterpolatedString(_, _)) => 3u8,
                        _ => 0u8, // default to i32
                    };
                    value_types.push(type_val);
                }

                // Create temporary arrays on stack for keys, values, and types
                let keys_size = count * 8; // i64 pointers
                let values_size = count * 8; // i64 values
                let types_size = count * 1; // u8 types

                let keys_slot = builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, keys_size as u32, 8));
                let values_slot = builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, values_size as u32, 8));
                let types_slot = builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, types_size as u32, 1));

                // Store keys, values, and types
                for (i, ((key_val, value_val), type_val)) in keys.iter().zip(values.iter()).zip(value_types.iter()).enumerate() {
                    let offset = (i * 8) as i32;
                    builder.ins().stack_store(*key_val, keys_slot, offset);
                    builder.ins().stack_store(*value_val, values_slot, offset);

                    let type_offset = i as i32;
                    let type_const = builder.ins().iconst(I32, *type_val as i64);
                    builder.ins().stack_store(type_const, types_slot, type_offset);
                }

                // Get stack addresses
                let keys_addr = builder.ins().stack_addr(I64, keys_slot, 0);
                let values_addr = builder.ins().stack_addr(I64, values_slot, 0);
                let types_addr = builder.ins().stack_addr(I64, types_slot, 0);

                // Call plat_dict_create
                let create_sig = {
                    let mut sig = module.make_signature();
                    sig.call_conv = CallConv::SystemV;
                    sig.params.push(AbiParam::new(I64)); // keys pointer
                    sig.params.push(AbiParam::new(I64)); // values pointer
                    sig.params.push(AbiParam::new(I64)); // value_types pointer
                    sig.params.push(AbiParam::new(I64)); // count
                    sig.returns.push(AbiParam::new(I64)); // dict pointer
                    sig
                };

                let create_id = module.declare_function("plat_dict_create", Linkage::Import, &create_sig)
                    .map_err(CodegenError::ModuleError)?;
                let create_ref = module.declare_func_in_func(create_id, builder.func);

                let count_val = builder.ins().iconst(I64, count);
                let call = builder.ins().call(create_ref, &[keys_addr, values_addr, types_addr, count_val]);
                let dict_ptr = builder.inst_results(call)[0];

                Ok(dict_ptr)
            }
            Literal::Set(elements, _) => {
                // Process set literal: Set{element1, element2, element3}
                let count = elements.len() as i64;

                if count == 0 {
                    // Empty set
                    let create_sig = {
                        let mut sig = module.make_signature();
                        sig.call_conv = CallConv::SystemV;
                        sig.params.push(AbiParam::new(I64)); // values pointer (null)
                        sig.params.push(AbiParam::new(I64)); // value_types pointer (null)
                        sig.params.push(AbiParam::new(I64)); // count (0)
                        sig.returns.push(AbiParam::new(I64)); // set pointer
                        sig
                    };

                    let create_id = module.declare_function("plat_set_create", Linkage::Import, &create_sig)
                        .map_err(CodegenError::ModuleError)?;
                    let create_ref = module.declare_func_in_func(create_id, builder.func);

                    let null_ptr = builder.ins().iconst(I64, 0);
                    let count_val = builder.ins().iconst(I64, 0);
                    let call = builder.ins().call(create_ref, &[null_ptr, null_ptr, count_val]);
                    return Ok(builder.inst_results(call)[0]);
                }

                // Generate arrays for values and value types
                let mut values = Vec::new();
                let mut value_types = Vec::new();

                for element_expr in elements {
                    // Evaluate element
                    let value_val = Self::generate_expression_helper(builder, element_expr, variables, variable_types, functions, module, string_counter, variable_counter)?;
                    values.push(value_val);

                    // Determine value type
                    let type_val = match element_expr {
                        Expression::Literal(Literal::Bool(_, _)) => 2u8, // SET_VALUE_TYPE_BOOL
                        Expression::Literal(Literal::Integer(val, _)) => {
                            if *val > i32::MAX as i64 || *val < i32::MIN as i64 {
                                1u8 // SET_VALUE_TYPE_I64
                            } else {
                                0u8 // SET_VALUE_TYPE_I32
                            }
                        }
                        Expression::Literal(Literal::String(_, _)) => 3u8, // SET_VALUE_TYPE_STRING
                        Expression::Literal(Literal::InterpolatedString(_, _)) => 3u8,
                        _ => 0u8, // default to i32
                    };
                    value_types.push(type_val);
                }

                // Create temporary arrays on stack for values and types
                let values_size = count * 8; // i64 values
                let types_size = count * 1; // u8 types

                let values_slot = builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, values_size as u32, 8));
                let types_slot = builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, types_size as u32, 1));

                // Store values and types
                for (i, (value_val, type_val)) in values.iter().zip(value_types.iter()).enumerate() {
                    let offset = (i * 8) as i32;
                    builder.ins().stack_store(*value_val, values_slot, offset);

                    let type_offset = i as i32;
                    let type_const = builder.ins().iconst(I32, *type_val as i64);
                    builder.ins().stack_store(type_const, types_slot, type_offset);
                }

                // Get stack addresses
                let values_addr = builder.ins().stack_addr(I64, values_slot, 0);
                let types_addr = builder.ins().stack_addr(I64, types_slot, 0);

                // Call plat_set_create
                let create_sig = {
                    let mut sig = module.make_signature();
                    sig.call_conv = CallConv::SystemV;
                    sig.params.push(AbiParam::new(I64)); // values pointer
                    sig.params.push(AbiParam::new(I64)); // value_types pointer
                    sig.params.push(AbiParam::new(I64)); // count
                    sig.returns.push(AbiParam::new(I64)); // set pointer
                    sig
                };

                let create_id = module.declare_function("plat_set_create", Linkage::Import, &create_sig)
                    .map_err(CodegenError::ModuleError)?;
                let create_ref = module.declare_func_in_func(create_id, builder.func);

                let count_val = builder.ins().iconst(I64, count);
                let call = builder.ins().call(create_ref, &[values_addr, types_addr, count_val]);
                let set_ptr = builder.inst_results(call)[0];

                Ok(set_ptr)
            }
        }
    }

    fn variant_discriminant(_enum_name: &str, variant_name: &str) -> u32 {
        // Simple hash function for variant discriminants
        // In a real implementation, this would be tracked per enum
        let mut hash = 0u32;
        for byte in variant_name.bytes() {
            hash = hash.wrapping_mul(31).wrapping_add(byte as u32);
        }
        // Ensure we use only the high 32 bits for discriminant
        hash
    }

    fn is_dict_type(expr: &Expression, variable_types: &HashMap<String, VariableType>) -> bool {
        match expr {
            Expression::Literal(Literal::Dict(_, _)) => true,
            Expression::Identifier { name, .. } => {
                // Look up variable type
                if let Some(var_type) = variable_types.get(name) {
                    matches!(var_type, VariableType::Dict)
                } else {
                    false
                }
            }
            _ => false
        }
    }

    fn get_dict_value_type(expr: &Expression, variable_types: &HashMap<String, VariableType>) -> u8 {
        // Import the constants from runtime
        const DICT_VALUE_TYPE_I32: u8 = 0;
        const DICT_VALUE_TYPE_I64: u8 = 1;
        const DICT_VALUE_TYPE_BOOL: u8 = 2;
        const DICT_VALUE_TYPE_STRING: u8 = 3;

        match expr {
            Expression::Literal(Literal::Integer(_, _)) => DICT_VALUE_TYPE_I32,
            Expression::Literal(Literal::String(_, _)) | Expression::Literal(Literal::InterpolatedString(_, _)) => DICT_VALUE_TYPE_STRING,
            Expression::Literal(Literal::Bool(_, _)) => DICT_VALUE_TYPE_BOOL,
            Expression::Identifier { name, .. } => {
                // Look up variable type
                if let Some(var_type) = variable_types.get(name) {
                    match var_type {
                        VariableType::I32 => DICT_VALUE_TYPE_I32,
                        VariableType::I64 => DICT_VALUE_TYPE_I64,
                        VariableType::Bool => DICT_VALUE_TYPE_BOOL,
                        VariableType::String => DICT_VALUE_TYPE_STRING,
                        _ => DICT_VALUE_TYPE_I64, // Default to i64
                    }
                } else {
                    DICT_VALUE_TYPE_I64 // Default
                }
            }
            _ => DICT_VALUE_TYPE_I64, // Default to i64
        }
    }

    fn is_set_type(expr: &Expression, variable_types: &HashMap<String, VariableType>) -> bool {
        match expr {
            Expression::Literal(Literal::Set(_, _)) => true,
            Expression::Identifier { name, .. } => {
                // Look up variable type
                if let Some(var_type) = variable_types.get(name) {
                    matches!(var_type, VariableType::Set)
                } else {
                    false
                }
            }
            _ => false
        }
    }

    fn is_list_type(expr: &Expression, variable_types: &HashMap<String, VariableType>) -> bool {
        match expr {
            Expression::Literal(Literal::Array(_, _)) => true,
            Expression::Identifier { name, .. } => {
                // Look up variable type
                if let Some(var_type) = variable_types.get(name) {
                    matches!(var_type, VariableType::Array)
                } else {
                    false
                }
            }
            _ => false
        }
    }

    fn is_class_type(expr: &Expression, variable_types: &HashMap<String, VariableType>) -> bool {
        match expr {
            Expression::ConstructorCall { .. } => true,
            Expression::Identifier { name, .. } => {
                // Look up variable type
                if let Some(var_type) = variable_types.get(name) {
                    matches!(var_type, VariableType::Class(_))
                } else {
                    false
                }
            }
            _ => false
        }
    }

    fn get_class_name(expr: &Expression, variable_types: &HashMap<String, VariableType>) -> Option<String> {
        match expr {
            Expression::ConstructorCall { class_name, .. } => Some(class_name.clone()),
            Expression::Identifier { name, .. } => {
                if let Some(VariableType::Class(class_name)) = variable_types.get(name) {
                    Some(class_name.clone())
                } else {
                    None
                }
            }
            _ => None
        }
    }

    fn get_set_value_type(expr: &Expression, variable_types: &HashMap<String, VariableType>) -> u8 {
        // Import the constants from runtime
        const SET_VALUE_TYPE_I32: u8 = 0;
        const SET_VALUE_TYPE_I64: u8 = 1;
        const SET_VALUE_TYPE_BOOL: u8 = 2;
        const SET_VALUE_TYPE_STRING: u8 = 3;

        match expr {
            Expression::Literal(Literal::Integer(_, _)) => SET_VALUE_TYPE_I32,
            Expression::Literal(Literal::String(_, _)) | Expression::Literal(Literal::InterpolatedString(_, _)) => SET_VALUE_TYPE_STRING,
            Expression::Literal(Literal::Bool(_, _)) => SET_VALUE_TYPE_BOOL,
            Expression::Identifier { name, .. } => {
                // Look up variable type
                if let Some(var_type) = variable_types.get(name) {
                    match var_type {
                        VariableType::I32 => SET_VALUE_TYPE_I32,
                        VariableType::I64 => SET_VALUE_TYPE_I64,
                        VariableType::Bool => SET_VALUE_TYPE_BOOL,
                        VariableType::String => SET_VALUE_TYPE_STRING,
                        _ => SET_VALUE_TYPE_I64, // Default to i64
                    }
                } else {
                    SET_VALUE_TYPE_I64 // Default
                }
            }
            _ => SET_VALUE_TYPE_I64, // Default to i64
        }
    }
}

#[derive(Debug)]
pub enum CodegenError {
    ModuleError(ModuleError),
    ObjectEmitError(object::write::Error),
    UnsupportedTarget,
    IsaCreationFailed,
    UnsupportedFeature(String),
    UndefinedVariable(String),
    UndefinedFunction(String),
    SettingsError(cranelift_codegen::settings::SetError),
}

impl From<cranelift_codegen::settings::SetError> for CodegenError {
    fn from(error: cranelift_codegen::settings::SetError) -> Self {
        CodegenError::SettingsError(error)
    }
}

impl std::fmt::Display for CodegenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CodegenError::ModuleError(e) => write!(f, "Module error: {}", e),
            CodegenError::ObjectEmitError(e) => write!(f, "Object emit error: {}", e),
            CodegenError::UnsupportedTarget => write!(f, "Unsupported target platform"),
            CodegenError::IsaCreationFailed => write!(f, "Failed to create ISA"),
            CodegenError::UnsupportedFeature(msg) => write!(f, "Unsupported feature: {}", msg),
            CodegenError::UndefinedVariable(name) => write!(f, "Undefined variable: {}", name),
            CodegenError::UndefinedFunction(name) => write!(f, "Undefined function: {}", name),
            CodegenError::SettingsError(e) => write!(f, "Settings error: {}", e),
        }
    }
}

impl std::error::Error for CodegenError {}