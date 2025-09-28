/// Cranelift-based code generation for the Plat language
/// Generates native machine code from the Plat AST

use plat_ast::{self as ast, *};
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
            Expression::EnumConstructor { enum_name, .. } => VariableType::Enum(enum_name.clone()),
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
            Statement::Let { name, value, .. } => {
                let val = Self::generate_expression_helper(builder, value, variables, variable_types, functions, module, string_counter, variable_counter)?;
                let var = Variable::from_u32(*variable_counter);
                *variable_counter += 1;

                // Determine Cranelift type and Plat type based on expression
                let (cranelift_type, plat_type) = match value {
                    Expression::Literal(Literal::String(_, _)) => (I64, VariableType::String),
                    Expression::Literal(Literal::InterpolatedString(_, _)) => (I64, VariableType::String),
                    Expression::Literal(Literal::Array(_, _)) => (I64, VariableType::Array),
                    Expression::Index { .. } => (I32, VariableType::I32), // Array indexing returns i32 elements
                    Expression::MethodCall { method, .. } if method == "len" => (I32, VariableType::I32), // len() returns i32
                    Expression::Literal(Literal::Bool(_, _)) => (I32, VariableType::Bool),
                    Expression::EnumConstructor { enum_name, .. } => (I64, VariableType::Enum(enum_name.clone())),
                    Expression::Match { arms, .. } => {
                        let match_type = Self::determine_match_return_type(arms, variable_types);
                        let cranelift_type = match match_type {
                            VariableType::String | VariableType::Array | VariableType::Enum(_) => I64,
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
            Statement::Var { name, value, .. } => {
                let val = Self::generate_expression_helper(builder, value, variables, variable_types, functions, module, string_counter, variable_counter)?;
                let var = Variable::from_u32(*variable_counter);
                *variable_counter += 1;

                // Determine Cranelift type and Plat type based on expression
                let (cranelift_type, plat_type) = match value {
                    Expression::Literal(Literal::String(_, _)) => (I64, VariableType::String),
                    Expression::Literal(Literal::InterpolatedString(_, _)) => (I64, VariableType::String),
                    Expression::Literal(Literal::Array(_, _)) => (I64, VariableType::Array),
                    Expression::Index { .. } => (I32, VariableType::I32), // Array indexing returns i32 elements
                    Expression::MethodCall { method, .. } if method == "len" => (I32, VariableType::I32), // len() returns i32
                    Expression::Literal(Literal::Bool(_, _)) => (I32, VariableType::Bool),
                    Expression::EnumConstructor { enum_name, .. } => (I64, VariableType::Enum(enum_name.clone())),
                    Expression::Match { arms, .. } => {
                        let match_type = Self::determine_match_return_type(arms, variable_types);
                        let cranelift_type = match match_type {
                            VariableType::String | VariableType::Array | VariableType::Enum(_) => I64,
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
                    sig.returns.push(AbiParam::new(I32)); // element value
                    sig
                };

                let get_id = module.declare_function("plat_array_get", Linkage::Import, &get_sig)
                    .map_err(CodegenError::ModuleError)?;
                let get_ref = module.declare_func_in_func(get_id, builder.func);

                let index_i64 = builder.ins().uextend(I64, current_index);
                let call = builder.ins().call(get_ref, &[array_val, index_i64]);
                let element_val = builder.inst_results(call)[0];

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
            Expression::Assignment { name, value, .. } => {
                let val = Self::generate_expression_helper(builder, value, variables, variable_types, functions, module, string_counter, variable_counter)?;
                if let Some(&var) = variables.get(name) {
                    builder.def_var(var, val);
                    Ok(val)
                } else {
                    Err(CodegenError::UndefinedVariable(name.clone()))
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
                    sig.returns.push(AbiParam::new(I32)); // element value
                    sig
                };

                let get_id = module.declare_function("plat_array_get", Linkage::Import, &get_sig)
                    .map_err(CodegenError::ModuleError)?;
                let get_ref = module.declare_func_in_func(get_id, builder.func);

                // Convert i32 index to i64 for function call
                let index_i64 = builder.ins().uextend(I64, index_val);

                // Call plat_array_get
                let call = builder.ins().call(get_ref, &[object_val, index_i64]);
                let result = builder.inst_results(call)[0];

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
                        Expression::Identifier { name, .. } => {
                            matches!(variable_types.get(name), Some(VariableType::String) | Some(VariableType::Array))
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
                    VariableType::String | VariableType::Array | VariableType::Enum(_) => I64,
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
            _ => {
                // TODO: Implement blocks, etc.
                Err(CodegenError::UnsupportedFeature("Complex expressions not yet implemented".to_string()))
            }
        }
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
                        // Array expressions need to be converted to strings
                        Expression::Literal(Literal::Array(_, _)) |
                        Expression::Index { .. } => {
                            // Arrays and indexing results - convert arrays to strings, but indexing gives i32
                            let val_type = builder.func.dfg.value_type(expr_val);
                            if val_type == I64 {
                                // This is an array pointer, convert to string
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