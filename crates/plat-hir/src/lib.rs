#[cfg(test)]
mod tests;

use plat_ast::*;
use plat_diags::DiagnosticError;
use std::collections::HashMap;

pub struct TypeChecker {
    scopes: Vec<HashMap<String, HirType>>,
    functions: HashMap<String, FunctionSignature>,
    enums: HashMap<String, EnumInfo>,
    current_function_return_type: Option<HirType>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HirType {
    Bool,
    I32,
    I64,
    String,
    List(Box<HirType>),
    Dict(Box<HirType>, Box<HirType>), // key type, value type
    Set(Box<HirType>), // element type
    Enum(String, Vec<HirType>), // name, type parameters
    Unit, // For functions that don't return anything
}

#[derive(Debug, Clone)]
pub struct FunctionSignature {
    pub params: Vec<HirType>,
    pub return_type: HirType,
    pub is_mutable: bool,
}

#[derive(Debug, Clone)]
pub struct EnumInfo {
    pub name: String,
    pub type_params: Vec<String>,
    pub variants: HashMap<String, Vec<HirType>>, // variant name -> field types
    pub methods: HashMap<String, FunctionSignature>,
}

#[derive(Debug, Clone)]
pub struct VariantInfo {
    pub enum_name: String,
    pub variant_name: String,
    pub field_types: Vec<HirType>,
}

impl TypeChecker {
    pub fn new() -> Self {
        let mut checker = Self {
            scopes: vec![HashMap::new()], // Global scope
            functions: HashMap::new(),
            enums: HashMap::new(),
            current_function_return_type: None,
        };

        // Register built-in Option<T> type
        checker.register_builtin_option();

        // Register built-in Result<T, E> type
        checker.register_builtin_result();

        checker
    }

    fn register_builtin_option(&mut self) {
        let mut variants = HashMap::new();
        // None variant has no fields
        variants.insert("None".to_string(), vec![]);
        // Some variant has one generic field T
        variants.insert("Some".to_string(), vec![HirType::Unit]); // Placeholder, will be replaced by actual type

        let option_info = EnumInfo {
            name: "Option".to_string(),
            type_params: vec!["T".to_string()],
            variants,
            methods: HashMap::new(),
        };

        self.enums.insert("Option".to_string(), option_info);
    }

    fn register_builtin_result(&mut self) {
        let mut variants = HashMap::new();
        // Ok variant has one generic field T
        variants.insert("Ok".to_string(), vec![HirType::Unit]); // Placeholder
        // Err variant has one generic field E
        variants.insert("Err".to_string(), vec![HirType::Unit]); // Placeholder

        let result_info = EnumInfo {
            name: "Result".to_string(),
            type_params: vec!["T".to_string(), "E".to_string()],
            variants,
            methods: HashMap::new(),
        };

        self.enums.insert("Result".to_string(), result_info);
    }

    pub fn check_program(mut self, program: &Program) -> Result<(), DiagnosticError> {
        // First pass: collect all enum definitions
        for enum_decl in &program.enums {
            self.collect_enum_info(enum_decl)?;
        }

        // Second pass: collect all function signatures (including enum methods)
        for function in &program.functions {
            self.collect_function_signature(function)?;
        }

        // Collect enum method signatures
        for enum_decl in &program.enums {
            for method in &enum_decl.methods {
                let method_name = format!("{}::{}", enum_decl.name, method.name);
                self.collect_function_signature_with_name(&method_name, method)?;
            }
        }

        // Check that main function exists
        if !self.functions.contains_key("main") {
            return Err(DiagnosticError::Type(
                "Program must have a main function".to_string()
            ));
        }

        // Validate main function signature
        let main_sig = &self.functions["main"];
        if !main_sig.params.is_empty() {
            return Err(DiagnosticError::Type(
                "Main function must have no parameters".to_string()
            ));
        }
        // Main can return either Unit or i32 (for exit code)
        if main_sig.return_type != HirType::Unit && main_sig.return_type != HirType::I32 {
            return Err(DiagnosticError::Type(
                "Main function must return either nothing or i32".to_string()
            ));
        }

        // Third pass: type check all functions
        for function in &program.functions {
            self.check_function(function)?;
        }

        // Type check enum methods
        for enum_decl in &program.enums {
            for method in &enum_decl.methods {
                self.check_enum_method(enum_decl, method)?;
            }
        }

        Ok(())
    }

    fn collect_enum_info(&mut self, enum_decl: &EnumDecl) -> Result<(), DiagnosticError> {
        let mut variants = HashMap::new();
        let mut methods = HashMap::new();

        // Collect variant information
        for variant in &enum_decl.variants {
            let field_types: Result<Vec<HirType>, DiagnosticError> = variant.fields
                .iter()
                .map(|field_type| self.ast_type_to_hir_type(field_type))
                .collect();

            let field_types = field_types?;

            variants.insert(variant.name.clone(), field_types);
        }

        // Collect method signatures
        for method in &enum_decl.methods {
            let param_types: Result<Vec<HirType>, DiagnosticError> = method.params
                .iter()
                .map(|param| self.ast_type_to_hir_type(&param.ty))
                .collect();

            let return_type = match &method.return_type {
                Some(ty) => self.ast_type_to_hir_type(ty)?,
                None => HirType::Unit,
            };

            let signature = FunctionSignature {
                params: param_types?,
                return_type,
                is_mutable: method.is_mutable,
            };

            methods.insert(method.name.clone(), signature);
        }

        let enum_info = EnumInfo {
            name: enum_decl.name.clone(),
            type_params: enum_decl.type_params.clone(),
            variants,
            methods,
        };

        if self.enums.insert(enum_decl.name.clone(), enum_info).is_some() {
            return Err(DiagnosticError::Type(
                format!("Enum '{}' is defined multiple times", enum_decl.name)
            ));
        }

        Ok(())
    }

    fn collect_function_signature(&mut self, function: &Function) -> Result<(), DiagnosticError> {
        self.collect_function_signature_with_name(&function.name, function)
    }

    fn collect_function_signature_with_name(&mut self, name: &str, function: &Function) -> Result<(), DiagnosticError> {
        let param_types: Result<Vec<HirType>, DiagnosticError> = function.params
            .iter()
            .map(|param| self.ast_type_to_hir_type(&param.ty))
            .collect();

        let return_type = match &function.return_type {
            Some(ty) => self.ast_type_to_hir_type(ty)?,
            None => HirType::Unit,
        };

        let signature = FunctionSignature {
            params: param_types?,
            return_type,
            is_mutable: function.is_mutable,
        };

        if self.functions.insert(name.to_string(), signature).is_some() {
            return Err(DiagnosticError::Type(
                format!("Function '{}' is defined multiple times", name)
            ));
        }

        Ok(())
    }

    fn check_function(&mut self, function: &Function) -> Result<(), DiagnosticError> {
        // Set up function scope
        self.push_scope();

        let signature = self.functions[&function.name].clone();
        self.current_function_return_type = Some(signature.return_type.clone());

        // Add parameters to scope
        for (param, param_type) in function.params.iter().zip(signature.params.iter()) {
            if self.scopes.last_mut().unwrap().insert(param.name.clone(), param_type.clone()).is_some() {
                return Err(DiagnosticError::Type(
                    format!("Parameter '{}' is defined multiple times", param.name)
                ));
            }
        }

        // Check function body
        self.check_block(&function.body)?;

        self.pop_scope();
        self.current_function_return_type = None;
        Ok(())
    }

    fn check_block(&mut self, block: &Block) -> Result<(), DiagnosticError> {
        for statement in &block.statements {
            self.check_statement(statement)?;
        }
        Ok(())
    }

    fn check_statement(&mut self, statement: &Statement) -> Result<(), DiagnosticError> {
        match statement {
            Statement::Let { name, ty, value, .. } => {
                let value_type = self.check_expression(value)?;

                // Check if explicit type matches inferred type
                if let Some(explicit_type) = ty {
                    let explicit_hir_type = self.ast_type_to_hir_type(explicit_type)?;
                    if explicit_hir_type != value_type {
                        return Err(DiagnosticError::Type(
                            format!("Type mismatch: expected {:?}, found {:?}", explicit_hir_type, value_type)
                        ));
                    }
                }

                // Check for shadowing (not allowed with let)
                if self.scopes.last().unwrap().contains_key(name) {
                    return Err(DiagnosticError::Type(
                        format!("Variable '{}' is already defined in this scope", name)
                    ));
                }

                self.scopes.last_mut().unwrap().insert(name.clone(), value_type);
            }
            Statement::Var { name, ty, value, .. } => {
                let value_type = self.check_expression(value)?;

                // Check if explicit type matches inferred type
                if let Some(explicit_type) = ty {
                    let explicit_hir_type = self.ast_type_to_hir_type(explicit_type)?;
                    if explicit_hir_type != value_type {
                        return Err(DiagnosticError::Type(
                            format!("Type mismatch: expected {:?}, found {:?}", explicit_hir_type, value_type)
                        ));
                    }
                }

                // Check for shadowing (not allowed with var)
                if self.scopes.last().unwrap().contains_key(name) {
                    return Err(DiagnosticError::Type(
                        format!("Variable '{}' is already defined in this scope", name)
                    ));
                }

                self.scopes.last_mut().unwrap().insert(name.clone(), value_type);
            }
            Statement::Expression(expr) => {
                self.check_expression(expr)?;
            }
            Statement::Return { value, .. } => {
                let return_type = match value {
                    Some(expr) => self.check_expression(expr)?,
                    None => HirType::Unit,
                };

                let expected_return_type = self.current_function_return_type.as_ref()
                    .ok_or_else(|| DiagnosticError::Type("Return statement outside function".to_string()))?;

                if return_type != *expected_return_type {
                    return Err(DiagnosticError::Type(
                        format!("Return type mismatch: expected {:?}, found {:?}", expected_return_type, return_type)
                    ));
                }
            }
            Statement::If { condition, then_branch, else_branch, .. } => {
                let condition_type = self.check_expression(condition)?;
                if condition_type != HirType::Bool {
                    return Err(DiagnosticError::Type(
                        format!("If condition must be boolean, found {:?}", condition_type)
                    ));
                }

                self.push_scope();
                self.check_block(then_branch)?;
                self.pop_scope();

                if let Some(else_block) = else_branch {
                    self.push_scope();
                    self.check_block(else_block)?;
                    self.pop_scope();
                }
            }
            Statement::While { condition, body, .. } => {
                let condition_type = self.check_expression(condition)?;
                if condition_type != HirType::Bool {
                    return Err(DiagnosticError::Type(
                        format!("While condition must be boolean, found {:?}", condition_type)
                    ));
                }

                self.push_scope();
                self.check_block(body)?;
                self.pop_scope();
            }
            Statement::For { variable, iterable, body, .. } => {
                let iterable_type = self.check_expression(iterable)?;

                // Extract element type from List
                let element_type = match iterable_type {
                    HirType::List(element_type) => *element_type,
                    _ => return Err(DiagnosticError::Type(
                        format!("For loop can only iterate over List types, found {:?}", iterable_type)
                    )),
                };

                // Create new scope for loop body and add loop variable
                self.push_scope();

                // Check if variable already exists in current scope
                if self.scopes.last().unwrap().contains_key(variable) {
                    return Err(DiagnosticError::Type(
                        format!("Loop variable '{}' is already defined in this scope", variable)
                    ));
                }

                self.scopes.last_mut().unwrap().insert(variable.clone(), element_type);
                self.check_block(body)?;
                self.pop_scope();
            }
            Statement::Print { value, .. } => {
                let value_type = self.check_expression(value)?;
                // Print accepts any type (will be converted to string)
                match value_type {
                    HirType::Bool | HirType::I32 | HirType::I64 | HirType::String => {},
                    _ => return Err(DiagnosticError::Type(
                        format!("Cannot print value of type {:?}", value_type)
                    )),
                }
            }
        }
        Ok(())
    }

    fn check_expression(&mut self, expression: &Expression) -> Result<HirType, DiagnosticError> {
        match expression {
            Expression::Literal(literal) => self.check_literal(literal),
            Expression::Identifier { name, .. } => {
                self.lookup_variable(name)
            }
            Expression::Binary { left, op, right, .. } => {
                let left_type = self.check_expression(left)?;
                let right_type = self.check_expression(right)?;
                self.check_binary_op(op, &left_type, &right_type)
            }
            Expression::Unary { op, operand, .. } => {
                let operand_type = self.check_expression(operand)?;
                self.check_unary_op(op, &operand_type)
            }
            Expression::Call { function, args, .. } => {
                let signature = self.functions.get(function)
                    .ok_or_else(|| DiagnosticError::Type(format!("Unknown function '{}'", function)))?
                    .clone();

                if args.len() != signature.params.len() {
                    return Err(DiagnosticError::Type(
                        format!("Function '{}' expects {} arguments, got {}", function, signature.params.len(), args.len())
                    ));
                }

                for (arg, expected_type) in args.iter().zip(signature.params.iter()) {
                    let arg_type = self.check_expression(arg)?;
                    if arg_type != *expected_type {
                        return Err(DiagnosticError::Type(
                            format!("Function '{}' expects argument of type {:?}, got {:?}", function, expected_type, arg_type)
                        ));
                    }
                }

                Ok(signature.return_type)
            }
            Expression::Assignment { name, value, .. } => {
                let value_type = self.check_expression(value)?;
                let variable_type = self.lookup_variable(name)?;

                if value_type != variable_type {
                    return Err(DiagnosticError::Type(
                        format!("Assignment type mismatch: variable '{}' has type {:?}, assigned {:?}", name, variable_type, value_type)
                    ));
                }

                Ok(HirType::Unit)
            }
            Expression::Index { object, index, .. } => {
                let object_type = self.check_expression(object)?;
                let index_type = self.check_expression(index)?;

                // Index must be i32
                if index_type != HirType::I32 {
                    return Err(DiagnosticError::Type(
                        format!("Array index must be i32, got {:?}", index_type)
                    ));
                }

                // Object must be List
                match object_type {
                    HirType::List(element_type) => Ok(*element_type),
                    _ => Err(DiagnosticError::Type(
                        format!("Cannot index into type {:?}", object_type)
                    ))
                }
            }
            Expression::MethodCall { object, method, args, .. } => {
                let object_type = self.check_expression(object)?;

                match (&object_type, method.as_str()) {
                    // Array methods
                    (HirType::List(_), "len") => {
                        if !args.is_empty() {
                            return Err(DiagnosticError::Type(
                                "len() method takes no arguments".to_string()
                            ));
                        }
                        Ok(HirType::I32)
                    }
                    // String methods
                    (HirType::String, "length") => {
                        if !args.is_empty() {
                            return Err(DiagnosticError::Type(
                                "length() method takes no arguments".to_string()
                            ));
                        }
                        Ok(HirType::I32)
                    }
                    (HirType::String, "concat") => {
                        if args.len() != 1 {
                            return Err(DiagnosticError::Type(
                                "concat() method takes exactly one argument".to_string()
                            ));
                        }
                        let arg_type = self.check_expression(&args[0])?;
                        if arg_type != HirType::String {
                            return Err(DiagnosticError::Type(
                                format!("concat() method expects string argument, got {:?}", arg_type)
                            ));
                        }
                        Ok(HirType::String)
                    }
                    (HirType::String, "contains") => {
                        if args.len() != 1 {
                            return Err(DiagnosticError::Type(
                                "contains() method takes exactly one argument".to_string()
                            ));
                        }
                        let arg_type = self.check_expression(&args[0])?;
                        if arg_type != HirType::String {
                            return Err(DiagnosticError::Type(
                                format!("contains() method expects string argument, got {:?}", arg_type)
                            ));
                        }
                        Ok(HirType::Bool)
                    }
                    (HirType::String, "starts_with") => {
                        if args.len() != 1 {
                            return Err(DiagnosticError::Type(
                                "starts_with() method takes exactly one argument".to_string()
                            ));
                        }
                        let arg_type = self.check_expression(&args[0])?;
                        if arg_type != HirType::String {
                            return Err(DiagnosticError::Type(
                                format!("starts_with() method expects string argument, got {:?}", arg_type)
                            ));
                        }
                        Ok(HirType::Bool)
                    }
                    (HirType::String, "ends_with") => {
                        if args.len() != 1 {
                            return Err(DiagnosticError::Type(
                                "ends_with() method takes exactly one argument".to_string()
                            ));
                        }
                        let arg_type = self.check_expression(&args[0])?;
                        if arg_type != HirType::String {
                            return Err(DiagnosticError::Type(
                                format!("ends_with() method expects string argument, got {:?}", arg_type)
                            ));
                        }
                        Ok(HirType::Bool)
                    }
                    (HirType::String, "trim") => {
                        if !args.is_empty() {
                            return Err(DiagnosticError::Type(
                                "trim() method takes no arguments".to_string()
                            ));
                        }
                        Ok(HirType::String)
                    }
                    (HirType::String, "trim_left") => {
                        if !args.is_empty() {
                            return Err(DiagnosticError::Type(
                                "trim_left() method takes no arguments".to_string()
                            ));
                        }
                        Ok(HirType::String)
                    }
                    (HirType::String, "trim_right") => {
                        if !args.is_empty() {
                            return Err(DiagnosticError::Type(
                                "trim_right() method takes no arguments".to_string()
                            ));
                        }
                        Ok(HirType::String)
                    }
                    (HirType::String, "replace") => {
                        if args.len() != 2 {
                            return Err(DiagnosticError::Type(
                                "replace() method takes exactly two arguments".to_string()
                            ));
                        }
                        let from_type = self.check_expression(&args[0])?;
                        let to_type = self.check_expression(&args[1])?;
                        if from_type != HirType::String || to_type != HirType::String {
                            return Err(DiagnosticError::Type(
                                format!("replace() method expects two string arguments, got {:?} and {:?}", from_type, to_type)
                            ));
                        }
                        Ok(HirType::String)
                    }
                    (HirType::String, "replace_all") => {
                        if args.len() != 2 {
                            return Err(DiagnosticError::Type(
                                "replace_all() method takes exactly two arguments".to_string()
                            ));
                        }
                        let from_type = self.check_expression(&args[0])?;
                        let to_type = self.check_expression(&args[1])?;
                        if from_type != HirType::String || to_type != HirType::String {
                            return Err(DiagnosticError::Type(
                                format!("replace_all() method expects two string arguments, got {:?} and {:?}", from_type, to_type)
                            ));
                        }
                        Ok(HirType::String)
                    }
                    (HirType::String, "split") => {
                        if args.len() != 1 {
                            return Err(DiagnosticError::Type(
                                "split() method takes exactly one argument".to_string()
                            ));
                        }
                        let arg_type = self.check_expression(&args[0])?;
                        if arg_type != HirType::String {
                            return Err(DiagnosticError::Type(
                                format!("split() method expects string argument, got {:?}", arg_type)
                            ));
                        }
                        Ok(HirType::List(Box::new(HirType::String)))
                    }
                    (HirType::String, "is_alpha") => {
                        if !args.is_empty() {
                            return Err(DiagnosticError::Type(
                                "is_alpha() method takes no arguments".to_string()
                            ));
                        }
                        Ok(HirType::Bool)
                    }
                    (HirType::String, "is_numeric") => {
                        if !args.is_empty() {
                            return Err(DiagnosticError::Type(
                                "is_numeric() method takes no arguments".to_string()
                            ));
                        }
                        Ok(HirType::Bool)
                    }
                    (HirType::String, "is_alphanumeric") => {
                        if !args.is_empty() {
                            return Err(DiagnosticError::Type(
                                "is_alphanumeric() method takes no arguments".to_string()
                            ));
                        }
                        Ok(HirType::Bool)
                    }
                    _ => Err(DiagnosticError::Type(
                        format!("Type {:?} has no method '{}'", object_type, method)
                    ))
                }
            }
            Expression::Block(block) => {
                self.push_scope();
                self.check_block(block)?;
                self.pop_scope();
                Ok(HirType::Unit)
            }
            Expression::EnumConstructor { enum_name, variant, args, .. } => {
                // Check if enum exists
                let enum_info = self.enums.get(enum_name)
                    .ok_or_else(|| DiagnosticError::Type(
                        format!("Unknown enum '{}'", enum_name)
                    ))?.clone();

                // Check if variant exists
                let variant_fields = enum_info.variants.get(variant)
                    .ok_or_else(|| DiagnosticError::Type(
                        format!("Enum '{}' has no variant '{}'", enum_name, variant)
                    ))?.clone();

                // For generic enums (Option, Result), infer type parameters from arguments
                let mut inferred_type_params = vec![];

                if enum_name == "Option" && variant == "Some" && args.len() == 1 {
                    // Option::Some(value) - infer T from value type
                    let arg_type = self.check_expression(&args[0])?;
                    inferred_type_params.push(arg_type.clone());

                    // Check argument count
                    if args.len() != 1 {
                        return Err(DiagnosticError::Type(
                            format!("Option::Some expects 1 argument, got {}", args.len())
                        ));
                    }
                } else if enum_name == "Option" && variant == "None" {
                    // Option::None - type will be inferred from context
                    // We'll use I32 as default for now (ideally this would be a type variable)
                    inferred_type_params.push(HirType::I32);

                    if args.len() != 0 {
                        return Err(DiagnosticError::Type(
                            format!("Option::None expects 0 arguments, got {}", args.len())
                        ));
                    }
                } else if enum_name == "Result" && variant == "Ok" && args.len() == 1 {
                    // Result::Ok(value) - infer T from value type
                    let arg_type = self.check_expression(&args[0])?;
                    inferred_type_params.push(arg_type.clone());
                    inferred_type_params.push(HirType::I32); // E defaults to I32

                    if args.len() != 1 {
                        return Err(DiagnosticError::Type(
                            format!("Result::Ok expects 1 argument, got {}", args.len())
                        ));
                    }
                } else if enum_name == "Result" && variant == "Err" && args.len() == 1 {
                    // Result::Err(error) - infer E from error type
                    let arg_type = self.check_expression(&args[0])?;
                    inferred_type_params.push(HirType::I32); // T defaults to I32
                    inferred_type_params.push(arg_type.clone());

                    if args.len() != 1 {
                        return Err(DiagnosticError::Type(
                            format!("Result::Err expects 1 argument, got {}", args.len())
                        ));
                    }
                } else {
                    // Non-generic enum or user-defined enum
                    // Check argument count
                    if args.len() != variant_fields.len() {
                        return Err(DiagnosticError::Type(
                            format!("Variant '{}::{}' expects {} arguments, got {}",
                                enum_name, variant, variant_fields.len(), args.len())
                        ));
                    }

                    // Check argument types
                    for (i, (arg, expected_type)) in args.iter().zip(variant_fields.iter()).enumerate() {
                        let arg_type = self.check_expression(arg)?;
                        // For built-in generic types, skip type checking here as we handle it above
                        if !enum_info.type_params.is_empty() && *expected_type == HirType::Unit {
                            continue;
                        }
                        if arg_type != *expected_type {
                            return Err(DiagnosticError::Type(
                                format!("Argument {} of variant '{}::{}' has type {:?}, expected {:?}",
                                    i + 1, enum_name, variant, arg_type, expected_type)
                            ));
                        }
                    }
                }

                // Return the enum type with inferred type parameters
                Ok(HirType::Enum(enum_name.clone(), inferred_type_params))
            }
            Expression::Match { value, arms, .. } => {
                let value_type = self.check_expression(value)?;

                // Ensure match value is an enum
                let (enum_name, _type_params) = match &value_type {
                    HirType::Enum(name, params) => (name.clone(), params.clone()),
                    _ => return Err(DiagnosticError::Type(
                        format!("Match expressions can only be used with enums, got {:?}", value_type)
                    ))
                };

                if arms.is_empty() {
                    return Err(DiagnosticError::Type(
                        "Match expression must have at least one arm".to_string()
                    ));
                }

                // Check all arms have consistent return type
                let mut result_type = None;
                let mut covered_variants = std::collections::HashSet::new();

                for arm in arms {
                    // Each arm gets its own scope for pattern bindings
                    self.push_scope();

                    // Type check the pattern
                    self.check_pattern(&arm.pattern, &value_type)?;

                    // Track covered variants for exhaustiveness checking
                    if let Pattern::EnumVariant { variant, .. } = &arm.pattern {
                        covered_variants.insert(variant.clone());
                    }

                    // Type check the arm body
                    let arm_type = self.check_expression(&arm.body)?;

                    // Pop the arm scope
                    self.pop_scope();

                    match &result_type {
                        None => result_type = Some(arm_type),
                        Some(expected) => {
                            if arm_type != *expected {
                                return Err(DiagnosticError::Type(
                                    format!("Match arm returns type {:?}, expected {:?}", arm_type, expected)
                                ));
                            }
                        }
                    }
                }

                // Check exhaustiveness
                let enum_variants: Vec<String> = self.enums[&enum_name].variants.keys().cloned().collect();
                for variant_name in &enum_variants {
                    if !covered_variants.contains(variant_name) {
                        return Err(DiagnosticError::Type(
                            format!("Match expression is not exhaustive: missing variant '{}'", variant_name)
                        ));
                    }
                }

                Ok(result_type.unwrap())
            }
            Expression::Try { expression, .. } => {
                let expr_type = self.check_expression(expression)?;

                // The ? operator only works on Option<T> and Result<T, E> types
                match &expr_type {
                    HirType::Enum(name, type_params) if name == "Option" => {
                        // Option::Some(T) -> T, Option::None -> early return None
                        // Function must return Option<T> or compatible type
                        if type_params.len() != 1 {
                            return Err(DiagnosticError::Type(
                                "Option type must have exactly one type parameter".to_string()
                            ));
                        }
                        // Return the inner type T
                        Ok(type_params[0].clone())
                    }
                    HirType::Enum(name, type_params) if name == "Result" => {
                        // Result::Ok(T) -> T, Result::Err(E) -> early return Err(E)
                        // Function must return Result<T, E> or compatible type
                        if type_params.len() != 2 {
                            return Err(DiagnosticError::Type(
                                "Result type must have exactly two type parameters".to_string()
                            ));
                        }
                        // Return the inner type T (success type)
                        Ok(type_params[0].clone())
                    }
                    _ => Err(DiagnosticError::Type(
                        format!("? operator can only be used with Option or Result types, got {:?}", expr_type)
                    ))
                }
            }
        }
    }

    fn check_literal(&mut self, literal: &Literal) -> Result<HirType, DiagnosticError> {
        match literal {
            Literal::Bool(_, _) => Ok(HirType::Bool),
            Literal::Integer(_, _) => Ok(HirType::I32), // Default integer type
            Literal::String(_, _) => Ok(HirType::String),
            Literal::InterpolatedString(_, _) => Ok(HirType::String),
            Literal::Array(elements, _) => {
                if elements.is_empty() {
                    return Err(DiagnosticError::Type(
                        "Cannot infer type of empty array literal. Use explicit type annotation.".to_string()
                    ));
                }

                // Check first element to determine type
                let first_type = self.check_expression(&elements[0])?;

                // Check all elements have the same type
                for (i, element) in elements.iter().enumerate().skip(1) {
                    let element_type = self.check_expression(element)?;
                    if element_type != first_type {
                        return Err(DiagnosticError::Type(
                            format!("Array element {} has type {:?}, expected {:?}", i, element_type, first_type)
                        ));
                    }
                }

                Ok(HirType::List(Box::new(first_type)))
            }
            Literal::Dict(pairs, _) => {
                if pairs.is_empty() {
                    return Err(DiagnosticError::Type(
                        "Cannot infer type of empty dict literal. Use explicit type annotation.".to_string()
                    ));
                }

                // Check first pair to determine key and value types
                let (first_key, first_value) = &pairs[0];
                let key_type = self.check_expression(first_key)?;
                let value_type = self.check_expression(first_value)?;

                // Check all pairs have consistent key and value types
                for (i, (key, value)) in pairs.iter().enumerate().skip(1) {
                    let current_key_type = self.check_expression(key)?;
                    let current_value_type = self.check_expression(value)?;

                    if current_key_type != key_type {
                        return Err(DiagnosticError::Type(
                            format!("Dict key {} has type {:?}, expected {:?}", i, current_key_type, key_type)
                        ));
                    }

                    if current_value_type != value_type {
                        return Err(DiagnosticError::Type(
                            format!("Dict value {} has type {:?}, expected {:?}", i, current_value_type, value_type)
                        ));
                    }
                }

                Ok(HirType::Dict(Box::new(key_type), Box::new(value_type)))
            }
            Literal::Set(elements, _) => {
                if elements.is_empty() {
                    return Err(DiagnosticError::Type(
                        "Cannot infer type of empty set literal. Use explicit type annotation.".to_string()
                    ));
                }

                // Check first element to determine element type
                let first_type = self.check_expression(&elements[0])?;

                // Check all elements have consistent type
                for (i, element) in elements.iter().enumerate().skip(1) {
                    let current_type = self.check_expression(element)?;
                    if current_type != first_type {
                        return Err(DiagnosticError::Type(
                            format!("Set element {} has type {:?}, expected {:?}", i, current_type, first_type)
                        ));
                    }
                }

                Ok(HirType::Set(Box::new(first_type)))
            }
        }
    }

    fn check_binary_op(&self, op: &BinaryOp, left: &HirType, right: &HirType) -> Result<HirType, DiagnosticError> {
        match op {
            BinaryOp::Add | BinaryOp::Subtract | BinaryOp::Multiply | BinaryOp::Divide | BinaryOp::Modulo => {
                match (left, right) {
                    (HirType::I32, HirType::I32) => Ok(HirType::I32),
                    (HirType::I64, HirType::I64) => Ok(HirType::I64),
                    (HirType::String, HirType::String) if matches!(op, BinaryOp::Add) => Ok(HirType::String),
                    _ => Err(DiagnosticError::Type(
                        format!("Cannot apply {:?} to types {:?} and {:?}", op, left, right)
                    ))
                }
            }
            BinaryOp::Equal | BinaryOp::NotEqual => {
                if left == right {
                    Ok(HirType::Bool)
                } else {
                    Err(DiagnosticError::Type(
                        format!("Cannot compare types {:?} and {:?}", left, right)
                    ))
                }
            }
            BinaryOp::Less | BinaryOp::LessEqual | BinaryOp::Greater | BinaryOp::GreaterEqual => {
                match (left, right) {
                    (HirType::I32, HirType::I32) | (HirType::I64, HirType::I64) => Ok(HirType::Bool),
                    _ => Err(DiagnosticError::Type(
                        format!("Cannot compare types {:?} and {:?}", left, right)
                    ))
                }
            }
            BinaryOp::And | BinaryOp::Or => {
                match (left, right) {
                    (HirType::Bool, HirType::Bool) => Ok(HirType::Bool),
                    _ => Err(DiagnosticError::Type(
                        format!("Logical operators require boolean operands, got {:?} and {:?}", left, right)
                    ))
                }
            }
        }
    }

    fn check_unary_op(&self, op: &UnaryOp, operand: &HirType) -> Result<HirType, DiagnosticError> {
        match op {
            UnaryOp::Not => {
                match operand {
                    HirType::Bool => Ok(HirType::Bool),
                    _ => Err(DiagnosticError::Type(
                        format!("Cannot apply 'not' to type {:?}", operand)
                    ))
                }
            }
            UnaryOp::Negate => {
                match operand {
                    HirType::I32 => Ok(HirType::I32),
                    HirType::I64 => Ok(HirType::I64),
                    _ => Err(DiagnosticError::Type(
                        format!("Cannot negate type {:?}", operand)
                    ))
                }
            }
        }
    }

    fn ast_type_to_hir_type(&self, ast_type: &Type) -> Result<HirType, DiagnosticError> {
        match ast_type {
            Type::Bool => Ok(HirType::Bool),
            Type::I32 => Ok(HirType::I32),
            Type::I64 => Ok(HirType::I64),
            Type::String => Ok(HirType::String),
            Type::List(element_type) => {
                let element_hir_type = self.ast_type_to_hir_type(element_type)?;
                Ok(HirType::List(Box::new(element_hir_type)))
            }
            Type::Dict(key_type, value_type) => {
                let key_hir_type = self.ast_type_to_hir_type(key_type)?;
                let value_hir_type = self.ast_type_to_hir_type(value_type)?;
                Ok(HirType::Dict(Box::new(key_hir_type), Box::new(value_hir_type)))
            }
            Type::Set(element_type) => {
                let element_hir_type = self.ast_type_to_hir_type(element_type)?;
                Ok(HirType::Set(Box::new(element_hir_type)))
            }
            Type::Named(name, type_params) => {
                // Check if this is a known enum
                if self.enums.contains_key(name) {
                    let type_args: Result<Vec<HirType>, DiagnosticError> = type_params
                        .iter()
                        .map(|param| self.ast_type_to_hir_type(param))
                        .collect();
                    Ok(HirType::Enum(name.clone(), type_args?))
                } else {
                    Err(DiagnosticError::Type(
                        format!("Unknown type '{}'", name)
                    ))
                }
            }
        }
    }

    fn lookup_variable(&self, name: &str) -> Result<HirType, DiagnosticError> {
        // Search from innermost to outermost scope
        for scope in self.scopes.iter().rev() {
            if let Some(ty) = scope.get(name) {
                return Ok(ty.clone());
            }
        }

        Err(DiagnosticError::Type(format!("Undefined variable '{}'", name)))
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn check_pattern(&mut self, pattern: &Pattern, expected_type: &HirType) -> Result<(), DiagnosticError> {
        match pattern {
            Pattern::EnumVariant { enum_name, variant, bindings, .. } => {
                // Get expected enum name
                let expected_enum = match expected_type {
                    HirType::Enum(name, _) => name,
                    _ => return Err(DiagnosticError::Type(
                        format!("Pattern expects enum type, got {:?}", expected_type)
                    ))
                };

                // Check enum name matches (if specified)
                if let Some(pattern_enum) = enum_name {
                    if pattern_enum != expected_enum {
                        return Err(DiagnosticError::Type(
                            format!("Pattern expects enum '{}', got '{}'", expected_enum, pattern_enum)
                        ));
                    }
                }

                // Check variant exists and get field types
                let variant_fields = {
                    let enum_info = &self.enums[expected_enum];
                    enum_info.variants.get(variant)
                        .ok_or_else(|| DiagnosticError::Type(
                            format!("Enum '{}' has no variant '{}'", expected_enum, variant)
                        ))?.clone()
                };

                // For generic built-in types, infer the actual field types from the expected type
                let actual_field_types = if expected_enum == "Option" {
                    if let HirType::Enum(_, type_params) = expected_type {
                        if variant == "Some" && !type_params.is_empty() {
                            vec![type_params[0].clone()]
                        } else {
                            vec![]
                        }
                    } else {
                        variant_fields
                    }
                } else if expected_enum == "Result" {
                    if let HirType::Enum(_, type_params) = expected_type {
                        if variant == "Ok" && type_params.len() >= 1 {
                            vec![type_params[0].clone()]
                        } else if variant == "Err" && type_params.len() >= 2 {
                            vec![type_params[1].clone()]
                        } else {
                            vec![]
                        }
                    } else {
                        variant_fields
                    }
                } else {
                    variant_fields
                };

                // Check binding count matches field count
                if bindings.len() != actual_field_types.len() {
                    return Err(DiagnosticError::Type(
                        format!("Variant '{}::{}' has {} fields, but pattern has {} bindings",
                            expected_enum, variant, actual_field_types.len(), bindings.len())
                    ));
                }

                // Add bindings to current scope
                for (binding, field_type) in bindings.iter().zip(actual_field_types.iter()) {
                    if self.scopes.last().unwrap().contains_key(binding) {
                        return Err(DiagnosticError::Type(
                            format!("Variable '{}' is already bound in this pattern", binding)
                        ));
                    }
                    self.scopes.last_mut().unwrap().insert(binding.clone(), field_type.clone());
                }

                Ok(())
            }
            Pattern::Identifier { name, .. } => {
                // Simple binding pattern
                self.scopes.last_mut().unwrap().insert(name.clone(), expected_type.clone());
                Ok(())
            }
            Pattern::Literal(literal) => {
                // Check literal type matches expected type
                let literal_type = self.check_literal(literal)?;
                if literal_type != *expected_type {
                    return Err(DiagnosticError::Type(
                        format!("Literal pattern has type {:?}, expected {:?}", literal_type, expected_type)
                    ));
                }
                Ok(())
            }
        }
    }

    fn check_enum_method(&mut self, enum_decl: &EnumDecl, method: &Function) -> Result<(), DiagnosticError> {
        // Set up method scope with implicit self parameter
        self.push_scope();

        // Add self parameter of enum type
        let self_type = HirType::Enum(enum_decl.name.clone(), vec![]); // For now, no generics
        self.scopes.last_mut().unwrap().insert("self".to_string(), self_type);

        // Add method parameters
        for param in &method.params {
            let param_type = self.ast_type_to_hir_type(&param.ty)?;
            if self.scopes.last().unwrap().contains_key(&param.name) {
                return Err(DiagnosticError::Type(
                    format!("Parameter '{}' shadows another variable", param.name)
                ));
            }
            self.scopes.last_mut().unwrap().insert(param.name.clone(), param_type);
        }

        // Set current function return type
        let old_return_type = self.current_function_return_type.clone();
        self.current_function_return_type = match &method.return_type {
            Some(ty) => Some(self.ast_type_to_hir_type(ty)?),
            None => Some(HirType::Unit),
        };

        // Check method body
        self.check_block(&method.body)?;

        // Restore previous return type
        self.current_function_return_type = old_return_type;

        self.pop_scope();
        Ok(())
    }
}