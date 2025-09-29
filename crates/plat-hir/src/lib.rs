#[cfg(test)]
mod tests;

use plat_ast::*;
use plat_diags::DiagnosticError;
use std::collections::HashMap;

pub struct TypeChecker {
    scopes: Vec<HashMap<String, HirType>>,
    functions: HashMap<String, FunctionSignature>,
    enums: HashMap<String, EnumInfo>,
    classes: HashMap<String, ClassInfo>,
    current_function_return_type: Option<HirType>,
    current_class_context: Option<String>, // Track which class we're currently type-checking
    current_method_is_init: bool, // Track if we're currently in an init method
    type_parameters: Vec<String>, // Track current type parameters in scope (like T, U)
    monomorphizer: Monomorphizer, // For generic type specialization
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HirType {
    Bool,
    I32,
    I64,
    String,
    List(Box<HirType>),
    Dict(Box<HirType>, Box<HirType>), // key type, value type
    Set(Box<HirType>), // element type
    Enum(String, Vec<HirType>), // name, type parameters
    Class(String, Vec<HirType>), // name, type parameters
    TypeParameter(String), // For generic type parameters like T, U, etc.
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

#[derive(Debug, Clone)]
pub struct ClassInfo {
    pub name: String,
    pub type_params: Vec<String>,
    pub parent_class: Option<String>, // None for no inheritance
    pub fields: HashMap<String, FieldInfo>, // field name -> field info
    pub methods: HashMap<String, FunctionSignature>,
    pub virtual_methods: HashMap<String, FunctionSignature>, // methods that can be overridden
}

#[derive(Debug, Clone)]
pub struct FieldInfo {
    pub ty: HirType,
    pub is_mutable: bool,
}

impl TypeChecker {
    pub fn new() -> Self {
        let mut checker = Self {
            scopes: vec![HashMap::new()], // Global scope
            functions: HashMap::new(),
            enums: HashMap::new(),
            classes: HashMap::new(),
            current_function_return_type: None,
            current_class_context: None,
            current_method_is_init: false,
            type_parameters: Vec::new(),
            monomorphizer: Monomorphizer::new(),
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

        // First phase: register all class names (without processing types)
        for class_decl in &program.classes {
            self.register_class_name(class_decl)?;
        }

        // Second phase: process class field types and method signatures
        for class_decl in &program.classes {
            self.collect_class_info(class_decl)?;
        }

        // Third phase: validate inheritance relationships
        for class_decl in &program.classes {
            self.validate_inheritance(class_decl)?;
        }

        // Second pass: collect all function signatures (including enum and class methods)
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

        // Note: Class method signatures are already collected in collect_class_info
        // to ensure type parameters are properly scoped

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

        // Type check class methods
        for class_decl in &program.classes {
            for method in &class_decl.methods {
                self.check_class_method(class_decl, method)?;
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

    fn register_class_name(&mut self, class_decl: &ClassDecl) -> Result<(), DiagnosticError> {
        // Just register the class name with empty info for now
        let class_info = ClassInfo {
            name: class_decl.name.clone(),
            type_params: class_decl.type_params.clone(),
            parent_class: class_decl.parent_class.clone(),
            fields: HashMap::new(),
            methods: HashMap::new(),
            virtual_methods: HashMap::new(),
        };

        if self.classes.insert(class_decl.name.clone(), class_info).is_some() {
            return Err(DiagnosticError::Type(
                format!("Class '{}' is defined multiple times", class_decl.name)
            ));
        }

        Ok(())
    }

    fn collect_class_info(&mut self, class_decl: &ClassDecl) -> Result<(), DiagnosticError> {
        // Add type parameters to scope
        let old_type_params = self.type_parameters.clone();
        self.type_parameters.extend(class_decl.type_params.iter().cloned());


        let mut fields = HashMap::new();
        let mut methods = HashMap::new();

        // Collect field information
        for field in &class_decl.fields {
            let field_type = self.ast_type_to_hir_type(&field.ty)?;
            let field_info = FieldInfo {
                ty: field_type,
                is_mutable: field.is_mutable,
            };

            if fields.insert(field.name.clone(), field_info).is_some() {
                return Err(DiagnosticError::Type(
                    format!("Field '{}' is defined multiple times in class '{}'", field.name, class_decl.name)
                ));
            }
        }

        // Collect method signatures
        for method in &class_decl.methods {
            let mut param_types = Vec::new();
            for param in &method.params {
                let param_type = self.ast_type_to_hir_type(&param.ty)?;
                param_types.push(param_type);
            }

            let return_type = match &method.return_type {
                Some(ty) => self.ast_type_to_hir_type(ty)?,
                None => HirType::Unit,
            };

            let signature = FunctionSignature {
                params: param_types,
                return_type,
                is_mutable: method.is_mutable,
            };

            // Store in class methods
            methods.insert(method.name.clone(), signature.clone());

            // Also store in global functions map with qualified name
            let method_name = format!("{}::{}", class_decl.name, method.name);
            if self.functions.insert(method_name, signature).is_some() {
                return Err(DiagnosticError::Type(
                    format!("Method '{}::{}' is defined multiple times", class_decl.name, method.name)
                ));
            }
        }

        // Separate virtual methods from regular methods
        let mut virtual_methods = HashMap::new();
        for method in &class_decl.methods {
            if method.is_virtual {
                let method_signature = methods.get(&method.name).unwrap().clone();
                virtual_methods.insert(method.name.clone(), method_signature);
            }
        }

        // Update the existing class info with the collected fields and methods
        let class_info = ClassInfo {
            name: class_decl.name.clone(),
            type_params: class_decl.type_params.clone(),
            parent_class: class_decl.parent_class.clone(),
            fields,
            methods,
            virtual_methods,
        };

        // Update the existing entry (it should exist from register_class_name)
        self.classes.insert(class_decl.name.clone(), class_info);

        // Restore previous type parameters
        self.type_parameters = old_type_params;

        Ok(())
    }

    fn validate_inheritance(&mut self, class_decl: &ClassDecl) -> Result<(), DiagnosticError> {
        if let Some(parent_name) = &class_decl.parent_class {
            // Check if parent class exists
            if !self.classes.contains_key(parent_name) {
                return Err(DiagnosticError::Type(
                    format!("Parent class '{}' not found for class '{}'", parent_name, class_decl.name)
                ));
            }

            // Check for circular inheritance
            let mut visited = std::collections::HashSet::new();
            let mut current = Some(parent_name.clone());
            visited.insert(class_decl.name.clone());

            while let Some(class_name) = current {
                if visited.contains(&class_name) {
                    return Err(DiagnosticError::Type(
                        format!("Circular inheritance detected involving class '{}'", class_decl.name)
                    ));
                }
                visited.insert(class_name.clone());

                current = self.classes.get(&class_name)
                    .and_then(|class_info| class_info.parent_class.clone());
            }

            // Validate method overrides
            let parent_class = self.classes.get(parent_name).unwrap().clone();
            for method in &class_decl.methods {
                if method.is_override {
                    // Method must override a virtual method in parent
                    if !parent_class.virtual_methods.contains_key(&method.name) {
                        return Err(DiagnosticError::Type(
                            format!("Method '{}' in class '{}' is marked override but parent '{}' has no virtual method '{}'",
                                method.name, class_decl.name, parent_name, method.name)
                        ));
                    }

                    // Signatures must match (for now, simplified check)
                    let parent_method = &parent_class.virtual_methods[&method.name];
                    let child_method_signature = self.classes[&class_decl.name].methods.get(&method.name).unwrap();

                    if parent_method.params.len() != child_method_signature.params.len() {
                        return Err(DiagnosticError::Type(
                            format!("Override method '{}' in class '{}' has different parameter count than parent method",
                                method.name, class_decl.name)
                        ));
                    }

                    // Return type must match
                    if parent_method.return_type != child_method_signature.return_type {
                        return Err(DiagnosticError::Type(
                            format!("Override method '{}' in class '{}' has different return type than parent method",
                                method.name, class_decl.name)
                        ));
                    }
                } else if parent_class.virtual_methods.contains_key(&method.name) && !method.is_virtual {
                    // Warning: overriding a virtual method without override keyword
                    // For now, we'll allow this but could be made stricter
                }
            }
        }

        // Check that non-override methods are not marked as override
        for method in &class_decl.methods {
            if method.is_override && class_decl.parent_class.is_none() {
                return Err(DiagnosticError::Type(
                    format!("Method '{}' in class '{}' is marked override but class has no parent",
                        method.name, class_decl.name)
                ));
            }
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
            Expression::Assignment { target, value, .. } => {
                let value_type = self.check_expression(value)?;

                match target.as_ref() {
                    Expression::Identifier { name, .. } => {
                        let variable_type = self.lookup_variable(name)?;

                        if value_type != variable_type {
                            return Err(DiagnosticError::Type(
                                format!("Assignment type mismatch: variable '{}' has type {:?}, assigned {:?}", name, variable_type, value_type)
                            ));
                        }
                    }
                    Expression::MemberAccess { object, member, .. } => {
                        // Check if we're assigning to a field of a class instance
                        let object_type = self.check_expression(object)?;

                        match &object_type {
                            HirType::Class(class_name, _) => {
                                let class_info = self.classes.get(class_name)
                                    .ok_or_else(|| DiagnosticError::Type(
                                        format!("Unknown class '{}'", class_name)
                                    ))?;

                                if let Some(field_info) = class_info.fields.get(member) {
                                    // Allow assignment to immutable fields if we're in an init method and assigning to self
                                    let is_self_assignment = match object.as_ref() {
                                        Expression::Self_ { .. } => true,
                                        _ => false,
                                    };

                                    if !field_info.is_mutable && !(self.current_method_is_init && is_self_assignment) {
                                        return Err(DiagnosticError::Type(
                                            format!("Cannot assign to immutable field '{}.{}'", class_name, member)
                                        ));
                                    }
                                    if value_type != field_info.ty {
                                        return Err(DiagnosticError::Type(
                                            format!("Assignment type mismatch: field '{}.{}' has type {:?}, assigned {:?}",
                                                   class_name, member, field_info.ty, value_type)
                                        ));
                                    }
                                } else {
                                    return Err(DiagnosticError::Type(
                                        format!("Class '{}' has no field '{}'", class_name, member)
                                    ));
                                }
                            }
                            _ => {
                                return Err(DiagnosticError::Type(
                                    format!("Member access assignment is only allowed on class instances, got {:?}", object_type)
                                ));
                            }
                        }
                    }
                    _ => {
                        return Err(DiagnosticError::Type(
                            "Invalid assignment target".to_string()
                        ));
                    }
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
                    (HirType::List(_), "length") => {
                        if !args.is_empty() {
                            return Err(DiagnosticError::Type(
                                "length() method takes no arguments".to_string()
                            ));
                        }
                        Ok(HirType::I32)
                    }
                    (HirType::List(element_type), "get") => {
                        if args.len() != 1 {
                            return Err(DiagnosticError::Type(
                                "get() method takes exactly one argument".to_string()
                            ));
                        }
                        let index_type = self.check_expression(&args[0])?;
                        if index_type != HirType::I32 {
                            return Err(DiagnosticError::Type(
                                format!("get() method expects i32 index, got {:?}", index_type)
                            ));
                        }
                        // Returns Option<T> where T is the element type
                        Ok(HirType::Enum("Option".to_string(), vec![(**element_type).clone()]))
                    }
                    (HirType::List(element_type), "set") => {
                        if args.len() != 2 {
                            return Err(DiagnosticError::Type(
                                "set() method takes exactly two arguments".to_string()
                            ));
                        }
                        let index_type = self.check_expression(&args[0])?;
                        let value_type = self.check_expression(&args[1])?;
                        if index_type != HirType::I32 {
                            return Err(DiagnosticError::Type(
                                format!("set() method expects i32 index, got {:?}", index_type)
                            ));
                        }
                        if value_type != **element_type {
                            return Err(DiagnosticError::Type(
                                format!("set() method expects value of type {:?}, got {:?}", element_type, value_type)
                            ));
                        }
                        Ok(HirType::Unit)
                    }
                    (HirType::List(element_type), "append") => {
                        if args.len() != 1 {
                            return Err(DiagnosticError::Type(
                                "append() method takes exactly one argument".to_string()
                            ));
                        }
                        let value_type = self.check_expression(&args[0])?;
                        if value_type != **element_type {
                            return Err(DiagnosticError::Type(
                                format!("append() method expects value of type {:?}, got {:?}", element_type, value_type)
                            ));
                        }
                        Ok(HirType::Unit)
                    }
                    (HirType::List(element_type), "insert_at") => {
                        if args.len() != 2 {
                            return Err(DiagnosticError::Type(
                                "insert_at() method takes exactly two arguments".to_string()
                            ));
                        }
                        let index_type = self.check_expression(&args[0])?;
                        let value_type = self.check_expression(&args[1])?;
                        if index_type != HirType::I32 {
                            return Err(DiagnosticError::Type(
                                format!("insert_at() method expects i32 index, got {:?}", index_type)
                            ));
                        }
                        if value_type != **element_type {
                            return Err(DiagnosticError::Type(
                                format!("insert_at() method expects value of type {:?}, got {:?}", element_type, value_type)
                            ));
                        }
                        Ok(HirType::Unit)
                    }
                    (HirType::List(element_type), "remove_at") => {
                        if args.len() != 1 {
                            return Err(DiagnosticError::Type(
                                "remove_at() method takes exactly one argument".to_string()
                            ));
                        }
                        let index_type = self.check_expression(&args[0])?;
                        if index_type != HirType::I32 {
                            return Err(DiagnosticError::Type(
                                format!("remove_at() method expects i32 index, got {:?}", index_type)
                            ));
                        }
                        // Returns Option<T> where T is the element type
                        Ok(HirType::Enum("Option".to_string(), vec![(**element_type).clone()]))
                    }
                    (HirType::List(_), "clear") => {
                        if !args.is_empty() {
                            return Err(DiagnosticError::Type(
                                "clear() method takes no arguments".to_string()
                            ));
                        }
                        Ok(HirType::Unit)
                    }
                    (HirType::List(element_type), "contains") => {
                        if args.len() != 1 {
                            return Err(DiagnosticError::Type(
                                "contains() method takes exactly one argument".to_string()
                            ));
                        }
                        let value_type = self.check_expression(&args[0])?;
                        if value_type != **element_type {
                            return Err(DiagnosticError::Type(
                                format!("contains() method expects value of type {:?}, got {:?}", element_type, value_type)
                            ));
                        }
                        Ok(HirType::Bool)
                    }
                    (HirType::List(element_type), "index_of") => {
                        if args.len() != 1 {
                            return Err(DiagnosticError::Type(
                                "index_of() method takes exactly one argument".to_string()
                            ));
                        }
                        let value_type = self.check_expression(&args[0])?;
                        if value_type != **element_type {
                            return Err(DiagnosticError::Type(
                                format!("index_of() method expects value of type {:?}, got {:?}", element_type, value_type)
                            ));
                        }
                        // Returns Option<i32>
                        Ok(HirType::Enum("Option".to_string(), vec![HirType::I32]))
                    }
                    (HirType::List(element_type), "count") => {
                        if args.len() != 1 {
                            return Err(DiagnosticError::Type(
                                "count() method takes exactly one argument".to_string()
                            ));
                        }
                        let value_type = self.check_expression(&args[0])?;
                        if value_type != **element_type {
                            return Err(DiagnosticError::Type(
                                format!("count() method expects value of type {:?}, got {:?}", element_type, value_type)
                            ));
                        }
                        Ok(HirType::I32)
                    }
                    (HirType::List(element_type), "slice") => {
                        if args.len() != 2 {
                            return Err(DiagnosticError::Type(
                                "slice() method takes exactly two arguments".to_string()
                            ));
                        }
                        let start_type = self.check_expression(&args[0])?;
                        let end_type = self.check_expression(&args[1])?;
                        if start_type != HirType::I32 {
                            return Err(DiagnosticError::Type(
                                format!("slice() method expects i32 start index, got {:?}", start_type)
                            ));
                        }
                        if end_type != HirType::I32 {
                            return Err(DiagnosticError::Type(
                                format!("slice() method expects i32 end index, got {:?}", end_type)
                            ));
                        }
                        // Returns List<T> where T is the element type
                        Ok(HirType::List(element_type.clone()))
                    }
                    (HirType::List(element_type), "concat") => {
                        if args.len() != 1 {
                            return Err(DiagnosticError::Type(
                                "concat() method takes exactly one argument".to_string()
                            ));
                        }
                        let other_type = self.check_expression(&args[0])?;
                        match other_type {
                            HirType::List(other_element_type) if *other_element_type == **element_type => {
                                Ok(HirType::List(element_type.clone()))
                            }
                            _ => Err(DiagnosticError::Type(
                                format!("concat() method expects List<{:?}>, got {:?}", element_type, other_type)
                            ))
                        }
                    }
                    (HirType::List(element_type), "all") => {
                        if args.len() != 1 {
                            return Err(DiagnosticError::Type(
                                "all() method takes exactly one argument".to_string()
                            ));
                        }
                        // For now, accept any function - in a more advanced type system,
                        // we'd check that it's a function T -> bool
                        let _predicate_type = self.check_expression(&args[0])?;
                        Ok(HirType::Bool)
                    }
                    (HirType::List(element_type), "any") => {
                        if args.len() != 1 {
                            return Err(DiagnosticError::Type(
                                "any() method takes exactly one argument".to_string()
                            ));
                        }
                        // For now, accept any function - in a more advanced type system,
                        // we'd check that it's a function T -> bool
                        let _predicate_type = self.check_expression(&args[0])?;
                        Ok(HirType::Bool)
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
                    // Dict methods
                    (HirType::Dict(key_type, value_type), "get") => {
                        if args.len() != 1 {
                            return Err(DiagnosticError::Type(
                                "get() method takes exactly one argument".to_string()
                            ));
                        }
                        let arg_type = self.check_expression(&args[0])?;
                        if arg_type != **key_type {
                            return Err(DiagnosticError::Type(
                                format!("get() method expects key of type {:?}, got {:?}", key_type, arg_type)
                            ));
                        }
                        Ok((**value_type).clone())
                    }
                    (HirType::Dict(key_type, value_type), "set") => {
                        if args.len() != 2 {
                            return Err(DiagnosticError::Type(
                                "set() method takes exactly two arguments".to_string()
                            ));
                        }
                        let key_arg_type = self.check_expression(&args[0])?;
                        if key_arg_type != **key_type {
                            return Err(DiagnosticError::Type(
                                format!("set() method expects key of type {:?}, got {:?}", key_type, key_arg_type)
                            ));
                        }
                        let value_arg_type = self.check_expression(&args[1])?;
                        if value_arg_type != **value_type {
                            return Err(DiagnosticError::Type(
                                format!("set() method expects value of type {:?}, got {:?}", value_type, value_arg_type)
                            ));
                        }
                        Ok(HirType::Bool)  // Returns true on success
                    }
                    (HirType::Dict(key_type, _value_type), "remove") => {
                        if args.len() != 1 {
                            return Err(DiagnosticError::Type(
                                "remove() method takes exactly one argument".to_string()
                            ));
                        }
                        let arg_type = self.check_expression(&args[0])?;
                        if arg_type != **key_type {
                            return Err(DiagnosticError::Type(
                                format!("remove() method expects key of type {:?}, got {:?}", key_type, arg_type)
                            ));
                        }
                        Ok(HirType::I64)  // Returns removed value or 0
                    }
                    (HirType::Dict(_, _), "clear") => {
                        if !args.is_empty() {
                            return Err(DiagnosticError::Type(
                                "clear() method takes no arguments".to_string()
                            ));
                        }
                        Ok(HirType::Unit)
                    }
                    (HirType::Dict(_, _), "length") => {
                        if !args.is_empty() {
                            return Err(DiagnosticError::Type(
                                "length() method takes no arguments".to_string()
                            ));
                        }
                        Ok(HirType::I32)
                    }
                    (HirType::Dict(_, _), "keys") => {
                        if !args.is_empty() {
                            return Err(DiagnosticError::Type(
                                "keys() method takes no arguments".to_string()
                            ));
                        }
                        // Returns List[string] - keys are always strings for now
                        Ok(HirType::List(Box::new(HirType::String)))
                    }
                    (HirType::Dict(_key_type, value_type), "values") => {
                        if !args.is_empty() {
                            return Err(DiagnosticError::Type(
                                "values() method takes no arguments".to_string()
                            ));
                        }
                        // Returns List of value type
                        Ok(HirType::List(value_type.clone()))
                    }
                    (HirType::Dict(key_type, _), "has_key") => {
                        if args.len() != 1 {
                            return Err(DiagnosticError::Type(
                                "has_key() method takes exactly one argument".to_string()
                            ));
                        }
                        let arg_type = self.check_expression(&args[0])?;
                        if arg_type != **key_type {
                            return Err(DiagnosticError::Type(
                                format!("has_key() method expects key of type {:?}, got {:?}", key_type, arg_type)
                            ));
                        }
                        Ok(HirType::Bool)
                    }
                    (HirType::Dict(_key_type, value_type), "has_value") => {
                        if args.len() != 1 {
                            return Err(DiagnosticError::Type(
                                "has_value() method takes exactly one argument".to_string()
                            ));
                        }
                        let arg_type = self.check_expression(&args[0])?;
                        if arg_type != **value_type {
                            return Err(DiagnosticError::Type(
                                format!("has_value() method expects value of type {:?}, got {:?}", value_type, arg_type)
                            ));
                        }
                        Ok(HirType::Bool)
                    }
                    (HirType::Dict(key_type, value_type), "merge") => {
                        if args.len() != 1 {
                            return Err(DiagnosticError::Type(
                                "merge() method takes exactly one argument".to_string()
                            ));
                        }
                        let arg_type = self.check_expression(&args[0])?;
                        let expected_type = HirType::Dict(key_type.clone(), value_type.clone());
                        if arg_type != expected_type {
                            return Err(DiagnosticError::Type(
                                format!("merge() method expects Dict of same type, got {:?}", arg_type)
                            ));
                        }
                        Ok(HirType::Unit)
                    }
                    (HirType::Dict(key_type, value_type), "get_or") => {
                        if args.len() != 2 {
                            return Err(DiagnosticError::Type(
                                "get_or() method takes exactly two arguments".to_string()
                            ));
                        }
                        let key_arg_type = self.check_expression(&args[0])?;
                        if key_arg_type != **key_type {
                            return Err(DiagnosticError::Type(
                                format!("get_or() method expects key of type {:?}, got {:?}", key_type, key_arg_type)
                            ));
                        }
                        let default_arg_type = self.check_expression(&args[1])?;
                        if default_arg_type != **value_type {
                            return Err(DiagnosticError::Type(
                                format!("get_or() method expects default value of type {:?}, got {:?}", value_type, default_arg_type)
                            ));
                        }
                        Ok((**value_type).clone())
                    }
                    // Set methods
                    (HirType::Set(element_type), "add") => {
                        if args.len() != 1 {
                            return Err(DiagnosticError::Type(
                                "add() method takes exactly one argument".to_string()
                            ));
                        }
                        let value_type = self.check_expression(&args[0])?;
                        if value_type != **element_type {
                            return Err(DiagnosticError::Type(
                                format!("add() method expects value of type {:?}, got {:?}", element_type, value_type)
                            ));
                        }
                        Ok(HirType::Bool)
                    }
                    (HirType::Set(element_type), "remove") => {
                        if args.len() != 1 {
                            return Err(DiagnosticError::Type(
                                "remove() method takes exactly one argument".to_string()
                            ));
                        }
                        let value_type = self.check_expression(&args[0])?;
                        if value_type != **element_type {
                            return Err(DiagnosticError::Type(
                                format!("remove() method expects value of type {:?}, got {:?}", element_type, value_type)
                            ));
                        }
                        Ok(HirType::Bool)
                    }
                    (HirType::Set(_), "clear") => {
                        if !args.is_empty() {
                            return Err(DiagnosticError::Type(
                                "clear() method takes no arguments".to_string()
                            ));
                        }
                        Ok(HirType::Unit)
                    }
                    (HirType::Set(_), "length") => {
                        if !args.is_empty() {
                            return Err(DiagnosticError::Type(
                                "length() method takes no arguments".to_string()
                            ));
                        }
                        Ok(HirType::I32)
                    }
                    (HirType::Set(element_type), "contains") => {
                        if args.len() != 1 {
                            return Err(DiagnosticError::Type(
                                "contains() method takes exactly one argument".to_string()
                            ));
                        }
                        let value_type = self.check_expression(&args[0])?;
                        if value_type != **element_type {
                            return Err(DiagnosticError::Type(
                                format!("contains() method expects value of type {:?}, got {:?}", element_type, value_type)
                            ));
                        }
                        Ok(HirType::Bool)
                    }
                    (HirType::Set(element_type), "union") => {
                        if args.len() != 1 {
                            return Err(DiagnosticError::Type(
                                "union() method takes exactly one argument".to_string()
                            ));
                        }
                        let other_type = self.check_expression(&args[0])?;
                        match other_type {
                            HirType::Set(other_element_type) if *other_element_type == **element_type => {
                                Ok(HirType::Set(element_type.clone()))
                            }
                            _ => Err(DiagnosticError::Type(
                                format!("union() method expects Set<{:?}>, got {:?}", element_type, other_type)
                            ))
                        }
                    }
                    (HirType::Set(element_type), "intersection") => {
                        if args.len() != 1 {
                            return Err(DiagnosticError::Type(
                                "intersection() method takes exactly one argument".to_string()
                            ));
                        }
                        let other_type = self.check_expression(&args[0])?;
                        match other_type {
                            HirType::Set(other_element_type) if *other_element_type == **element_type => {
                                Ok(HirType::Set(element_type.clone()))
                            }
                            _ => Err(DiagnosticError::Type(
                                format!("intersection() method expects Set<{:?}>, got {:?}", element_type, other_type)
                            ))
                        }
                    }
                    (HirType::Set(element_type), "difference") => {
                        if args.len() != 1 {
                            return Err(DiagnosticError::Type(
                                "difference() method takes exactly one argument".to_string()
                            ));
                        }
                        let other_type = self.check_expression(&args[0])?;
                        match other_type {
                            HirType::Set(other_element_type) if *other_element_type == **element_type => {
                                Ok(HirType::Set(element_type.clone()))
                            }
                            _ => Err(DiagnosticError::Type(
                                format!("difference() method expects Set<{:?}>, got {:?}", element_type, other_type)
                            ))
                        }
                    }
                    (HirType::Set(element_type), "is_subset_of") => {
                        if args.len() != 1 {
                            return Err(DiagnosticError::Type(
                                "is_subset_of() method takes exactly one argument".to_string()
                            ));
                        }
                        let other_type = self.check_expression(&args[0])?;
                        match other_type {
                            HirType::Set(other_element_type) if *other_element_type == **element_type => {
                                Ok(HirType::Bool)
                            }
                            _ => Err(DiagnosticError::Type(
                                format!("is_subset_of() method expects Set<{:?}>, got {:?}", element_type, other_type)
                            ))
                        }
                    }
                    (HirType::Set(element_type), "is_superset_of") => {
                        if args.len() != 1 {
                            return Err(DiagnosticError::Type(
                                "is_superset_of() method takes exactly one argument".to_string()
                            ));
                        }
                        let other_type = self.check_expression(&args[0])?;
                        match other_type {
                            HirType::Set(other_element_type) if *other_element_type == **element_type => {
                                Ok(HirType::Bool)
                            }
                            _ => Err(DiagnosticError::Type(
                                format!("is_superset_of() method expects Set<{:?}>, got {:?}", element_type, other_type)
                            ))
                        }
                    }
                    (HirType::Set(element_type), "is_disjoint_from") => {
                        if args.len() != 1 {
                            return Err(DiagnosticError::Type(
                                "is_disjoint_from() method takes exactly one argument".to_string()
                            ));
                        }
                        let other_type = self.check_expression(&args[0])?;
                        match other_type {
                            HirType::Set(other_element_type) if *other_element_type == **element_type => {
                                Ok(HirType::Bool)
                            }
                            _ => Err(DiagnosticError::Type(
                                format!("is_disjoint_from() method expects Set<{:?}>, got {:?}", element_type, other_type)
                            ))
                        }
                    }
                    // Class methods
                    (HirType::Class(class_name, _), method_name) => {
                        // Check if method exists in class
                        let class_info = self.classes.get(class_name)
                            .ok_or_else(|| DiagnosticError::Type(
                                format!("Unknown class '{}'", class_name)
                            ))?.clone();

                        if let Some(method_signature) = class_info.methods.get(method_name) {
                            // Check argument count (exclude implicit self parameter)
                            if args.len() != method_signature.params.len() {
                                return Err(DiagnosticError::Type(
                                    format!("Method '{}::{}' expects {} arguments, got {}",
                                           class_name, method_name, method_signature.params.len(), args.len())
                                ));
                            }

                            // Check argument types
                            for (i, (arg, expected_type)) in args.iter().zip(method_signature.params.iter()).enumerate() {
                                let arg_type = self.check_expression(arg)?;
                                if arg_type != *expected_type {
                                    return Err(DiagnosticError::Type(
                                        format!("Argument {} of method '{}::{}' has type {:?}, expected {:?}",
                                               i + 1, class_name, method_name, arg_type, expected_type)
                                    ));
                                }
                            }

                            Ok(method_signature.return_type.clone())
                        } else {
                            Err(DiagnosticError::Type(
                                format!("Class '{}' has no method '{}'", class_name, method_name)
                            ))
                        }
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
            Expression::Self_ { .. } => {
                // Check if we're in a class method context
                match &self.current_class_context {
                    Some(class_name) => {
                        // Return the class type (for now without generics)
                        Ok(HirType::Class(class_name.clone(), vec![]))
                    }
                    None => Err(DiagnosticError::Type(
                        "'self' can only be used within class methods".to_string()
                    ))
                }
            }
            Expression::MemberAccess { object, member, .. } => {
                let object_type = self.check_expression(object)?;

                match &object_type {
                    HirType::Class(class_name, _) => {
                        // Check if field exists in class
                        let class_info = self.classes.get(class_name)
                            .ok_or_else(|| DiagnosticError::Type(
                                format!("Unknown class '{}'", class_name)
                            ))?;

                        if let Some(field_info) = class_info.fields.get(member) {
                            Ok(field_info.ty.clone())
                        } else {
                            Err(DiagnosticError::Type(
                                format!("Class '{}' has no field '{}'", class_name, member)
                            ))
                        }
                    }
                    _ => Err(DiagnosticError::Type(
                        format!("Member access is only allowed on class instances, got {:?}", object_type)
                    ))
                }
            }
            Expression::ConstructorCall { class_name, args, .. } => {
                // Check if class exists
                let class_info = self.classes.get(class_name)
                    .ok_or_else(|| DiagnosticError::Type(
                        format!("Unknown class '{}'", class_name)
                    ))?.clone();

                // Check if init method exists
                if !class_info.methods.contains_key("init") {
                    return Err(DiagnosticError::Type(
                        format!("Class '{}' has no init method", class_name)
                    ));
                }

                let init_signature = &class_info.methods["init"];

                // Check argument count (exclude self parameter)
                if args.len() != init_signature.params.len() {
                    return Err(DiagnosticError::Type(
                        format!("Constructor for '{}' expects {} arguments, got {}",
                               class_name, init_signature.params.len(), args.len())
                    ));
                }

                // Check that all required fields are provided in named arguments
                let mut provided_fields = std::collections::HashSet::new();
                for arg in args {
                    provided_fields.insert(&arg.name);
                }

                for field_name in class_info.fields.keys() {
                    if !provided_fields.contains(field_name) {
                        return Err(DiagnosticError::Type(
                            format!("Constructor for '{}' missing required field '{}'", class_name, field_name)
                        ));
                    }
                }

                // Infer generic type parameters from constructor arguments
                let mut inferred_type_args = Vec::new();

                if !class_info.type_params.is_empty() {
                    // For generic classes, we need to infer type parameters from the constructor arguments
                    // This is a simplified approach - in practice, you might want more sophisticated inference

                    // Create a type substitution based on field types and argument types
                    let mut type_mapping = HashMap::new();

                    for arg in args {
                        if let Some(field_info) = class_info.fields.get(&arg.name) {
                            let arg_type = self.check_expression(&arg.value)?;

                            // Try to match field type with argument type to infer generics
                            if let HirType::TypeParameter(param_name) = &field_info.ty {
                                if !type_mapping.contains_key(param_name) {
                                    type_mapping.insert(param_name.clone(), arg_type.clone());
                                }
                            }
                        }
                    }

                    // Build type arguments from the mapping
                    for type_param in &class_info.type_params {
                        if let Some(concrete_type) = type_mapping.get(type_param) {
                            inferred_type_args.push(concrete_type.clone());
                        } else {
                            // Default to I32 if we can't infer the type
                            inferred_type_args.push(HirType::I32);
                        }
                    }
                }

                // Check argument types match field types (after substitution for generics)
                let substitution: TypeSubstitution = if !class_info.type_params.is_empty() {
                    class_info.type_params.iter()
                        .zip(inferred_type_args.iter())
                        .map(|(param, concrete)| (param.clone(), concrete.clone()))
                        .collect()
                } else {
                    HashMap::new()
                };

                for arg in args {
                    if let Some(field_info) = class_info.fields.get(&arg.name) {
                        let arg_type = self.check_expression(&arg.value)?;
                        let expected_type = field_info.ty.substitute_types(&substitution);

                        if arg_type != expected_type {
                            return Err(DiagnosticError::Type(
                                format!("Constructor argument '{}' has type {:?}, expected {:?}",
                                       arg.name, arg_type, expected_type)
                            ));
                        }
                    } else {
                        return Err(DiagnosticError::Type(
                            format!("Class '{}' has no field '{}'", class_name, arg.name)
                        ));
                    }
                }

                // If the class is generic, specialize it
                if !class_info.type_params.is_empty() {
                    let _specialized_name = self.monomorphizer.specialize_class(&class_info, &inferred_type_args)?;
                    // Return the generic class type with inferred type arguments
                    Ok(HirType::Class(class_name.clone(), inferred_type_args))
                } else {
                    // Return the non-generic class instance type
                    Ok(HirType::Class(class_name.clone(), vec![]))
                }
            }
            Expression::SuperCall { method, args, .. } => {
                // Check if we're in a class method context
                let current_class = self.current_class_context.as_ref()
                    .ok_or_else(|| DiagnosticError::Type(
                        "'super' can only be used within class methods".to_string()
                    ))?;

                // Get current class info
                let class_info = self.classes.get(current_class).unwrap();
                let parent_class_name = class_info.parent_class.as_ref()
                    .ok_or_else(|| DiagnosticError::Type(
                        format!("Class '{}' has no parent class for 'super' call", current_class)
                    ))?;

                // Get parent class info
                let parent_class = self.classes.get(parent_class_name)
                    .ok_or_else(|| DiagnosticError::Type(
                        format!("Parent class '{}' not found", parent_class_name)
                    ))?;

                // Check if method exists in parent and clone the signature
                let parent_method_signature = parent_class.methods.get(method)
                    .ok_or_else(|| DiagnosticError::Type(
                        format!("Parent class '{}' has no method '{}'", parent_class_name, method)
                    ))?.clone();

                // Check argument count (exclude implicit self parameter)
                if args.len() != parent_method_signature.params.len() {
                    return Err(DiagnosticError::Type(
                        format!("Super method '{}' expects {} arguments, got {}",
                               method, parent_method_signature.params.len(), args.len())
                    ));
                }

                // Check argument types
                for (i, (arg, expected_type)) in args.iter().zip(parent_method_signature.params.iter()).enumerate() {
                    let arg_type = self.check_expression(arg)?;
                    if arg_type != *expected_type {
                        return Err(DiagnosticError::Type(
                            format!("Argument {} of super method '{}' has type {:?}, expected {:?}",
                                   i + 1, method, arg_type, expected_type)
                        ));
                    }
                }

                Ok(parent_method_signature.return_type)
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
                // Check if this is a type parameter (T, U, etc.)
                if self.type_parameters.contains(name) {
                    // Type parameters shouldn't have their own type parameters
                    if !type_params.is_empty() {
                        return Err(DiagnosticError::Type(
                            format!("Type parameter '{}' cannot have type arguments", name)
                        ));
                    }
                    Ok(HirType::TypeParameter(name.clone()))
                }
                // Check if this is a known enum
                else if self.enums.contains_key(name) {
                    let type_args: Result<Vec<HirType>, DiagnosticError> = type_params
                        .iter()
                        .map(|param| self.ast_type_to_hir_type(param))
                        .collect();
                    Ok(HirType::Enum(name.clone(), type_args?))
                }
                // Check if this is a known class
                else if self.classes.contains_key(name) {
                    let type_args: Result<Vec<HirType>, DiagnosticError> = type_params
                        .iter()
                        .map(|param| self.ast_type_to_hir_type(param))
                        .collect();
                    Ok(HirType::Class(name.clone(), type_args?))
                }
                else {
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

    fn check_class_method(&mut self, class_decl: &ClassDecl, method: &Function) -> Result<(), DiagnosticError> {
        // Set up method scope with implicit self parameter
        self.push_scope();

        // Add class type parameters to scope
        let old_type_params = self.type_parameters.clone();
        self.type_parameters.extend(class_decl.type_params.iter().cloned());

        // Set current class context
        let old_class_context = self.current_class_context.clone();
        self.current_class_context = Some(class_decl.name.clone());

        // Set init method flag
        let old_is_init = self.current_method_is_init;
        self.current_method_is_init = method.name == "init";

        // Add self parameter of class type (with type parameters)
        let type_args: Vec<HirType> = class_decl.type_params.iter()
            .map(|param| HirType::TypeParameter(param.clone()))
            .collect();
        let self_type = HirType::Class(class_decl.name.clone(), type_args);
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

        // Special validation for init method
        if method.name == "init" {
            // Check that init method initializes all fields
            self.validate_init_method(class_decl, method)?;
        }

        // Set current function return type
        let old_return_type = self.current_function_return_type.clone();
        self.current_function_return_type = match &method.return_type {
            Some(ty) => Some(self.ast_type_to_hir_type(ty)?),
            None => Some(HirType::Unit),
        };

        // Check method body
        self.check_block(&method.body)?;

        // Restore previous state
        self.current_function_return_type = old_return_type;
        self.current_class_context = old_class_context;
        self.current_method_is_init = old_is_init;
        self.type_parameters = old_type_params;

        self.pop_scope();
        Ok(())
    }

    fn validate_init_method(&mut self, class_decl: &ClassDecl, init_method: &Function) -> Result<(), DiagnosticError> {
        // For now, just check that init method returns the class type or Unit
        match &init_method.return_type {
            Some(Type::Named(name, _)) if name == &class_decl.name => Ok(()),
            None => Ok(()), // Unit return is fine
            Some(_) => Err(DiagnosticError::Type(
                format!("Init method for class '{}' must return {} or nothing", class_decl.name, class_decl.name)
            )),
        }
    }
}

/// Type substitution for generic type parameters
/// Maps type parameter names (like "T", "U") to concrete types (like HirType::I32)
pub type TypeSubstitution = HashMap<String, HirType>;

/// Trait for types that can have type parameters substituted
pub trait TypeSubstitutable {
    fn substitute_types(&self, substitution: &TypeSubstitution) -> Self;
}

impl TypeSubstitutable for HirType {
    fn substitute_types(&self, substitution: &TypeSubstitution) -> Self {
        match self {
            HirType::TypeParameter(param_name) => {
                // Replace type parameter with concrete type if available
                substitution.get(param_name).cloned().unwrap_or_else(|| {
                    // If no substitution available, keep as type parameter
                    // This shouldn't happen in well-formed code after monomorphization
                    self.clone()
                })
            }
            HirType::List(element_type) => {
                HirType::List(Box::new(element_type.substitute_types(substitution)))
            }
            HirType::Dict(key_type, value_type) => {
                HirType::Dict(
                    Box::new(key_type.substitute_types(substitution)),
                    Box::new(value_type.substitute_types(substitution))
                )
            }
            HirType::Set(element_type) => {
                HirType::Set(Box::new(element_type.substitute_types(substitution)))
            }
            HirType::Enum(name, type_params) => {
                HirType::Enum(
                    name.clone(),
                    type_params.iter().map(|t| t.substitute_types(substitution)).collect()
                )
            }
            HirType::Class(name, type_params) => {
                HirType::Class(
                    name.clone(),
                    type_params.iter().map(|t| t.substitute_types(substitution)).collect()
                )
            }
            // Primitive types don't need substitution
            HirType::Bool | HirType::I32 | HirType::I64 | HirType::String | HirType::Unit => {
                self.clone()
            }
        }
    }
}

impl TypeSubstitutable for FieldInfo {
    fn substitute_types(&self, substitution: &TypeSubstitution) -> Self {
        FieldInfo {
            ty: self.ty.substitute_types(substitution),
            is_mutable: self.is_mutable,
        }
    }
}

impl TypeSubstitutable for FunctionSignature {
    fn substitute_types(&self, substitution: &TypeSubstitution) -> Self {
        FunctionSignature {
            params: self.params.iter().map(|t| t.substitute_types(substitution)).collect(),
            return_type: self.return_type.substitute_types(substitution),
            is_mutable: self.is_mutable,
        }
    }
}

/// Monomorphization: Generate specialized versions of generic types
pub struct Monomorphizer {
    /// Track which generic types have been instantiated with what concrete types
    /// Key: (generic_type_name, concrete_type_args), Value: specialized_name
    instantiations: HashMap<(String, Vec<HirType>), String>,

    /// Generated specialized types
    specialized_classes: HashMap<String, ClassInfo>,
    specialized_enums: HashMap<String, EnumInfo>,

    /// Counter for generating unique specialized names
    specialization_counter: usize,
}

impl Monomorphizer {
    pub fn new() -> Self {
        Self {
            instantiations: HashMap::new(),
            specialized_classes: HashMap::new(),
            specialized_enums: HashMap::new(),
            specialization_counter: 0,
        }
    }

    /// Get or create a specialized version of a generic class
    pub fn specialize_class(&mut self, class_info: &ClassInfo, type_args: &[HirType]) -> Result<String, DiagnosticError> {
        // If not generic, return original name
        if class_info.type_params.is_empty() {
            return Ok(class_info.name.clone());
        }

        // Check if we already specialized this combination
        let key = (class_info.name.clone(), type_args.to_vec());
        if let Some(specialized_name) = self.instantiations.get(&key) {
            return Ok(specialized_name.clone());
        }

        // Validate type argument count
        if class_info.type_params.len() != type_args.len() {
            return Err(DiagnosticError::Type(
                format!("Class '{}' expects {} type arguments, got {}",
                    class_info.name, class_info.type_params.len(), type_args.len())
            ));
        }

        // Create type substitution map
        let mut substitution = TypeSubstitution::new();
        for (param_name, concrete_type) in class_info.type_params.iter().zip(type_args.iter()) {
            substitution.insert(param_name.clone(), concrete_type.clone());
        }

        // Generate specialized name
        let specialized_name = format!("{}$specialized${}", class_info.name, self.specialization_counter);
        self.specialization_counter += 1;

        // Create specialized class info
        let specialized_fields: HashMap<String, FieldInfo> = class_info.fields.iter()
            .map(|(name, field)| (name.clone(), field.substitute_types(&substitution)))
            .collect();

        let specialized_methods: HashMap<String, FunctionSignature> = class_info.methods.iter()
            .map(|(name, sig)| (name.clone(), sig.substitute_types(&substitution)))
            .collect();

        let specialized_class = ClassInfo {
            name: specialized_name.clone(),
            type_params: vec![], // Specialized classes are not generic
            parent_class: class_info.parent_class.clone(),
            fields: specialized_fields,
            methods: specialized_methods,
            virtual_methods: HashMap::new(), // For now, specialized classes don't inherit virtuals
        };

        // Store the specialized class
        self.specialized_classes.insert(specialized_name.clone(), specialized_class);
        self.instantiations.insert(key, specialized_name.clone());

        Ok(specialized_name)
    }

    /// Get or create a specialized version of a generic enum
    pub fn specialize_enum(&mut self, enum_info: &EnumInfo, type_args: &[HirType]) -> Result<String, DiagnosticError> {
        // If not generic, return original name
        if enum_info.type_params.is_empty() {
            return Ok(enum_info.name.clone());
        }

        // Check if we already specialized this combination
        let key = (enum_info.name.clone(), type_args.to_vec());
        if let Some(specialized_name) = self.instantiations.get(&key) {
            return Ok(specialized_name.clone());
        }

        // Validate type argument count
        if enum_info.type_params.len() != type_args.len() {
            return Err(DiagnosticError::Type(
                format!("Enum '{}' expects {} type arguments, got {}",
                    enum_info.name, enum_info.type_params.len(), type_args.len())
            ));
        }

        // Create type substitution map
        let mut substitution = TypeSubstitution::new();
        for (param_name, concrete_type) in enum_info.type_params.iter().zip(type_args.iter()) {
            substitution.insert(param_name.clone(), concrete_type.clone());
        }

        // Generate specialized name
        let specialized_name = format!("{}$specialized${}", enum_info.name, self.specialization_counter);
        self.specialization_counter += 1;

        // Create specialized enum info
        let specialized_variants: HashMap<String, Vec<HirType>> = enum_info.variants.iter()
            .map(|(name, field_types)| {
                let specialized_fields = field_types.iter()
                    .map(|ty| ty.substitute_types(&substitution))
                    .collect();
                (name.clone(), specialized_fields)
            })
            .collect();

        let specialized_methods: HashMap<String, FunctionSignature> = enum_info.methods.iter()
            .map(|(name, sig)| (name.clone(), sig.substitute_types(&substitution)))
            .collect();

        let specialized_enum = EnumInfo {
            name: specialized_name.clone(),
            type_params: vec![], // Specialized enums are not generic
            variants: specialized_variants,
            methods: specialized_methods,
        };

        // Store the specialized enum
        self.specialized_enums.insert(specialized_name.clone(), specialized_enum);
        self.instantiations.insert(key, specialized_name.clone());

        Ok(specialized_name)
    }

    /// Get all specialized classes generated so far
    pub fn get_specialized_classes(&self) -> &HashMap<String, ClassInfo> {
        &self.specialized_classes
    }

    /// Get all specialized enums generated so far
    pub fn get_specialized_enums(&self) -> &HashMap<String, EnumInfo> {
        &self.specialized_enums
    }
}

impl TypeChecker {
    /// Get the monomorphized types after type checking
    pub fn get_monomorphized_types(self) -> (HashMap<String, ClassInfo>, HashMap<String, EnumInfo>) {
        (self.monomorphizer.specialized_classes, self.monomorphizer.specialized_enums)
    }

    /// Get a reference to the monomorphizer for debugging
    pub fn get_monomorphizer(&self) -> &Monomorphizer {
        &self.monomorphizer
    }
}