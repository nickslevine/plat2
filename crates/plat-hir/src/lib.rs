#[cfg(test)]
mod tests;

use plat_ast::*;
use plat_diags::DiagnosticError;
use std::collections::HashMap;

pub struct TypeChecker {
    scopes: Vec<HashMap<String, HirType>>,
    functions: HashMap<String, FunctionSignature>,
    current_function_return_type: Option<HirType>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HirType {
    Bool,
    I32,
    I64,
    String,
    List(Box<HirType>),
    Unit, // For functions that don't return anything
}

#[derive(Debug, Clone)]
pub struct FunctionSignature {
    pub params: Vec<HirType>,
    pub return_type: HirType,
}

impl TypeChecker {
    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()], // Global scope
            functions: HashMap::new(),
            current_function_return_type: None,
        }
    }

    pub fn check_program(mut self, program: &Program) -> Result<(), DiagnosticError> {
        // First pass: collect all function signatures
        for function in &program.functions {
            self.collect_function_signature(function)?;
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

        // Second pass: type check all functions
        for function in &program.functions {
            self.check_function(function)?;
        }

        Ok(())
    }

    fn collect_function_signature(&mut self, function: &Function) -> Result<(), DiagnosticError> {
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
        };

        if self.functions.insert(function.name.clone(), signature).is_some() {
            return Err(DiagnosticError::Type(
                format!("Function '{}' is defined multiple times", function.name)
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
                    (HirType::List(_), "len") => {
                        if !args.is_empty() {
                            return Err(DiagnosticError::Type(
                                "len() method takes no arguments".to_string()
                            ));
                        }
                        Ok(HirType::I32)
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
}