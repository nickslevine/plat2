/// Cranelift-based code generation for the Plat language
/// Generates native machine code from the Plat AST

use plat_ast::{self as ast, *};
use cranelift_codegen::ir::types::*;
use cranelift_codegen::ir::{
    AbiParam, Value, condcodes::IntCC,
};
use cranelift_codegen::isa::CallConv;
use cranelift_codegen::settings::{self, Configurable};
use cranelift_codegen::Context;
use cranelift_codegen::ir::InstBuilder;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_module::{Linkage, Module, ModuleError, FuncId, DataDescription};
use cranelift_object::{ObjectBuilder, ObjectModule};
use std::collections::HashMap;

pub struct CodeGenerator {
    module: ObjectModule,
    context: Context,
    functions: HashMap<String, FuncId>,
    string_counter: usize,
}

impl CodeGenerator {
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
        // First pass: declare all functions
        for function in &program.functions {
            self.declare_function(function)?;
        }

        // Second pass: generate code for all functions
        for function in &program.functions {
            self.generate_function(function)?;
        }

        // Finalize the module and return object code
        let object_product = self.module.finish();
        Ok(object_product.emit().map_err(CodegenError::ObjectEmitError)?)
    }

    fn declare_function(&mut self, function: &ast::Function) -> Result<(), CodegenError> {
        let mut sig = self.module.make_signature();

        // Set calling convention
        sig.call_conv = CallConv::SystemV;

        // Add parameters
        for _param in &function.params {
            // For now, treat all parameters as i32
            sig.params.push(AbiParam::new(I32));
        }

        // Add return type
        if let Some(_return_type) = &function.return_type {
            // For now, treat all returns as i32
            sig.returns.push(AbiParam::new(I32));
        } else if function.name == "main" {
            // Main function always returns i32 (exit code) even if not specified
            sig.returns.push(AbiParam::new(I32));
        }

        let func_id = self.module.declare_function(&function.name, Linkage::Export, &sig)
            .map_err(CodegenError::ModuleError)?;

        self.functions.insert(function.name.clone(), func_id);

        Ok(())
    }

    fn generate_function(&mut self, function: &ast::Function) -> Result<(), CodegenError> {
        let func_id = self.functions[&function.name];

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
        let mut variable_counter = 0u32;

        // Add function parameters as variables
        let params = builder.block_params(entry_block).to_vec();
        for (i, param) in function.params.iter().enumerate() {
            let var = Variable::from_u32(variable_counter);
            variable_counter += 1;
            builder.declare_var(var, I32);
            builder.def_var(var, params[i]);
            variables.insert(param.name.clone(), var);
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
        variable_counter: &mut u32,
        functions: &HashMap<String, FuncId>,
        module: &mut ObjectModule,
        string_counter: &mut usize
    ) -> Result<bool, CodegenError> {
        match statement {
            Statement::Let { name, value, .. } => {
                let val = Self::generate_expression_helper(builder, value, variables, functions, module, string_counter)?;
                let var = Variable::from_u32(*variable_counter);
                *variable_counter += 1;

                // Determine type based on expression
                let var_type = match value {
                    Expression::Literal(Literal::String(_, _)) => I64,
                    Expression::Literal(Literal::InterpolatedString(_, _)) => I64,
                    _ => I32,
                };

                builder.declare_var(var, var_type);
                builder.def_var(var, val);
                variables.insert(name.clone(), var);
                Ok(false)
            }
            Statement::Var { name, value, .. } => {
                let val = Self::generate_expression_helper(builder, value, variables, functions, module, string_counter)?;
                let var = Variable::from_u32(*variable_counter);
                *variable_counter += 1;

                // Determine type based on expression
                let var_type = match value {
                    Expression::Literal(Literal::String(_, _)) => I64,
                    Expression::Literal(Literal::InterpolatedString(_, _)) => I64,
                    _ => I32,
                };

                builder.declare_var(var, var_type);
                builder.def_var(var, val);
                variables.insert(name.clone(), var);
                Ok(false)
            }
            Statement::Return { value, .. } => {
                if let Some(expr) = value {
                    let val = Self::generate_expression_helper(builder, expr, variables, functions, module, string_counter)?;
                    builder.ins().return_(&[val]);
                } else {
                    builder.ins().return_(&[]);
                }
                Ok(true)
            }
            Statement::Expression(expr) => {
                Self::generate_expression_helper(builder, expr, variables, functions, module, string_counter)?;
                Ok(false)
            }
            Statement::Print { value, .. } => {
                // Generate the value to print
                let val = Self::generate_expression_helper(builder, value, variables, functions, module, string_counter)?;

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
            _ => {
                // TODO: Implement other statements (if, while, etc.)
                Err(CodegenError::UnsupportedFeature("Complex control flow not yet implemented".to_string()))
            }
        }
    }

    fn generate_expression_helper(
        builder: &mut FunctionBuilder,
        expr: &Expression,
        variables: &HashMap<String, Variable>,
        functions: &HashMap<String, FuncId>,
        module: &mut ObjectModule,
        string_counter: &mut usize
    ) -> Result<Value, CodegenError> {
        match expr {
            Expression::Literal(literal) => {
                Self::generate_literal(builder, literal, variables, functions, module, string_counter)
            }
            Expression::Identifier { name, .. } => {
                if let Some(&var) = variables.get(name) {
                    Ok(builder.use_var(var))
                } else {
                    Err(CodegenError::UndefinedVariable(name.clone()))
                }
            }
            Expression::Binary { left, op, right, .. } => {
                let left_val = Self::generate_expression_helper(builder, left, variables, functions, module, string_counter)?;
                let right_val = Self::generate_expression_helper(builder, right, variables, functions, module, string_counter)?;

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
                    _ => {
                        // TODO: Implement logical operators with short-circuiting
                        Err(CodegenError::UnsupportedFeature("Logical operators not yet implemented".to_string()))
                    }
                }
            }
            Expression::Unary { op, operand, .. } => {
                let operand_val = Self::generate_expression_helper(builder, operand, variables, functions, module, string_counter)?;

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
                let val = Self::generate_expression_helper(builder, value, variables, functions, module, string_counter)?;
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
                    let arg_val = Self::generate_expression_helper(builder, arg, variables, functions, module, string_counter)?;
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
        functions: &HashMap<String, FuncId>,
        module: &mut ObjectModule,
        string_counter: &mut usize
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
                // Create a unique string constant name
                let string_name = format!("str_{}", *string_counter);
                *string_counter += 1;

                // Create string data (null-terminated for C compatibility)
                let mut string_data = s.as_bytes().to_vec();
                string_data.push(0); // null terminator

                // Declare data object for the string
                let string_id = module.declare_data(&string_name, Linkage::Local, false, false)
                    .map_err(CodegenError::ModuleError)?;

                // Define the string data
                let mut data_desc = DataDescription::new();
                data_desc.define(string_data.into_boxed_slice());
                module.define_data(string_id, &data_desc)
                    .map_err(CodegenError::ModuleError)?;

                // Get a reference to the string data in the function
                let string_ref = module.declare_data_in_func(string_id, builder.func);
                let string_ptr = builder.ins().symbol_value(I64, string_ref);

                Ok(string_ptr)
            }
            Literal::InterpolatedString(parts, _) => {
                // For string interpolation, we need to:
                // 1. Generate each part (text or expression)
                // 2. Convert expressions to strings
                // 3. Concatenate all parts

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

                // For now, let's implement a simple approach: build the interpolated string
                // by creating a template with placeholders and collect the expression values
                let mut template = String::new();
                let mut expr_values = Vec::new();
                let mut placeholder_count = 0;

                for part in parts {
                    match part {
                        ast::InterpolationPart::Text(text) => {
                            template.push_str(text);
                        }
                        ast::InterpolationPart::Expression(expr) => {
                            template.push_str(&format!("${{{}}}", placeholder_count));
                            placeholder_count += 1;
                            let val = Self::generate_expression_helper(
                                builder, expr, variables, functions, module, string_counter
                            )?;
                            expr_values.push(val);
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

                // For now, return just the template pointer until we implement runtime interpolation
                // TODO: Call runtime interpolation function with template and values
                Ok(template_ptr)
            }
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