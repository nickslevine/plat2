/// Cranelift-based code generation for the Plat language
/// Generates native machine code from the Plat AST

use plat_ast::{self as ast, BinaryOp, Block, Expression, IntType, Literal, MatchArm, Pattern, Program, Statement, UnaryOp, FloatType};
use plat_ast::Type as AstType;
use cranelift_codegen::ir::types::*;
use std::os::raw::c_char;
use cranelift_codegen::ir::{
    AbiParam, Value, condcodes::{IntCC, FloatCC}, StackSlotData, StackSlotKind, MemFlags,
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
    Int8,
    Int16,
    Int32,
    Int64,
    Float8,
    Float16,
    Float32,
    Float64,
    String,
    Array(Box<VariableType>), // Array with element type
    Dict,
    Set,
    Class(String), // class name
    Enum(String), // enum name
    Task(Box<VariableType>), // Task<T> with inner type
}

/// Metadata about a class field
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct ClassField {
    name: String,
    ty: AstType,
    offset: i32,
    cranelift_type: Type,
}

/// Metadata about a virtual method in a class
#[derive(Debug, Clone)]
struct VirtualMethod {
    name: String,
    vtable_index: usize,
    func_id: Option<FuncId>,
}

/// Metadata about a class definition
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct ClassMetadata {
    name: String,
    fields: Vec<ClassField>,
    size: i32,
    parent_class: Option<String>,
    virtual_methods: Vec<VirtualMethod>,
    has_vtable: bool,
}

pub struct CodeGenerator {
    module: ObjectModule,
    context: Context,
    functions: HashMap<String, FuncId>,
    string_counter: usize,
    class_metadata: HashMap<String, ClassMetadata>,
    module_name: Option<String>, // Name of the current module for name mangling
    type_aliases: HashMap<String, AstType>, // Type aliases resolved from program
    newtypes: HashMap<String, AstType>, // Newtypes map to their underlying type
    test_mode: bool, // Whether we're in test mode
    bench_mode: bool, // Whether we're in bench mode
}

impl CodeGenerator {
    /// Compute the mangled function name for export
    fn mangle_function_name(&self, simple_name: &str) -> String {
        if let Some(mod_name) = &self.module_name {
            // Skip mangling for main function - it must remain "_main" for linking
            if simple_name == "main" {
                return simple_name.to_string();
            }
            format!("{}::{}", mod_name, simple_name)
        } else {
            simple_name.to_string()
        }
    }

    /// Determine the variable type that a match expression returns
    fn determine_match_return_type(arms: &[MatchArm], _variable_types: &HashMap<String, VariableType>) -> VariableType {
        if arms.is_empty() {
            return VariableType::Int32;
        }

        // Check all arms to determine if we have mixed types requiring unified handling
        let mut has_string_literal = false;
        let mut has_integer_literal = false;
        let mut has_pattern_binding = false;

        for arm in arms {
            match &arm.body {
                Expression::Literal(Literal::String(_, _)) => has_string_literal = true,
                Expression::Literal(Literal::InterpolatedString(_, _)) => has_string_literal = true,
                Expression::Literal(Literal::Integer(_, _, _)) => has_integer_literal = true,
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
            return VariableType::Int32;
        }

        // Fallback to specific type detection
        match &arms[0].body {
            Expression::Literal(Literal::Bool(_, _)) => VariableType::Bool,
            Expression::Literal(Literal::Array(_, _)) => VariableType::Array(Box::new(VariableType::Int32)),
            Expression::Literal(Literal::Dict(_, _)) => VariableType::Dict,
            Expression::Literal(Literal::Set(_, _)) => VariableType::Set,
            Expression::EnumConstructor { enum_name, .. } => VariableType::Enum(enum_name.clone()),
            Expression::ConstructorCall { class_name, .. } => VariableType::Class(class_name.clone()),
            _ => VariableType::Int32,
        }
    }

    /// Infer the element type from an iterable expression (array)
    /// This looks at the expression structure to determine what type of elements it contains
    fn infer_element_type(iterable: &Expression, variable_types: &HashMap<String, VariableType>) -> VariableType {
        match iterable {
            // Direct array literal: look at first element to infer type
            Expression::Literal(Literal::Array(elements, _)) => {
                if elements.is_empty() {
                    return VariableType::Int32; // Default for empty arrays
                }
                match &elements[0] {
                    Expression::Literal(Literal::Bool(_, _)) => VariableType::Bool,
                    Expression::Literal(Literal::Integer(_, _, _)) => VariableType::Int32,
                    Expression::Literal(Literal::String(_, _)) => VariableType::String,
                    Expression::Literal(Literal::InterpolatedString(_, _)) => VariableType::String,
                    Expression::EnumConstructor { enum_name, .. } => VariableType::Enum(enum_name.clone()),
                    Expression::ConstructorCall { class_name, .. } => VariableType::Class(class_name.clone()),
                    Expression::Literal(Literal::Array(_, _)) => VariableType::Array(Box::new(VariableType::Int32)),
                    Expression::Literal(Literal::Dict(_, _)) => VariableType::Dict,
                    Expression::Literal(Literal::Set(_, _)) => VariableType::Set,
                    Expression::Identifier { name, .. } => {
                        // Look up the variable's type
                        variable_types.get(name).cloned().unwrap_or(VariableType::Int32)
                    }
                    _ => VariableType::Int32,
                }
            }
            // Variable reference: look up its type in variable_types
            Expression::Identifier { name, .. } => {
                // For arrays stored in variables, extract the element type from Array(element_type)
                match variable_types.get(name) {
                    Some(VariableType::Array(element_type)) => *element_type.clone(),
                    _ => VariableType::Int32, // Default if not found or not an array
                }
            }
            // Method call that returns an array
            Expression::MethodCall { .. } => {
                VariableType::Int32 // Default assumption
            }
            // Function call that returns an array
            Expression::Call { .. } => {
                VariableType::Int32 // Default assumption
            }
            _ => VariableType::Int32,
        }
    }

    fn infer_expression_type(expr: &Expression, variable_types: &HashMap<String, VariableType>) -> VariableType {
        match expr {
            Expression::Literal(Literal::Bool(_, _)) => VariableType::Bool,
            Expression::Literal(Literal::Integer(_, int_type, _)) => {
                match int_type {
                    IntType::I32 => VariableType::Int32,
                    IntType::I64 => VariableType::Int64,
                }
            }
            Expression::Literal(Literal::Float(_, float_type, _)) => {
                match float_type {
                    FloatType::F32 => VariableType::Float32,
                    FloatType::F64 => VariableType::Float64,
                }
            }
            Expression::Literal(Literal::String(_, _)) => VariableType::String,
            Expression::Literal(Literal::InterpolatedString(_, _)) => VariableType::String,
            Expression::Identifier { name, .. } => {
                variable_types.get(name).cloned().unwrap_or(VariableType::Int32)
            }
            Expression::Binary { left, op, right, .. } => {
                // For arithmetic operations, infer from operands
                match op {
                    BinaryOp::Add | BinaryOp::Subtract | BinaryOp::Multiply | BinaryOp::Divide | BinaryOp::Modulo => {
                        let left_type = Self::infer_expression_type(left, variable_types);
                        let right_type = Self::infer_expression_type(right, variable_types);
                        // Priority: F64 > F32/F16/F8 > I64 > I32/I16/I8
                        if left_type == VariableType::Float64 || right_type == VariableType::Float64 {
                            VariableType::Float64
                        } else if matches!(left_type, VariableType::Float32 | VariableType::Float16 | VariableType::Float8)
                               || matches!(right_type, VariableType::Float32 | VariableType::Float16 | VariableType::Float8) {
                            left_type.clone()
                        } else if left_type == VariableType::Int64 || right_type == VariableType::Int64 {
                            VariableType::Int64
                        } else {
                            left_type.clone()
                        }
                    }
                    _ => VariableType::Bool, // Comparison and logical operations return bool
                }
            }
            _ => VariableType::Int32, // Default
        }
    }

    /// Infer the return type of a block by analyzing return statements
    fn infer_block_return_type(block: &Block, variable_types: &HashMap<String, VariableType>) -> VariableType {
        for stmt in &block.statements {
            if let Statement::Return { value, .. } = stmt {
                if let Some(expr) = value {
                    return Self::infer_expression_type(expr, variable_types);
                }
            }
        }
        // Default to Int32 if no return found
        VariableType::Int32
    }

    /// Get the spawn function name for a given return type
    fn get_spawn_function_name(return_type: &VariableType) -> &'static str {
        match return_type {
            VariableType::Bool => "plat_spawn_task_bool",
            VariableType::Int32 => "plat_spawn_task_i32",
            VariableType::Int64 => "plat_spawn_task_i64",
            VariableType::Float32 => "plat_spawn_task_f32",
            VariableType::Float64 => "plat_spawn_task_f64",
            _ => "plat_spawn_task_i64", // Default to i64 for unsupported types
        }
    }

    /// Get the await function name for a given return type
    fn get_await_function_name(return_type: &VariableType) -> &'static str {
        match return_type {
            VariableType::Bool => "plat_task_await_bool",
            VariableType::Int32 => "plat_task_await_i32",
            VariableType::Int64 => "plat_task_await_i64",
            VariableType::Float32 => "plat_task_await_f32",
            VariableType::Float64 => "plat_task_await_f64",
            _ => "plat_task_await_i64", // Default to i64 for unsupported types
        }
    }

    /// Find all captured variables in an expression (variables not defined in local_vars)
    fn find_captured_variables(
        expr: &Expression,
        local_vars: &HashMap<String, VariableType>,
        captured: &mut Vec<String>
    ) {
        match expr {
            Expression::Identifier { name, .. } => {
                if !local_vars.contains_key(name) && !captured.contains(name) {
                    captured.push(name.clone());
                }
            }
            Expression::Binary { left, right, .. } => {
                Self::find_captured_variables(left, local_vars, captured);
                Self::find_captured_variables(right, local_vars, captured);
            }
            Expression::Unary { operand, .. } => {
                Self::find_captured_variables(operand, local_vars, captured);
            }
            Expression::Call { args, .. } => {
                // Function name is a string, not an expression, no need to check for captures
                for arg in args {
                    Self::find_captured_variables(&arg.value, local_vars, captured);
                }
            }
            Expression::MethodCall { object, args, .. } => {
                Self::find_captured_variables(object, local_vars, captured);
                for arg in args {
                    Self::find_captured_variables(&arg.value, local_vars, captured);
                }
            }
            Expression::Index { object, index, .. } => {
                Self::find_captured_variables(object, local_vars, captured);
                Self::find_captured_variables(index, local_vars, captured);
            }
            Expression::MemberAccess { object, .. } => {
                Self::find_captured_variables(object, local_vars, captured);
            }
            Expression::Assignment { target, value, .. } => {
                Self::find_captured_variables(target, local_vars, captured);
                Self::find_captured_variables(value, local_vars, captured);
            }
            Expression::If { condition, then_branch, else_branch, .. } => {
                Self::find_captured_variables(condition, local_vars, captured);
                Self::find_captured_variables(then_branch, local_vars, captured);
                if let Some(else_expr) = else_branch {
                    Self::find_captured_variables(else_expr, local_vars, captured);
                }
            }
            Expression::Block(block) => {
                let mut block_locals = local_vars.clone();
                for stmt in &block.statements {
                    Self::find_captured_in_statement(stmt, &mut block_locals, captured);
                }
            }
            Expression::Match { value, arms, .. } => {
                Self::find_captured_variables(value, local_vars, captured);
                for arm in arms {
                    Self::find_captured_variables(&arm.body, local_vars, captured);
                }
            }
            Expression::Cast { value, .. } => {
                Self::find_captured_variables(value, local_vars, captured);
            }
            Expression::Spawn { body, .. } => {
                // Don't recurse into spawn - it has its own scope
                Self::find_captured_variables(body, local_vars, captured);
            }
            _ => {} // Literals and other expressions don't capture
        }
    }

    /// Find captured variables in a statement
    fn find_captured_in_statement(
        stmt: &Statement,
        local_vars: &mut HashMap<String, VariableType>,
        captured: &mut Vec<String>
    ) {
        match stmt {
            Statement::Let { name, value, .. } | Statement::Var { name, value, .. } => {
                Self::find_captured_variables(value, local_vars, captured);
                // Add to local vars (type doesn't matter for capture detection)
                local_vars.insert(name.clone(), VariableType::Int32);
            }
            Statement::Return { value, .. } => {
                if let Some(expr) = value {
                    Self::find_captured_variables(expr, local_vars, captured);
                }
            }
            Statement::Expression(expression) => {
                Self::find_captured_variables(expression, local_vars, captured);
            }
            Statement::For { variable, iterable, body, .. } => {
                Self::find_captured_variables(iterable, local_vars, captured);
                local_vars.insert(variable.clone(), VariableType::Int32);
                for stmt in &body.statements {
                    Self::find_captured_in_statement(stmt, local_vars, captured);
                }
            }
            Statement::While { condition, body, .. } => {
                Self::find_captured_variables(condition, local_vars, captured);
                for stmt in &body.statements {
                    Self::find_captured_in_statement(stmt, local_vars, captured);
                }
            }
            Statement::If { condition, then_branch, else_branch, .. } => {
                Self::find_captured_variables(condition, local_vars, captured);
                for stmt in &then_branch.statements {
                    Self::find_captured_in_statement(stmt, local_vars, captured);
                }
                if let Some(else_block) = else_branch {
                    for stmt in &else_block.statements {
                        Self::find_captured_in_statement(stmt, local_vars, captured);
                    }
                }
            }
            Statement::Concurrent { body, .. } => {
                for stmt in &body.statements {
                    Self::find_captured_in_statement(stmt, local_vars, captured);
                }
            }
            Statement::Print { value, .. } => {
                Self::find_captured_variables(value, local_vars, captured);
            }
        }
    }

    /// Convert a VariableType to the corresponding Cranelift Type
    fn variable_type_to_cranelift_type(var_type: &VariableType) -> Type {
        match var_type {
            VariableType::Bool => I32,      // Booleans are represented as i32
            VariableType::Int8 => I8,
            VariableType::Int16 => I16,
            VariableType::Int32 => I32,
            VariableType::Int64 => I64,
            VariableType::Float8 => F32,    // Using F32 for 8-bit float
            VariableType::Float16 => F32,   // Using F32 for 16-bit float
            VariableType::Float32 => F32,
            VariableType::Float64 => F64,
            VariableType::String => I64,    // Strings are pointers
            VariableType::Array(_) => I64,  // Arrays are pointers
            VariableType::Dict => I64,      // Dicts are pointers
            VariableType::Set => I64,       // Sets are pointers
            VariableType::Class(_) => I64,  // Class instances are pointers
            VariableType::Enum(_) => I64,   // Enums are 64-bit values (discriminant + data)
            VariableType::Task(_) => I64,   // Task handles are 64-bit IDs
        }
    }

    /// Resolve a type alias recursively
    fn resolve_type_alias(&self, ty: &AstType) -> AstType {
        Self::resolve_type_alias_or_newtype(&self.type_aliases, &self.newtypes, ty)
    }

    /// Resolve both type aliases and newtypes to their underlying types
    fn resolve_type_alias_or_newtype(
        type_aliases: &HashMap<String, AstType>,
        newtypes: &HashMap<String, AstType>,
        ty: &AstType
    ) -> AstType {
        match ty {
            AstType::Named(name, type_params) if type_params.is_empty() => {
                // Check if this is a newtype first (resolve to underlying type)
                if let Some(resolved) = newtypes.get(name) {
                    // Recursively resolve in case of chained newtypes/aliases
                    Self::resolve_type_alias_or_newtype(type_aliases, newtypes, resolved)
                }
                // Check if this is a type alias
                else if let Some(resolved) = type_aliases.get(name) {
                    // Recursively resolve in case of chained aliases
                    Self::resolve_type_alias_or_newtype(type_aliases, newtypes, resolved)
                } else {
                    ty.clone()
                }
            }
            _ => ty.clone(),
        }
    }

    /// Static version of resolve_type_alias for use in helper methods
    fn resolve_type_alias_static(type_aliases: &HashMap<String, AstType>, ty: &AstType) -> AstType {
        match ty {
            AstType::Named(name, type_params) if type_params.is_empty() => {
                // Check if this is a type alias
                if let Some(resolved) = type_aliases.get(name) {
                    // Recursively resolve in case of chained aliases
                    Self::resolve_type_alias_static(type_aliases, resolved)
                } else {
                    ty.clone()
                }
            }
            _ => ty.clone(),
        }
    }

    /// Convert an AST type to a Cranelift type (resolving aliases first)
    fn ast_type_to_cranelift(&self, ty: &AstType) -> Type {
        let resolved_ty = self.resolve_type_alias(ty);
        match resolved_ty {
            AstType::String => I64,
            AstType::Int64 => I64,
            AstType::Float64 => F64,
            AstType::List(_) => I64,
            AstType::Dict(_, _) => I64,
            AstType::Set(_) => I64,
            AstType::Named(_, _) => I64, // Custom types (classes, enums) are pointers
            AstType::Bool => I32, // Booleans are I32
            AstType::Int8 => I8,
            AstType::Int16 => I16,
            AstType::Int32 => I32,
            AstType::Float8 => F32, // Cranelift doesn't support 8-bit floats, use F32
            AstType::Float16 => F32, // Cranelift doesn't support 16-bit floats, use F32
            AstType::Float32 => F32,
        }
    }

    /// Convert an AST type to a VariableType
    fn ast_type_to_variable_type(&self, ast_type: &AstType) -> VariableType {
        Self::ast_type_to_variable_type_static(&self.type_aliases, ast_type)
    }

    /// Static version of ast_type_to_variable_type for use in helper methods
    fn ast_type_to_variable_type_static(type_aliases: &HashMap<String, AstType>, ast_type: &AstType) -> VariableType {
        let resolved_ty = Self::resolve_type_alias_static(type_aliases, ast_type);
        match resolved_ty {
            AstType::Bool => VariableType::Bool,
            AstType::Int8 => VariableType::Int8,
            AstType::Int16 => VariableType::Int16,
            AstType::Int32 => VariableType::Int32,
            AstType::Int64 => VariableType::Int64,
            AstType::Float8 => VariableType::Float8,
            AstType::Float16 => VariableType::Float16,
            AstType::Float32 => VariableType::Float32,
            AstType::Float64 => VariableType::Float64,
            AstType::String => VariableType::String,
            AstType::List(element_type) => {
                let element_var_type = Self::ast_type_to_variable_type_static(type_aliases, &element_type);
                VariableType::Array(Box::new(element_var_type))
            }
            AstType::Dict(_, _) => VariableType::Dict,
            AstType::Set(_) => VariableType::Set,
            AstType::Named(type_name, type_params) => {
                // Check if this is a Task<T> type
                if type_name == "Task" && type_params.len() == 1 {
                    let inner_var_type = Self::ast_type_to_variable_type_static(type_aliases, &type_params[0]);
                    VariableType::Task(Box::new(inner_var_type))
                } else {
                    VariableType::Class(type_name.clone())
                }
            }
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
            class_metadata: HashMap::new(),
            module_name: None,
            type_aliases: HashMap::new(),
            newtypes: HashMap::new(),
            test_mode: false,
            bench_mode: false,
        })
    }

    /// Enable test mode for this code generator
    pub fn with_test_mode(mut self) -> Self {
        self.test_mode = true;
        self
    }

    /// Enable bench mode for this code generator
    pub fn with_bench_mode(mut self) -> Self {
        self.bench_mode = true;
        self
    }

    fn build_class_metadata(&mut self, class_decl: &ast::ClassDecl) -> Result<(), CodegenError> {
        let mut fields = Vec::new();
        let mut current_offset = 0i32;

        // Check if this class or any parent has virtual methods
        let has_virtual_methods = class_decl.methods.iter().any(|m| m.is_virtual || m.is_override);
        let has_vtable = has_virtual_methods || class_decl.parent_class.is_some();

        // If this class has a vtable, reserve space for vtable pointer at offset 0
        if has_vtable {
            current_offset = 8; // vtable pointer is always 8 bytes
        }

        // If we have a parent class, inherit its fields
        if let Some(parent_name) = &class_decl.parent_class {
            if let Some(parent_metadata) = self.class_metadata.get(parent_name) {
                // Copy parent's fields (they already have correct offsets including vtable)
                for parent_field in &parent_metadata.fields {
                    fields.push(parent_field.clone());
                }
                current_offset = parent_metadata.size;
            }
        }

        // Add this class's own fields
        for field in &class_decl.fields {
            // Determine Cranelift type and size for this field
            let (cranelift_type, size, alignment) = match &field.ty {
                AstType::String => (I64, 8, 8),
                AstType::Int64 => (I64, 8, 8),
                AstType::Float64 => (F64, 8, 8),
                AstType::List(_) => (I64, 8, 8),
                AstType::Dict(_, _) => (I64, 8, 8),
                AstType::Set(_) => (I64, 8, 8),
                AstType::Named(_, _) => (I64, 8, 8), // Custom types are pointers
                AstType::Int8 => (I8, 1, 1),
                AstType::Int16 => (I16, 2, 2),
                AstType::Int32 => (I32, 4, 4),
                AstType::Float8 => (F32, 4, 4), // Using F32 for 8-bit float
                AstType::Float16 => (F32, 4, 4), // Using F32 for 16-bit float
                AstType::Float32 => (F32, 4, 4),
                AstType::Bool => (I32, 4, 4),
            };

            // Align the offset
            if current_offset % alignment != 0 {
                current_offset = ((current_offset / alignment) + 1) * alignment;
            }

            fields.push(ClassField {
                name: field.name.clone(),
                ty: field.ty.clone(),
                offset: current_offset,
                cranelift_type,
            });

            current_offset += size;
        }

        // Align total size to 8 bytes
        let size = if current_offset % 8 != 0 {
            ((current_offset / 8) + 1) * 8
        } else {
            current_offset
        };

        // Build virtual method table
        let mut virtual_methods = Vec::new();

        // If we have a parent, inherit its virtual methods
        if let Some(parent_name) = &class_decl.parent_class {
            if let Some(parent_metadata) = self.class_metadata.get(parent_name) {
                virtual_methods = parent_metadata.virtual_methods.clone();
            }
        }

        // Process this class's methods
        for method in &class_decl.methods {
            if method.is_virtual {
                // New virtual method - add to vtable
                virtual_methods.push(VirtualMethod {
                    name: method.name.clone(),
                    vtable_index: virtual_methods.len(),
                    func_id: None, // Will be filled in later
                });
            } else if method.is_override {
                // Override existing virtual method
                if let Some(vm) = virtual_methods.iter_mut().find(|vm| vm.name == method.name) {
                    // Keep the same vtable_index, update func_id later
                    vm.func_id = None;
                } else {
                    return Err(CodegenError::UnsupportedFeature(
                        format!("Method '{}' marked as override but no virtual method found in parent", method.name)
                    ));
                }
            }
        }

        let metadata = ClassMetadata {
            name: class_decl.name.clone(),
            fields,
            size,
            parent_class: class_decl.parent_class.clone(),
            virtual_methods,
            has_vtable,
        };

        self.class_metadata.insert(class_decl.name.clone(), metadata);
        Ok(())
    }

    fn generate_vtables(&mut self, program: &Program) -> Result<(), CodegenError> {
        // Generate vtable global variables for each class with virtual methods
        // Each vtable is an array of function pointers stored as a global data object

        for class_decl in &program.classes {
            let metadata = self.class_metadata.get(&class_decl.name)
                .ok_or_else(|| CodegenError::UnsupportedFeature(
                    format!("Class metadata not found for '{}'", class_decl.name)
                ))?
                .clone(); // Clone to avoid borrow issues

            if !metadata.has_vtable || metadata.virtual_methods.is_empty() {
                continue; // Skip classes without virtual methods
            }

            // Create vtable data structure
            let vtable_name = format!("{}_vtable", class_decl.name);
            let vtable_size = metadata.virtual_methods.len() * 8; // 8 bytes per function pointer

            // Create a mutable data descriptor for the vtable
            let mut data_desc = DataDescription::new();
            data_desc.define_zeroinit(vtable_size);

            // Note: We can't use direct function relocations in the data section easily
            // Instead, we'll generate an initialization function that populates the vtable at startup
            // This is a common approach for vtables in compilers

            // Declare the vtable data as writable (will be initialized at startup)
            let vtable_data_id = self.module.declare_data(
                &vtable_name,
                Linkage::Export,
                true,  // writable - will be initialized at runtime
                false, // not thread-local
            ).map_err(CodegenError::ModuleError)?;

            self.module.define_data(vtable_data_id, &data_desc)
                .map_err(CodegenError::ModuleError)?;

            eprintln!("DEBUG: Created vtable '{}' with {} entries", vtable_name, metadata.virtual_methods.len());

            // Now generate an initialization function for this vtable
            self.generate_vtable_init_function(&class_decl.name, &metadata)?;
        }

        Ok(())
    }

    fn generate_vtable_init_function(&mut self, class_name: &str, metadata: &ClassMetadata) -> Result<(), CodegenError> {
        // Generate a function like: void ClassName_vtable_init()
        // This function will be called at program startup to initialize the vtable

        let init_func_name = format!("{}_vtable_init", class_name);
        let vtable_name = format!("{}_vtable", class_name);

        // Create function signature: void -> void
        let mut sig = self.module.make_signature();
        sig.call_conv = CallConv::SystemV;
        // No parameters, no return value

        // Declare the initialization function
        let init_func_id = self.module.declare_function(&init_func_name, Linkage::Export, &sig)
            .map_err(CodegenError::ModuleError)?;

        // Store for later use
        self.functions.insert(init_func_name.clone(), init_func_id);

        // Generate the function body
        self.context.func.signature = sig;
        let mut func_ctx = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut self.context.func, &mut func_ctx);

        let entry_block = builder.create_block();
        builder.switch_to_block(entry_block);
        builder.seal_block(entry_block);

        // Get the address of the vtable data
        let vtable_data_id = self.module.declare_data(
            &vtable_name,
            Linkage::Export,
            true,
            false,
        ).map_err(CodegenError::ModuleError)?;

        let vtable_ref = self.module.declare_data_in_func(vtable_data_id, &mut builder.func);
        let vtable_addr = builder.ins().global_value(I64, vtable_ref);

        // For each virtual method, store its function pointer in the vtable
        for (i, vmethod) in metadata.virtual_methods.iter().enumerate() {
            let method_name = format!("{}__{}", class_name, vmethod.name);

            // Get the function ID for this method
            if let Some(&func_id) = self.functions.get(&method_name) {
                // Get function reference
                let func_ref = self.module.declare_func_in_func(func_id, &mut builder.func);

                // Get function address as a pointer
                let func_addr = builder.ins().func_addr(I64, func_ref);

                // Calculate offset in vtable (i * 8 bytes)
                let offset = (i * 8) as i32;

                // Store function pointer at vtable[i]
                builder.ins().store(MemFlags::new(), func_addr, vtable_addr, offset);
            }
        }

        // Return from init function
        builder.ins().return_(&[]);
        builder.finalize();

        // Define the function
        self.module.define_function(init_func_id, &mut self.context)
            .map_err(CodegenError::ModuleError)?;

        self.module.clear_context(&mut self.context);

        eprintln!("DEBUG: Generated vtable init function '{}'", init_func_name);
        Ok(())
    }

    #[allow(dead_code)]
    fn get_field_info(&self, class_name: &str, field_name: &str) -> Result<(i32, Type), CodegenError> {
        Self::get_field_info_static(&self.class_metadata, class_name, field_name)
    }

    #[allow(dead_code)]
    fn get_field_info_static(class_metadata: &HashMap<String, ClassMetadata>, class_name: &str, field_name: &str) -> Result<(i32, Type), CodegenError> {
        let metadata = class_metadata.get(class_name)
            .ok_or_else(|| CodegenError::UnsupportedFeature(
                format!("Unknown class '{}'", class_name)
            ))?;

        let field = metadata.fields.iter()
            .find(|f| f.name == field_name)
            .ok_or_else(|| CodegenError::UnsupportedFeature(
                format!("Unknown field '{}' in class '{}'", field_name, class_name)
            ))?;

        Ok((field.offset, field.cranelift_type))
    }

    #[allow(dead_code)]
    fn get_class_size(&self, class_name: &str) -> Result<i32, CodegenError> {
        let metadata = self.class_metadata.get(class_name)
            .ok_or_else(|| CodegenError::UnsupportedFeature(
                format!("Unknown class '{}'", class_name)
            ))?;

        Ok(metadata.size)
    }

    pub fn generate_code(mut self, program: &Program) -> Result<Vec<u8>, CodegenError> {
        // Extract module name for function name mangling
        if let Some(mod_decl) = &program.module_decl {
            self.module_name = Some(mod_decl.path.join("::"));
        }

        // Process type aliases
        for type_alias in &program.type_aliases {
            self.type_aliases.insert(type_alias.name.clone(), type_alias.ty.clone());
        }

        // Process newtypes - they map to their underlying type at runtime
        for newtype in &program.newtypes {
            self.newtypes.insert(newtype.name.clone(), newtype.underlying_type.clone());
        }

        // Build class metadata first (before declaring functions)
        for class_decl in &program.classes {
            eprintln!("DEBUG: Building metadata for class: {}", class_decl.name);
            self.build_class_metadata(class_decl)?;
        }
        eprintln!("DEBUG: Built metadata for {} classes", self.class_metadata.len());

        // First pass: declare all functions (including enum methods and test functions)
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

        // Declare class methods
        for class_decl in &program.classes {
            for method in &class_decl.methods {
                let method_name = format!("{}__{}", class_decl.name, method.name);
                self.declare_function_with_name(&method_name, method)?;
            }
        }

        // Declare test functions (only in test mode)
        if self.test_mode {
            for test_block in &program.test_blocks {
                for function in &test_block.functions {
                    self.declare_function(function)?;
                }
            }
        }

        // Declare bench functions (only in bench mode)
        if self.bench_mode {
            for bench_block in &program.bench_blocks {
                for function in &bench_block.functions {
                    self.declare_function(function)?;
                }
            }
        }

        // Generate vtables for classes with virtual methods
        self.generate_vtables(program)?;

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

        // Generate code for class methods
        for class_decl in &program.classes {
            for method in &class_decl.methods {
                let method_name = format!("{}__{}", class_decl.name, method.name);
                self.generate_function_with_name(&method_name, method)?;
            }
        }

        // Generate code for test functions (only in test mode)
        if self.test_mode {
            for test_block in &program.test_blocks {
                for function in &test_block.functions {
                    self.generate_function(function)?;
                }
            }
        }

        // Generate code for bench functions (only in bench mode)
        if self.bench_mode {
            for bench_block in &program.bench_blocks {
                for function in &bench_block.functions {
                    self.generate_function(function)?;
                }
            }
        }

        // Finalize the module and return object code
        let object_product = self.module.finish();
        Ok(object_product.emit().map_err(CodegenError::ObjectEmitError)?)
    }

    fn declare_function(&mut self, function: &ast::Function) -> Result<(), CodegenError> {
        let mangled_name = self.mangle_function_name(&function.name);
        self.declare_function_with_name(&mangled_name, function)
    }

    fn declare_function_with_name(&mut self, name: &str, function: &ast::Function) -> Result<(), CodegenError> {
        let mut sig = self.module.make_signature();

        // Set calling convention
        sig.call_conv = CallConv::SystemV;

        // Add implicit self parameter for enum and class methods
        if name.contains("::") {
            // This is an enum method, add self parameter (represented as i64 for enum value)
            sig.params.push(AbiParam::new(I64));
        } else if name.contains("__") {
            // This is a class method, add self parameter (represented as i64 for class instance pointer)
            sig.params.push(AbiParam::new(I64));
        }

        // Add parameters
        for param in &function.params {
            let param_type = self.ast_type_to_cranelift(&param.ty);
            sig.params.push(AbiParam::new(param_type));
        }

        // Add return type
        if let Some(return_type) = &function.return_type {
            // Special handling for main returning Result/Option
            if (function.name == "main" || name == "main") && Self::is_result_or_option_with_int_return(return_type) {
                // Main with Result<Int*, E> or Option<Int*> returns i32 exit code
                sig.returns.push(AbiParam::new(I32));
            } else {
                let ret_type = self.ast_type_to_cranelift(return_type);
                sig.returns.push(AbiParam::new(ret_type));
            }
        } else if function.name == "main" || name == "main" {
            // Main function returns i32 (exit code) if no return type specified
            sig.returns.push(AbiParam::new(I32));
        }

        eprintln!("DEBUG: Declaring function {} with {} params, {} returns", name, sig.params.len(), sig.returns.len());
        let func_id = self.module.declare_function(name, Linkage::Export, &sig)
            .map_err(CodegenError::ModuleError)?;

        self.functions.insert(name.to_string(), func_id);

        Ok(())
    }

    fn generate_function(&mut self, function: &ast::Function) -> Result<(), CodegenError> {
        let mangled_name = self.mangle_function_name(&function.name);
        self.generate_function_with_name(&mangled_name, function)
    }

    fn generate_function_with_name(&mut self, name: &str, function: &ast::Function) -> Result<(), CodegenError> {
        eprintln!("DEBUG: Generating function {}", name);
        let func_id = self.functions[name];

        // Get function signature
        let sig = self.module.declarations().get_function_decl(func_id).signature.clone();
        eprintln!("DEBUG: Function {} has {} statements in body", name, function.body.statements.len());

        // Create the function in Cranelift IR
        self.context.func.signature = sig;

        // Pre-compute parameter types (before creating the builder to avoid borrow conflicts)
        let param_cranelift_types: Vec<Type> = function.params
            .iter()
            .map(|param| self.ast_type_to_cranelift(&param.ty))
            .collect();
        let param_variable_types: Vec<VariableType> = function.params
            .iter()
            .map(|param| self.ast_type_to_variable_type(&param.ty))
            .collect();

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

        // Check if this is a class or enum method (has implicit self parameter)
        let has_implicit_self = name.contains("::") || name.contains("__");
        let param_offset = if has_implicit_self { 1 } else { 0 };

        // If this is a class/enum method, handle the implicit self parameter
        if has_implicit_self {
            let self_var = Variable::from_u32(variable_counter);
            variable_counter += 1;
            builder.declare_var(self_var, I64); // self is always an I64 pointer
            builder.def_var(self_var, params[0]);
            variables.insert("self".to_string(), self_var);

            // Track self type - for class methods, extract the class name from the method name
            if name.contains("__") {
                let class_name = name.split("__").next().unwrap_or("Unknown");
                variable_types.insert("self".to_string(), VariableType::Class(class_name.to_string()));
            } else {
                // For enum methods
                let enum_name = name.split("::").next().unwrap_or("Unknown");
                variable_types.insert("self".to_string(), VariableType::Enum(enum_name.to_string()));
            }
        }

        // Add user-defined parameters
        for (i, param) in function.params.iter().enumerate() {
            let var = Variable::from_u32(variable_counter);
            variable_counter += 1;

            // Use pre-computed types
            let cranelift_type = param_cranelift_types[i];
            let var_type = param_variable_types[i].clone();

            builder.declare_var(var, cranelift_type);
            builder.def_var(var, params[i + param_offset]);
            variables.insert(param.name.clone(), var);
            variable_types.insert(param.name.clone(), var_type);
        }

        // Generate function body - we need to avoid borrowing conflicts
        // Extract the functions HashMap and type_aliases to avoid borrowing self while builder exists
        let functions_copy = self.functions.clone();
        let type_aliases_copy = self.type_aliases.clone();

        // Initialize runtime for main function
        if function.name == "main" {
            // Declare plat_runtime_init function
            let mut init_sig = self.module.make_signature();
            init_sig.call_conv = CallConv::SystemV;

            let init_func_id = self.module.declare_function("plat_runtime_init", Linkage::Import, &init_sig)
                .map_err(CodegenError::ModuleError)?;
            let init_func_ref = self.module.declare_func_in_func(init_func_id, builder.func);

            // Call runtime init
            builder.ins().call(init_func_ref, &[]);
        }

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
                &mut self.string_counter,
                &self.class_metadata,
                &type_aliases_copy,
                name,
                &function.return_type,
                self.test_mode
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

        // Debug: print the generated IR for inspection
        eprintln!("DEBUG: Generated IR for function {}:", name);
        eprintln!("{}", self.context.func);

        // Define the function
        self.module.define_function(func_id, &mut self.context)
            .map_err(|e| {
                eprintln!("ERROR: Failed to define function {}: {:?}", name, e);
                CodegenError::ModuleError(e)
            })?;

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
        string_counter: &mut usize,
        class_metadata: &HashMap<String, ClassMetadata>,
        type_aliases: &HashMap<String, AstType>,
        function_name: &str,
        function_return_type: &Option<AstType>,
        test_mode: bool
    ) -> Result<bool, CodegenError> {
        match statement {
            Statement::Let { name, ty, value, .. } => {
                let val = Self::generate_expression_with_expected_type(builder, value, Some(ty), variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
                let var = Variable::from_u32(*variable_counter);
                *variable_counter += 1;

                // Type annotation is now mandatory
                let plat_type = Self::ast_type_to_variable_type_static(type_aliases, ty);
                let cranelift_type = Self::variable_type_to_cranelift_type(&plat_type);

                builder.declare_var(var, cranelift_type);
                builder.def_var(var, val);
                variables.insert(name.clone(), var);
                variable_types.insert(name.clone(), plat_type);
                Ok(false)
            }
            Statement::Var { name, ty, value, .. } => {
                let val = Self::generate_expression_with_expected_type(builder, value, Some(ty), variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
                let var = Variable::from_u32(*variable_counter);
                *variable_counter += 1;

                // Type annotation is now mandatory
                let plat_type = Self::ast_type_to_variable_type_static(type_aliases, ty);
                let cranelift_type = Self::variable_type_to_cranelift_type(&plat_type);

                builder.declare_var(var, cranelift_type);
                builder.def_var(var, val);
                variables.insert(name.clone(), var);
                variable_types.insert(name.clone(), plat_type);
                Ok(false)
            }
            Statement::Return { value, .. } => {
                if let Some(expr) = value {
                    let val = Self::generate_expression_helper(builder, expr, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

                    // Special handling for main returning Result/Option
                    if function_name == "main" && function_return_type.as_ref().map_or(false, |ty| Self::is_result_or_option_with_int_return(ty)) {
                        // Extract exit code from Result/Option
                        // Enum layout: discriminant in high 32 bits, value in low 32 bits (for i32)
                        let return_type = function_return_type.as_ref().unwrap();
                        let type_name = if let AstType::Named(name, _) = return_type {
                            name.as_str()
                        } else {
                            return Err(CodegenError::UnsupportedFeature("Invalid return type".to_string()));
                        };

                        // Extract discriminant from high 32 bits
                        let disc_64 = builder.ins().ushr_imm(val, 32);
                        let disc = builder.ins().ireduce(I32, disc_64);

                        // Compute expected discriminants
                        let success_disc = if type_name == "Result" {
                            Self::variant_discriminant("Result", "Ok") as i64
                        } else {
                            Self::variant_discriminant("Option", "Some") as i64
                        };
                        let error_disc = if type_name == "Result" {
                            Self::variant_discriminant("Result", "Err") as i64
                        } else {
                            Self::variant_discriminant("Option", "None") as i64
                        };

                        // Create blocks
                        let success_block = builder.create_block();
                        let error_block = builder.create_block();

                        // Check if discriminant matches success variant
                        let expected_success = builder.ins().iconst(I32, success_disc);
                        let is_success = builder.ins().icmp(IntCC::Equal, disc, expected_success);
                        builder.ins().brif(is_success, success_block, &[], error_block, &[]);

                        // Success block: extract value from low 32 bits
                        builder.switch_to_block(success_block);
                        builder.seal_block(success_block);
                        let exit_code = builder.ins().ireduce(I32, val);
                        builder.ins().return_(&[exit_code]);

                        // Error block: return error code (1)
                        builder.switch_to_block(error_block);
                        builder.seal_block(error_block);
                        let error_code = builder.ins().iconst(I32, 1);
                        builder.ins().return_(&[error_code]);
                    } else {
                        // Convert return value type if needed to match function signature
                        let return_val = if let Some(expected_type) = function_return_type {
                            let expr_type = Self::infer_expression_type(expr, variable_types);
                            match (expr_type, expected_type) {
                                // Convert i32 to i64 (sign extend)
                                (VariableType::Int32, AstType::Int64) => {
                                    builder.ins().sextend(I64, val)
                                }
                                // Convert i32 to i32 (no-op, but handle explicitly)
                                (VariableType::Int32, AstType::Int32) => val,
                                // i64 to i64 (no conversion needed)
                                (VariableType::Int64, AstType::Int64) => val,
                                // i64 to i32 (truncate)
                                (VariableType::Int64, AstType::Int32) => {
                                    builder.ins().ireduce(I32, val)
                                }
                                // Default: use as-is
                                _ => val,
                            }
                        } else {
                            val
                        };
                        builder.ins().return_(&[return_val]);
                    }
                } else {
                    builder.ins().return_(&[]);
                }
                Ok(true)
            }
            Statement::Expression(expr) => {
                Self::generate_expression_helper(builder, expr, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
                Ok(false)
            }
            Statement::Print { value, .. } => {
                // Generate the value to print
                let val = Self::generate_expression_helper(builder, value, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

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
                let condition_val = Self::generate_expression_helper(builder, condition, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

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
                        functions, module, string_counter, class_metadata, type_aliases,
                        function_name, function_return_type, test_mode
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
                            functions, module, string_counter, class_metadata, type_aliases,
                            function_name, function_return_type, test_mode
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
                let condition_val = Self::generate_expression_helper(builder, condition, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
                let _zero = builder.ins().iconst(I32, 0);
                let condition_bool = builder.ins().icmp_imm(IntCC::NotEqual, condition_val, 0);
                builder.ins().brif(condition_bool, loop_body, &[], loop_exit, &[]);

                // Loop body
                builder.switch_to_block(loop_body);
                let mut body_has_return = false;
                for stmt in &body.statements {
                    body_has_return |= Self::generate_statement_helper(
                        builder, stmt, variables, variable_types, variable_counter,
                        functions, module, string_counter, class_metadata, type_aliases,
                        function_name, function_return_type, test_mode
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
                // Check if this is a range-based for loop
                if let Expression::Range { start, end, inclusive, .. } = iterable {
                    // Range-based for loop
                    return Self::generate_range_for_loop(
                        builder, variable, start, end, *inclusive, body,
                        variables, variable_types, variable_counter, functions, module, string_counter, class_metadata, type_aliases,
                        function_name, function_return_type, test_mode
                    );
                }

                // Array-based for loop (existing code)
                // Infer the element type from the iterable expression
                let element_type = Self::infer_element_type(iterable, variable_types);
                let element_cranelift_type = Self::variable_type_to_cranelift_type(&element_type);

                // Evaluate iterable
                let array_val = Self::generate_expression_helper(builder, iterable, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

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

                // Create loop variable for element with correct Cranelift type
                let element_var = Variable::from_u32(*variable_counter);
                *variable_counter += 1;
                builder.declare_var(element_var, element_cranelift_type);

                // Store in variables map with proper element type
                let old_variable = variables.insert(variable.clone(), element_var);
                let old_type = variable_types.insert(variable.clone(), element_type.clone());

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

                // Convert the i64 value to the appropriate type based on element_type
                let element_val = match element_cranelift_type {
                    I32 => {
                        // For i32 types (bool, i32), reduce from i64 to i32
                        builder.ins().ireduce(I32, element_val_i64)
                    }
                    I64 => {
                        // For i64 types (string, arrays, objects, enums), keep as i64
                        element_val_i64
                    }
                    _ => {
                        // Fallback for any other types
                        element_val_i64
                    }
                };

                // Set loop variable to current element
                builder.def_var(element_var, element_val);

                // Execute loop body statements
                let mut body_has_return = false;
                for stmt in &body.statements {
                    body_has_return |= Self::generate_statement_helper(
                        builder, stmt, variables, variable_types, variable_counter,
                        functions, module, string_counter, class_metadata, type_aliases,
                        function_name, function_return_type, test_mode
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
            Statement::Concurrent { body, .. } => {
                // Execute concurrent block with scope tracking

                // Declare plat_scope_enter function
                let mut enter_sig = module.make_signature();
                enter_sig.call_conv = CallConv::SystemV;
                enter_sig.returns.push(AbiParam::new(I64)); // Returns scope ID

                let enter_func_name = "plat_scope_enter";
                let enter_func_id = if let Some(&func_id) = functions.get(enter_func_name) {
                    func_id
                } else {
                    module.declare_function(enter_func_name, Linkage::Import, &enter_sig)
                        .map_err(CodegenError::ModuleError)?
                };
                let enter_func_ref = module.declare_func_in_func(enter_func_id, builder.func);

                // Call plat_scope_enter to get scope ID
                let call_inst = builder.ins().call(enter_func_ref, &[]);
                let scope_id = builder.inst_results(call_inst)[0];

                // Execute the concurrent block body
                let mut body_returned = false;
                for stmt in &body.statements {
                    let returned = Self::generate_statement_helper(
                        builder,
                        stmt,
                        variables,
                        variable_types,
                        variable_counter,
                        functions,
                        module,
                        string_counter,
                        class_metadata,
                        type_aliases,
                        function_name,
                        function_return_type,
                        test_mode
                    )?;
                    if returned {
                        body_returned = true;
                        break;
                    }
                }

                // Declare plat_scope_exit function
                let mut exit_sig = module.make_signature();
                exit_sig.call_conv = CallConv::SystemV;
                exit_sig.params.push(AbiParam::new(I64)); // Takes scope ID

                let exit_func_name = "plat_scope_exit";
                let exit_func_id = if let Some(&func_id) = functions.get(exit_func_name) {
                    func_id
                } else {
                    module.declare_function(exit_func_name, Linkage::Import, &exit_sig)
                        .map_err(CodegenError::ModuleError)?
                };
                let exit_func_ref = module.declare_func_in_func(exit_func_id, builder.func);

                // Call plat_scope_exit to wait for all spawned tasks
                builder.ins().call(exit_func_ref, &[scope_id]);

                Ok(body_returned)
            }
        }
    }

    fn generate_range_for_loop(
        builder: &mut FunctionBuilder,
        variable: &str,
        start: &Expression,
        end: &Expression,
        inclusive: bool,
        body: &Block,
        variables: &mut HashMap<String, Variable>,
        variable_types: &mut HashMap<String, VariableType>,
        variable_counter: &mut u32,
        functions: &HashMap<String, FuncId>,
        module: &mut ObjectModule,
        string_counter: &mut usize,
        class_metadata: &HashMap<String, ClassMetadata>,
        type_aliases: &HashMap<String, AstType>,
        function_name: &str,
        function_return_type: &Option<AstType>,
        test_mode: bool
    ) -> Result<bool, CodegenError> {
        // Evaluate start and end expressions
        let start_val = Self::generate_expression_helper(builder, start, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
        let end_val = Self::generate_expression_helper(builder, end, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

        // Infer the integer type from start expression (both should be same type due to HIR check)
        let int_type = Self::infer_expression_type(start, variable_types);
        let cranelift_type = Self::variable_type_to_cranelift_type(&int_type);

        // Create loop variable
        let loop_var = Variable::from_u32(*variable_counter);
        *variable_counter += 1;
        builder.declare_var(loop_var, cranelift_type);
        builder.def_var(loop_var, start_val);

        // Store in variables map
        let old_variable = variables.insert(variable.to_string(), loop_var);
        let old_type = variable_types.insert(variable.to_string(), int_type);

        // Create blocks
        let loop_header = builder.create_block();
        let loop_body = builder.create_block();
        let loop_exit = builder.create_block();

        // Jump to loop header
        builder.ins().jump(loop_header, &[]);

        // Loop header: check condition
        builder.switch_to_block(loop_header);
        let current_val = builder.use_var(loop_var);

        // For inclusive ranges (..=), condition is: current_val <= end_val
        // For exclusive ranges (..), condition is: current_val < end_val
        let condition = if inclusive {
            if cranelift_type == I32 {
                builder.ins().icmp(IntCC::SignedLessThanOrEqual, current_val, end_val)
            } else {
                builder.ins().icmp(IntCC::SignedLessThanOrEqual, current_val, end_val)
            }
        } else {
            if cranelift_type == I32 {
                builder.ins().icmp(IntCC::SignedLessThan, current_val, end_val)
            } else {
                builder.ins().icmp(IntCC::SignedLessThan, current_val, end_val)
            }
        };

        builder.ins().brif(condition, loop_body, &[], loop_exit, &[]);

        // Loop body: execute statements
        builder.switch_to_block(loop_body);

        let mut body_has_return = false;
        for stmt in &body.statements {
            body_has_return |= Self::generate_statement_helper(
                builder, stmt, variables, variable_types, variable_counter,
                functions, module, string_counter, class_metadata, type_aliases,
                function_name, function_return_type, test_mode
            )?;
        }

        // Increment loop variable
        if !body_has_return {
            let current_val = builder.use_var(loop_var);
            let one = if cranelift_type == I32 {
                builder.ins().iconst(I32, 1)
            } else {
                builder.ins().iconst(I64, 1)
            };
            let next_val = builder.ins().iadd(current_val, one);
            builder.def_var(loop_var, next_val);
            builder.ins().jump(loop_header, &[]);
        }

        // Seal blocks
        builder.seal_block(loop_header);
        builder.seal_block(loop_body);

        // Loop exit
        builder.switch_to_block(loop_exit);
        builder.seal_block(loop_exit);

        // Restore old variable binding if it existed
        if let Some(old_var) = old_variable {
            variables.insert(variable.to_string(), old_var);
        } else {
            variables.remove(variable);
        }
        if let Some(old_typ) = old_type {
            variable_types.insert(variable.to_string(), old_typ);
        } else {
            variable_types.remove(variable);
        }

        Ok(false) // for loops don't guarantee return
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
        variable_counter: &mut u32,
        class_metadata: &HashMap<String, ClassMetadata>,
        test_mode: bool
    ) -> Result<Value, CodegenError> {
        match expr {
            Expression::Literal(Literal::Array(elements, _)) => {
                // Use expected type information for array generation
                Self::generate_typed_array_literal(builder, elements, expected_type, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)
            }
            Expression::Literal(Literal::Dict(pairs, _)) => {
                // Use expected type information for dict generation
                Self::generate_typed_dict_literal(builder, pairs, expected_type, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)
            }
            Expression::Literal(Literal::Set(elements, _)) => {
                // Use expected type information for set generation
                Self::generate_typed_set_literal(builder, elements, expected_type, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)
            }
            _ => {
                // For non-array expressions, use the regular helper
                Self::generate_expression_helper(builder, expr, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)
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
        variable_counter: &mut u32,
        class_metadata: &HashMap<String, ClassMetadata>,
        test_mode: bool
    ) -> Result<Value, CodegenError> {
        if pairs.is_empty() {
            // For empty dicts, determine type from annotation or default to string->i32
            let (_key_type, _value_type) = if let Some(AstType::Dict(key_type, value_type)) = expected_type {
                (key_type.as_ref(), value_type.as_ref())
            } else {
                (&AstType::String, &AstType::Int32) // default
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
            let key_val = Self::generate_expression_helper(builder, key_expr, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
            keys.push(key_val);

            // Evaluate value
            let value_val = Self::generate_expression_helper(builder, value_expr, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
            values.push(value_val);

            // Determine value type
            let type_val = match value_expr {
                Expression::Literal(Literal::Bool(_, _)) => 2u8, // DICT_VALUE_TYPE_BOOL
                Expression::Literal(Literal::Integer(val, _, _)) => {
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
        variable_counter: &mut u32,
        class_metadata: &HashMap<String, ClassMetadata>,
        test_mode: bool
    ) -> Result<Value, CodegenError> {
        if elements.is_empty() {
            // For empty sets, determine type from annotation or default to i32
            let _element_type = if let Some(AstType::Set(element_type)) = expected_type {
                element_type.as_ref()
            } else {
                &AstType::Int32 // default
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
            let value_val = Self::generate_expression_helper(builder, element_expr, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
            values.push(value_val);

            // Determine value type
            let type_val = match element_expr {
                Expression::Literal(Literal::Bool(_, _)) => 2u8, // SET_VALUE_TYPE_BOOL
                Expression::Literal(Literal::Integer(val, _, _)) => {
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
        variable_counter: &mut u32,
        class_metadata: &HashMap<String, ClassMetadata>,
        test_mode: bool
    ) -> Result<Value, CodegenError> {
        match expr {
            Expression::Literal(literal) => {
                Self::generate_literal(builder, literal, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)
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
                        let left_val = Self::generate_expression_helper(builder, left, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
                        let right_val = Self::generate_expression_helper(builder, right, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

                        // Determine if we're working with floats
                        let left_type = Self::infer_expression_type(left, variable_types);
                        let is_float = matches!(left_type, VariableType::Float8 | VariableType::Float16 | VariableType::Float32 | VariableType::Float64);

                        match op {
                            BinaryOp::Add => {
                                if is_float {
                                    Ok(builder.ins().fadd(left_val, right_val))
                                } else {
                                    Ok(builder.ins().iadd(left_val, right_val))
                                }
                            }
                            BinaryOp::Subtract => {
                                if is_float {
                                    Ok(builder.ins().fsub(left_val, right_val))
                                } else {
                                    Ok(builder.ins().isub(left_val, right_val))
                                }
                            }
                            BinaryOp::Multiply => {
                                if is_float {
                                    Ok(builder.ins().fmul(left_val, right_val))
                                } else {
                                    Ok(builder.ins().imul(left_val, right_val))
                                }
                            }
                            BinaryOp::Divide => {
                                if is_float {
                                    Ok(builder.ins().fdiv(left_val, right_val))
                                } else {
                                    Ok(builder.ins().sdiv(left_val, right_val))
                                }
                            }
                            BinaryOp::Modulo => Ok(builder.ins().srem(left_val, right_val)),
                            BinaryOp::Equal => {
                                if is_float {
                                    let cmp = builder.ins().fcmp(FloatCC::Equal, left_val, right_val);
                                    Ok(builder.ins().uextend(I32, cmp))
                                } else {
                                    let cmp = builder.ins().icmp(IntCC::Equal, left_val, right_val);
                                    Ok(builder.ins().uextend(I32, cmp))
                                }
                            }
                            BinaryOp::NotEqual => {
                                if is_float {
                                    let cmp = builder.ins().fcmp(FloatCC::NotEqual, left_val, right_val);
                                    Ok(builder.ins().uextend(I32, cmp))
                                } else {
                                    let cmp = builder.ins().icmp(IntCC::NotEqual, left_val, right_val);
                                    Ok(builder.ins().uextend(I32, cmp))
                                }
                            }
                            BinaryOp::Less => {
                                if is_float {
                                    let cmp = builder.ins().fcmp(FloatCC::LessThan, left_val, right_val);
                                    Ok(builder.ins().uextend(I32, cmp))
                                } else {
                                    let cmp = builder.ins().icmp(IntCC::SignedLessThan, left_val, right_val);
                                    Ok(builder.ins().uextend(I32, cmp))
                                }
                            }
                            BinaryOp::LessEqual => {
                                if is_float {
                                    let cmp = builder.ins().fcmp(FloatCC::LessThanOrEqual, left_val, right_val);
                                    Ok(builder.ins().uextend(I32, cmp))
                                } else {
                                    let cmp = builder.ins().icmp(IntCC::SignedLessThanOrEqual, left_val, right_val);
                                    Ok(builder.ins().uextend(I32, cmp))
                                }
                            }
                            BinaryOp::Greater => {
                                if is_float {
                                    let cmp = builder.ins().fcmp(FloatCC::GreaterThan, left_val, right_val);
                                    Ok(builder.ins().uextend(I32, cmp))
                                } else {
                                    let cmp = builder.ins().icmp(IntCC::SignedGreaterThan, left_val, right_val);
                                    Ok(builder.ins().uextend(I32, cmp))
                                }
                            }
                            BinaryOp::GreaterEqual => {
                                if is_float {
                                    let cmp = builder.ins().fcmp(FloatCC::GreaterThanOrEqual, left_val, right_val);
                                    Ok(builder.ins().uextend(I32, cmp))
                                } else {
                                    let cmp = builder.ins().icmp(IntCC::SignedGreaterThanOrEqual, left_val, right_val);
                                    Ok(builder.ins().uextend(I32, cmp))
                                }
                            }
                            _ => unreachable!()
                        }
                    }
                    BinaryOp::And => {
                        // Short-circuit AND: evaluate left first
                        let left_val = Self::generate_expression_helper(builder, left, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

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
                        let right_val = Self::generate_expression_helper(builder, right, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
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
                        let left_val = Self::generate_expression_helper(builder, left, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

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
                        let right_val = Self::generate_expression_helper(builder, right, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
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
                let operand_val = Self::generate_expression_helper(builder, operand, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

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
                let val = Self::generate_expression_helper(builder, value, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

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
                        // For member access assignment to class fields
                        // We need to:
                        // 1. Get the object pointer (should be in a variable)
                        // 2. Calculate the field offset
                        // 3. Store the value at object_ptr + offset

                        // Get the object value (class instance pointer)
                        let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

                        // Determine class name from the object type
                        let class_name = Self::get_class_name(object, variable_types)
                            .ok_or_else(|| CodegenError::UnsupportedFeature(
                                format!("Cannot determine class type for member access assignment")
                            ))?;

                        // Look up field offset from class metadata
                        let (offset, _field_type) = Self::get_field_info_static(class_metadata, &class_name, member)?;

                        // Store the value at the computed offset
                        builder.ins().store(MemFlags::new(), val, object_val, offset);

                        Ok(val)
                    }
                    _ => {
                        Err(CodegenError::UnsupportedFeature(
                            "Invalid assignment target in codegen".to_string()
                        ))
                    }
                }
            }
            Expression::Call { function, args, .. } => {
                // Handle built-in assert function
                if function == "assert" {
                    // Find the 'condition' and optional 'message' arguments
                    let condition_arg = args.iter()
                        .find(|arg| arg.name == "condition")
                        .ok_or_else(|| CodegenError::AssertError("Missing 'condition' argument in assert".to_string()))?;

                    let message_arg = args.iter().find(|arg| arg.name == "message");

                    // Generate code for the condition
                    let condition_val = Self::generate_expression_helper(
                        builder, &condition_arg.value, variables, variable_types,
                        functions, module, string_counter, variable_counter, class_metadata, test_mode
                    )?;

                    // Generate code for the optional message
                    let message_val = if let Some(msg_arg) = message_arg {
                        Self::generate_expression_helper(
                            builder, &msg_arg.value, variables, variable_types,
                            functions, module, string_counter, variable_counter, class_metadata, test_mode
                        )?
                    } else {
                        // Use null pointer for default message
                        builder.ins().iconst(I64, 0)
                    };

                    // In test mode, use plat_assert_test which returns Bool
                    // In normal mode, use plat_assert which exits on failure
                    if test_mode {
                        // Declare plat_assert_test function (returns bool)
                        let assert_sig = {
                            let mut sig = module.make_signature();
                            sig.call_conv = CallConv::SystemV;
                            sig.params.push(AbiParam::new(I32)); // condition (bool as i32)
                            sig.params.push(AbiParam::new(I64)); // message pointer
                            sig.returns.push(AbiParam::new(I32)); // returns bool
                            sig
                        };

                        let assert_id = module.declare_function("plat_assert_test", Linkage::Import, &assert_sig)
                            .map_err(CodegenError::ModuleError)?;
                        let assert_ref = module.declare_func_in_func(assert_id, builder.func);

                        // Call plat_assert_test and return the result
                        let call = builder.ins().call(assert_ref, &[condition_val, message_val]);
                        let result = builder.inst_results(call)[0];
                        return Ok(result);
                    } else {
                        // Declare plat_assert function (void return)
                        let assert_sig = {
                            let mut sig = module.make_signature();
                            sig.call_conv = CallConv::SystemV;
                            sig.params.push(AbiParam::new(I32)); // condition (bool as i32)
                            sig.params.push(AbiParam::new(I64)); // message pointer
                            sig
                        };

                        let assert_id = module.declare_function("plat_assert", Linkage::Import, &assert_sig)
                            .map_err(CodegenError::ModuleError)?;
                        let assert_ref = module.declare_func_in_func(assert_id, builder.func);

                        // Call plat_assert
                        builder.ins().call(assert_ref, &[condition_val, message_val]);

                        // assert returns Unit, represented as 0
                        return Ok(builder.ins().iconst(I64, 0));
                    }
                }

                // Handle built-in __test_reset function (test mode only)
                if function == "__test_reset" {
                    // Declare plat_test_reset function
                    let reset_sig = {
                        let mut sig = module.make_signature();
                        sig.call_conv = CallConv::SystemV;
                        sig
                    };

                    let reset_id = module.declare_function("plat_test_reset", Linkage::Import, &reset_sig)
                        .map_err(CodegenError::ModuleError)?;
                    let reset_ref = module.declare_func_in_func(reset_id, builder.func);

                    // Call plat_test_reset
                    builder.ins().call(reset_ref, &[]);

                    // __test_reset returns Unit, represented as 0
                    return Ok(builder.ins().iconst(I64, 0));
                }

                // Handle built-in __test_check function (test mode only)
                if function == "__test_check" {
                    // Declare plat_test_check function
                    let check_sig = {
                        let mut sig = module.make_signature();
                        sig.call_conv = CallConv::SystemV;
                        sig.returns.push(AbiParam::new(I32)); // returns bool
                        sig
                    };

                    let check_id = module.declare_function("plat_test_check", Linkage::Import, &check_sig)
                        .map_err(CodegenError::ModuleError)?;
                    let check_ref = module.declare_func_in_func(check_id, builder.func);

                    // Call plat_test_check and return the result
                    let call = builder.ins().call(check_ref, &[]);
                    let result = builder.inst_results(call)[0];
                    return Ok(result);
                }

                // Handle built-in tcp_listen function
                if function == "tcp_listen" {
                    // tcp_listen(host: String, port: Int32) -> Result<Int32, String>
                    let host_arg = args.iter().find(|arg| arg.name == "host")
                        .ok_or_else(|| CodegenError::UnsupportedFeature("tcp_listen missing 'host' parameter".to_string()))?;
                    let port_arg = args.iter().find(|arg| arg.name == "port")
                        .ok_or_else(|| CodegenError::UnsupportedFeature("tcp_listen missing 'port' parameter".to_string()))?;

                    let host_val = Self::generate_expression_helper(builder, &host_arg.value, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
                    let port_val = Self::generate_expression_helper(builder, &port_arg.value, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

                    let func_sig = {
                        let mut sig = module.make_signature();
                        sig.call_conv = CallConv::SystemV;
                        sig.params.push(AbiParam::new(I64)); // host (string pointer)
                        sig.params.push(AbiParam::new(I32)); // port
                        sig.returns.push(AbiParam::new(I64)); // Result enum pointer
                        sig
                    };

                    let func_id = module.declare_function("plat_tcp_listen", Linkage::Import, &func_sig)
                        .map_err(CodegenError::ModuleError)?;
                    let func_ref = module.declare_func_in_func(func_id, builder.func);

                    let call = builder.ins().call(func_ref, &[host_val, port_val]);
                    return Ok(builder.inst_results(call)[0]);
                }

                // Handle built-in tcp_accept function
                if function == "tcp_accept" {
                    // tcp_accept(listener: Int32) -> Result<Int32, String>
                    let listener_arg = args.iter().find(|arg| arg.name == "listener")
                        .ok_or_else(|| CodegenError::UnsupportedFeature("tcp_accept missing 'listener' parameter".to_string()))?;

                    let listener_val = Self::generate_expression_helper(builder, &listener_arg.value, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

                    let func_sig = {
                        let mut sig = module.make_signature();
                        sig.call_conv = CallConv::SystemV;
                        sig.params.push(AbiParam::new(I32)); // listener fd
                        sig.returns.push(AbiParam::new(I64)); // Result enum pointer
                        sig
                    };

                    let func_id = module.declare_function("plat_tcp_accept", Linkage::Import, &func_sig)
                        .map_err(CodegenError::ModuleError)?;
                    let func_ref = module.declare_func_in_func(func_id, builder.func);

                    let call = builder.ins().call(func_ref, &[listener_val]);
                    return Ok(builder.inst_results(call)[0]);
                }

                // Handle built-in tcp_connect function
                if function == "tcp_connect" {
                    // tcp_connect(host: String, port: Int32) -> Result<Int32, String>
                    let host_arg = args.iter().find(|arg| arg.name == "host")
                        .ok_or_else(|| CodegenError::UnsupportedFeature("tcp_connect missing 'host' parameter".to_string()))?;
                    let port_arg = args.iter().find(|arg| arg.name == "port")
                        .ok_or_else(|| CodegenError::UnsupportedFeature("tcp_connect missing 'port' parameter".to_string()))?;

                    let host_val = Self::generate_expression_helper(builder, &host_arg.value, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
                    let port_val = Self::generate_expression_helper(builder, &port_arg.value, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

                    let func_sig = {
                        let mut sig = module.make_signature();
                        sig.call_conv = CallConv::SystemV;
                        sig.params.push(AbiParam::new(I64)); // host (string pointer)
                        sig.params.push(AbiParam::new(I32)); // port
                        sig.returns.push(AbiParam::new(I64)); // Result enum pointer
                        sig
                    };

                    let func_id = module.declare_function("plat_tcp_connect", Linkage::Import, &func_sig)
                        .map_err(CodegenError::ModuleError)?;
                    let func_ref = module.declare_func_in_func(func_id, builder.func);

                    let call = builder.ins().call(func_ref, &[host_val, port_val]);
                    return Ok(builder.inst_results(call)[0]);
                }

                // Handle built-in tcp_read function
                if function == "tcp_read" {
                    // tcp_read(socket: Int32, max_bytes: Int32) -> Result<String, String>
                    let socket_arg = args.iter().find(|arg| arg.name == "socket")
                        .ok_or_else(|| CodegenError::UnsupportedFeature("tcp_read missing 'socket' parameter".to_string()))?;
                    let max_bytes_arg = args.iter().find(|arg| arg.name == "max_bytes")
                        .ok_or_else(|| CodegenError::UnsupportedFeature("tcp_read missing 'max_bytes' parameter".to_string()))?;

                    let socket_val = Self::generate_expression_helper(builder, &socket_arg.value, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
                    let max_bytes_val = Self::generate_expression_helper(builder, &max_bytes_arg.value, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

                    let func_sig = {
                        let mut sig = module.make_signature();
                        sig.call_conv = CallConv::SystemV;
                        sig.params.push(AbiParam::new(I32)); // socket fd
                        sig.params.push(AbiParam::new(I32)); // max_bytes
                        sig.returns.push(AbiParam::new(I64)); // Result enum pointer
                        sig
                    };

                    let func_id = module.declare_function("plat_tcp_read", Linkage::Import, &func_sig)
                        .map_err(CodegenError::ModuleError)?;
                    let func_ref = module.declare_func_in_func(func_id, builder.func);

                    let call = builder.ins().call(func_ref, &[socket_val, max_bytes_val]);
                    return Ok(builder.inst_results(call)[0]);
                }

                // Handle built-in tcp_write function
                if function == "tcp_write" {
                    // tcp_write(socket: Int32, data: String) -> Result<Int32, String>
                    let socket_arg = args.iter().find(|arg| arg.name == "socket")
                        .ok_or_else(|| CodegenError::UnsupportedFeature("tcp_write missing 'socket' parameter".to_string()))?;
                    let data_arg = args.iter().find(|arg| arg.name == "data")
                        .ok_or_else(|| CodegenError::UnsupportedFeature("tcp_write missing 'data' parameter".to_string()))?;

                    let socket_val = Self::generate_expression_helper(builder, &socket_arg.value, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
                    let data_val = Self::generate_expression_helper(builder, &data_arg.value, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

                    let func_sig = {
                        let mut sig = module.make_signature();
                        sig.call_conv = CallConv::SystemV;
                        sig.params.push(AbiParam::new(I32)); // socket fd
                        sig.params.push(AbiParam::new(I64)); // data (string pointer)
                        sig.returns.push(AbiParam::new(I64)); // Result enum pointer
                        sig
                    };

                    let func_id = module.declare_function("plat_tcp_write", Linkage::Import, &func_sig)
                        .map_err(CodegenError::ModuleError)?;
                    let func_ref = module.declare_func_in_func(func_id, builder.func);

                    let call = builder.ins().call(func_ref, &[socket_val, data_val]);
                    return Ok(builder.inst_results(call)[0]);
                }

                // Handle built-in tcp_close function
                if function == "tcp_close" {
                    // tcp_close(socket: Int32) -> Result<Bool, String>
                    let socket_arg = args.iter().find(|arg| arg.name == "socket")
                        .ok_or_else(|| CodegenError::UnsupportedFeature("tcp_close missing 'socket' parameter".to_string()))?;

                    let socket_val = Self::generate_expression_helper(builder, &socket_arg.value, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

                    let func_sig = {
                        let mut sig = module.make_signature();
                        sig.call_conv = CallConv::SystemV;
                        sig.params.push(AbiParam::new(I32)); // socket fd
                        sig.returns.push(AbiParam::new(I64)); // Result enum pointer
                        sig
                    };

                    let func_id = module.declare_function("plat_tcp_close", Linkage::Import, &func_sig)
                        .map_err(CodegenError::ModuleError)?;
                    let func_ref = module.declare_func_in_func(func_id, builder.func);

                    let call = builder.ins().call(func_ref, &[socket_val]);
                    return Ok(builder.inst_results(call)[0]);
                }

                // Check if this is actually a class constructor with no arguments (e.g., Empty())
                // This happens when a class has no fields and uses a default init
                if args.is_empty() && class_metadata.contains_key(function) {
                    // This is a zero-argument class constructor
                    // Generate the same code as ConstructorCall but with no field initialization
                    let metadata = class_metadata.get(function).unwrap();
                    let class_size = metadata.size as i64;
                    let has_vtable = metadata.has_vtable;

                    // Allocate memory using GC
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

                    let size_val = builder.ins().iconst(I64, class_size);
                    let call = builder.ins().call(gc_alloc_ref, &[size_val]);
                    let class_ptr = builder.inst_results(call)[0];

                    // If this class has a vtable, store the vtable pointer at offset 0
                    if has_vtable {
                        let vtable_name = format!("{}_vtable", function);

                        // Get the address of the vtable global
                        let vtable_data_id = module.declare_data(
                            &vtable_name,
                            Linkage::Export,
                            true,
                            false,
                        ).map_err(CodegenError::ModuleError)?;

                        let vtable_ref = module.declare_data_in_func(vtable_data_id, builder.func);
                        let vtable_addr = builder.ins().global_value(I64, vtable_ref);

                        // Store vtable pointer at offset 0
                        builder.ins().store(MemFlags::new(), vtable_addr, class_ptr, 0);
                    }

                    // No field initialization needed (no fields)
                    // Return the class pointer
                    return Ok(class_ptr);
                }

                // Check if this is a cross-module call (qualified name with ::)
                let func_id = if function.contains("::") {
                    // Cross-module call - declare as import with standard ABI
                    // For now, we assume all cross-module functions take i64 params and return i64
                    // This is a simplified approach that works for most Plat types (pointers, strings, classes, etc.)
                    // Future enhancement: pass HIR symbol table to get exact signatures

                    let sig = {
                        let mut sig = module.make_signature();
                        sig.call_conv = CallConv::SystemV;
                        // Add i64 parameter for each argument
                        for _ in args {
                            sig.params.push(AbiParam::new(I64));
                        }
                        // Assume i64 return (covers most Plat types: strings, objects, i64, etc.)
                        sig.returns.push(AbiParam::new(I64));
                        sig
                    };

                    module.declare_function(function, Linkage::Import, &sig)
                        .map_err(CodegenError::ModuleError)?
                } else {
                    // Local function call - look up in functions map
                    match functions.get(function) {
                        Some(&id) => id,
                        None => return Err(CodegenError::UndefinedFunction(function.clone())),
                    }
                };

                // Get function reference for calling
                let func_ref = module.declare_func_in_func(func_id, builder.func);

                // Evaluate arguments
                let mut arg_values = Vec::new();
                for arg in args {
                    let arg_val = Self::generate_expression_helper(builder, &arg.value, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
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
                let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
                let index_val = Self::generate_expression_helper(builder, index, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

                // Use safe get that returns Option<T>
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

                let found = results[0]; // i32: 0 or 1
                let value = results[1]; // i64

                // Compute discriminants for Option variants
                let none_disc = Self::variant_discriminant("Option", "None") as i64;
                let some_disc = Self::variant_discriminant("Option", "Some") as i64;

                // Create blocks for conditional
                let some_block = builder.create_block();
                let none_block = builder.create_block();
                let merge_block = builder.create_block();

                // Add parameter to merge block for the result
                builder.append_block_param(merge_block, I64);

                // Branch based on found
                builder.ins().brif(found, some_block, &[], none_block, &[]);

                // Some block: create Option::Some(value)
                builder.switch_to_block(some_block);
                builder.seal_block(some_block);

                // Check if value needs heap allocation (for pointer types)
                let element_type = Self::infer_element_type(object, variable_types);
                let needs_heap = matches!(element_type,
                    VariableType::String | VariableType::Array(_) | VariableType::Class(_) | VariableType::Enum(_)
                );

                let some_value = if needs_heap {
                    // Allocate: [discriminant:i32][padding:i32][value:i64]
                    let gc_alloc_sig = {
                        let mut sig = module.make_signature();
                        sig.call_conv = CallConv::SystemV;
                        sig.params.push(AbiParam::new(I64));
                        sig.returns.push(AbiParam::new(I64));
                        sig
                    };
                    let gc_alloc_id = module.declare_function("plat_gc_alloc", Linkage::Import, &gc_alloc_sig)
                        .map_err(CodegenError::ModuleError)?;
                    let gc_alloc_ref = module.declare_func_in_func(gc_alloc_id, builder.func);

                    let size = builder.ins().iconst(I64, 16);
                    let alloc_call = builder.ins().call(gc_alloc_ref, &[size]);
                    let ptr = builder.inst_results(alloc_call)[0];

                    let disc_val = builder.ins().iconst(I32, some_disc);
                    builder.ins().store(MemFlags::new(), disc_val, ptr, 0);
                    builder.ins().store(MemFlags::new(), value, ptr, 8);

                    ptr
                } else {
                    // Pack: discriminant in high 32 bits, value in low 32 bits
                    let disc_64 = builder.ins().iconst(I64, some_disc);
                    let disc_shifted = builder.ins().ishl_imm(disc_64, 32);
                    let value_32 = builder.ins().ireduce(I32, value);
                    let value_64 = builder.ins().uextend(I64, value_32);
                    builder.ins().bor(disc_shifted, value_64)
                };

                builder.ins().jump(merge_block, &[some_value]);

                // None block: create Option::None
                builder.switch_to_block(none_block);
                builder.seal_block(none_block);

                let none_disc_64 = builder.ins().iconst(I64, none_disc);
                let none_value = builder.ins().ishl_imm(none_disc_64, 32);

                builder.ins().jump(merge_block, &[none_value]);

                // Merge block
                builder.switch_to_block(merge_block);
                builder.seal_block(merge_block);

                let result = builder.block_params(merge_block)[0];
                Ok(result)
            }
            Expression::MethodCall { object, method, args, .. } => {
                match method.as_str() {
                    "len" => {
                        if !args.is_empty() {
                            return Err(CodegenError::UnsupportedFeature("len() method takes no arguments".to_string()));
                        }

                        let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

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

                        let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

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

                        let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
                        let arg_val = Self::generate_expression_helper(builder, &args[0].value, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

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

                        let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
                        let arg_val = Self::generate_expression_helper(builder, &args[0].value, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

                        // Determine object type for dispatch
                        let is_set = Self::is_set_type(object, variable_types);

                        if is_set {
                            // Set contains
                            let value_type = Self::get_set_value_type(&args[0].value, variable_types);

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

                        let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
                        let arg_val = Self::generate_expression_helper(builder, &args[0].value, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

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

                        let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

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

                        let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
                        let from_val = Self::generate_expression_helper(builder, &args[0].value, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
                        let to_val = Self::generate_expression_helper(builder, &args[1].value, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

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

                        let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
                        let delimiter_val = Self::generate_expression_helper(builder, &args[0].value, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

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

                        let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

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
                    "parse_int" | "parse_int64" | "parse_float" | "parse_bool" => {
                        if !args.is_empty() {
                            return Err(CodegenError::UnsupportedFeature(format!("{}() method takes no arguments", method)));
                        }

                        let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

                        let func_sig = {
                            let mut sig = module.make_signature();
                            sig.call_conv = CallConv::SystemV;
                            sig.params.push(AbiParam::new(I64)); // string pointer
                            sig.returns.push(AbiParam::new(I64)); // Result enum pointer
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

                        let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
                        let index_val = Self::generate_expression_helper(builder, &args[0].value, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

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

                        let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
                        let index_val = Self::generate_expression_helper(builder, &args[0].value, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
                        let value_val = Self::generate_expression_helper(builder, &args[1].value, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

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

                        let _call = builder.ins().call(func_ref, &[object_val, index_val, value_64]);
                        // Returns success as i32, but we're treating this as void operation for now
                        let zero = builder.ins().iconst(I32, 0);
                        Ok(zero)
                    }
                    "append" => {
                        if args.len() != 1 {
                            return Err(CodegenError::UnsupportedFeature("append() method takes exactly one argument".to_string()));
                        }

                        let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
                        let value_val = Self::generate_expression_helper(builder, &args[0].value, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

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

                        let _call = builder.ins().call(func_ref, &[object_val, value_64]);
                        // Returns success as i32, but we're treating this as void operation for now
                        let zero = builder.ins().iconst(I32, 0);
                        Ok(zero)
                    }
                    "insert_at" => {
                        if args.len() != 2 {
                            return Err(CodegenError::UnsupportedFeature("insert_at() method takes exactly two arguments".to_string()));
                        }

                        let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
                        let index_val = Self::generate_expression_helper(builder, &args[0].value, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
                        let value_val = Self::generate_expression_helper(builder, &args[1].value, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

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

                        let _call = builder.ins().call(func_ref, &[object_val, index_val, value_64]);
                        // Returns success as i32, but we're treating this as void operation for now
                        let zero = builder.ins().iconst(I32, 0);
                        Ok(zero)
                    }
                    "remove_at" => {
                        if args.len() != 1 {
                            return Err(CodegenError::UnsupportedFeature("remove_at() method takes exactly one argument".to_string()));
                        }

                        let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
                        let index_val = Self::generate_expression_helper(builder, &args[0].value, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

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

                        let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

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

                            let _call = builder.ins().call(func_ref, &[object_val]);
                            // Returns success as i32, but we're treating this as void operation for now
                            let zero = builder.ins().iconst(I32, 0);
                            Ok(zero)
                        }
                    }
                    "index_of" => {
                        if args.len() != 1 {
                            return Err(CodegenError::UnsupportedFeature("index_of() method takes exactly one argument".to_string()));
                        }

                        let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
                        let value_val = Self::generate_expression_helper(builder, &args[0].value, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

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

                        let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
                        let value_val = Self::generate_expression_helper(builder, &args[0].value, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

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

                        let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
                        let start_val = Self::generate_expression_helper(builder, &args[0].value, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
                        let end_val = Self::generate_expression_helper(builder, &args[1].value, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

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

                        let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

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

                        let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

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

                                let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
                                let key_val = Self::generate_expression_helper(builder, &args[0].value, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

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

                                let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
                                let key_val = Self::generate_expression_helper(builder, &args[0].value, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
                                let value_val = Self::generate_expression_helper(builder, &args[1].value, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

                                // Determine value type
                                let value_type = Self::get_dict_value_type(&args[1].value, variable_types);

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

                                let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
                                let key_val = Self::generate_expression_helper(builder, &args[0].value, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

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

                                let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

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

                                let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

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

                                let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

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

                                let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

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

                                let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
                                let key_val = Self::generate_expression_helper(builder, &args[0].value, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

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

                                let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
                                let value_val = Self::generate_expression_helper(builder, &args[0].value, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

                                // Determine value type
                                let value_type = Self::get_dict_value_type(&args[0].value, variable_types);

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

                                let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
                                let other_val = Self::generate_expression_helper(builder, &args[0].value, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

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

                                let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
                                let key_val = Self::generate_expression_helper(builder, &args[0].value, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
                                let default_val = Self::generate_expression_helper(builder, &args[1].value, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

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
                    "add" | "remove" | "union" | "intersection" | "difference" | "is_subset_of" | "is_superset_of" | "is_disjoint_from" if Self::is_set_type(object, variable_types) => {
                        match method.as_str() {
                            "add" | "remove" => {
                                if args.len() != 1 {
                                    return Err(CodegenError::UnsupportedFeature(format!("Set.{}() method takes exactly one argument", method)));
                                }

                                let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
                                let value_val = Self::generate_expression_helper(builder, &args[0].value, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

                                // Determine value type
                                let value_type = Self::get_set_value_type(&args[0].value, variable_types);

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

                                let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
                                let other_val = Self::generate_expression_helper(builder, &args[0].value, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

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

                                let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
                                let other_val = Self::generate_expression_helper(builder, &args[0].value, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

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
                    "await" => {
                        // Task.await() method
                        if !args.is_empty() {
                            return Err(CodegenError::UnsupportedFeature("await() method takes no arguments".to_string()));
                        }

                        // Determine the inner type of the Task<T> from the object
                        let task_inner_type = if let Expression::Identifier { name, .. } = object.as_ref() {
                            if let Some(VariableType::Task(inner)) = variable_types.get(name) {
                                (**inner).clone()
                            } else {
                                // Fallback to Int32 if type not found or not a Task
                                VariableType::Int32
                            }
                        } else {
                            // For complex expressions, default to Int32
                            VariableType::Int32
                        };

                        // Generate the task handle value
                        let task_handle = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

                        // Get the appropriate await function name based on inner type
                        let await_func_name = Self::get_await_function_name(&task_inner_type);
                        let await_return_type = Self::variable_type_to_cranelift_type(&task_inner_type);

                        let await_func_id = if let Some(&func_id) = functions.get(await_func_name) {
                            func_id
                        } else {
                            // Declare the await function
                            let mut await_sig = module.make_signature();
                            await_sig.call_conv = CallConv::SystemV;
                            await_sig.params.push(AbiParam::new(I64)); // Task handle
                            await_sig.returns.push(AbiParam::new(await_return_type)); // Result value

                            let func_id = module.declare_function(await_func_name, Linkage::Import, &await_sig)
                                .map_err(CodegenError::ModuleError)?;
                            func_id
                        };

                        // Call await function
                        let await_func_ref = module.declare_func_in_func(await_func_id, builder.func);
                        let call = builder.ins().call(await_func_ref, &[task_handle]);
                        let result = builder.inst_results(call)[0];

                        Ok(result)
                    }
                    // Class methods
                    method_name if Self::is_class_type(object, variable_types) => {
                        let object_val = Self::generate_expression_helper(builder, object, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
                        let class_name = Self::get_class_name(object, variable_types).unwrap_or_else(|| "Unknown".to_string());

                        // Check if this is a virtual method call that needs dynamic dispatch
                        let metadata = class_metadata.get(&class_name);
                        let is_virtual = metadata.map_or(false, |m| {
                            m.virtual_methods.iter().any(|vm| vm.name == method_name)
                        });

                        // Generate arguments first (needed for both static and dynamic calls)
                        let mut call_args = vec![object_val]; // Start with self
                        for (i, arg) in args.iter().enumerate() {
                            eprintln!("DEBUG: Processing argument {} of type {:?}", i, arg);
                            let arg_val = Self::generate_expression_helper(builder, &arg.value, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
                            call_args.push(arg_val);
                        }

                        if is_virtual && metadata.unwrap().has_vtable {
                            // Dynamic dispatch through vtable
                            eprintln!("DEBUG: Using dynamic dispatch for virtual method '{}' on class '{}'", method_name, class_name);

                            // Find the vtable index for this method
                            let vtable_index = metadata.unwrap()
                                .virtual_methods.iter()
                                .find(|vm| vm.name == method_name)
                                .map(|vm| vm.vtable_index)
                                .ok_or_else(|| CodegenError::UnsupportedFeature(
                                    format!("Virtual method '{}' not found in vtable", method_name)
                                ))?;

                            // Load vtable pointer from object at offset 0
                            let vtable_ptr = builder.ins().load(I64, MemFlags::new(), object_val, 0);

                            // Calculate offset in vtable: index * 8 (size of function pointer)
                            let vtable_offset = (vtable_index * 8) as i32;

                            // Load function pointer from vtable
                            let func_ptr = builder.ins().load(I64, MemFlags::new(), vtable_ptr, vtable_offset);

                            // Create signature for the indirect call
                            // Get the signature from a representative method
                            let func_name = format!("{}__{}", class_name, method_name);
                            let func_id = *functions.get(&func_name)
                                .ok_or_else(|| CodegenError::UnsupportedFeature(
                                    format!("Method function '{}' not found", func_name)
                                ))?;
                            let sig_ref = module.declarations().get_function_decl(func_id).signature.clone();

                            // Import the signature into the current function
                            let sig = builder.import_signature(sig_ref);

                            // Perform indirect call through function pointer
                            let call = builder.ins().call_indirect(sig, func_ptr, &call_args);

                            // Check if the method has a return value
                            let results = builder.inst_results(call);
                            if results.is_empty() {
                                // Void method - return unit (0) as I32
                                Ok(builder.ins().iconst(I32, 0))
                            } else {
                                // Method with return value - return as-is
                                Ok(results[0])
                            }
                        } else {
                            // Static dispatch (compile-time resolution)
                            eprintln!("DEBUG: Using static dispatch for method '{}' on class '{}'", method_name, class_name);

                            let func_name = format!("{}__{}", class_name, method_name);
                            let func_id = *functions.get(&func_name)
                                .ok_or_else(|| CodegenError::UnsupportedFeature(
                                    format!("Method function '{}' not found", func_name)
                                ))?;
                            let func_ref = module.declare_func_in_func(func_id, builder.func);

                            let sig = module.declarations().get_function_decl(func_id).signature.clone();
                            eprintln!("DEBUG: Function signature has {} params", sig.params.len());
                            eprintln!("DEBUG: About to call with {} call_args", call_args.len());

                            // Call the method directly
                            let call = builder.ins().call(func_ref, &call_args);

                            // Check if the method has a return value
                            let results = builder.inst_results(call);
                            if results.is_empty() {
                                // Void method - return unit (0) as I32
                                Ok(builder.ins().iconst(I32, 0))
                            } else {
                                // Method with return value - return as-is
                                Ok(results[0])
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
                    let arg_val = Self::generate_expression_helper(builder, &args[0].value, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

                    // Determine if we need heap allocation based on the argument type
                    let needs_heap = match &args[0].value {
                        Expression::Literal(Literal::String(_, _)) => true,
                        Expression::Literal(Literal::InterpolatedString(_, _)) => true,
                        Expression::Literal(Literal::Array(_, _)) => true,
                        Expression::Literal(Literal::Dict(_, _)) => true,
                        Expression::Literal(Literal::Set(_, _)) => true,
                        Expression::Identifier { name, .. } => {
                            matches!(variable_types.get(name), Some(VariableType::String) | Some(VariableType::Array(_)) | Some(VariableType::Dict) | Some(VariableType::Set) | Some(VariableType::Class(_)))
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
                        let arg_val = Self::generate_expression_helper(builder, &arg.value, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
                        let offset = 4 + (i * 4) as i32; // discriminant + field index * field_size
                        builder.ins().store(MemFlags::new(), arg_val, ptr, offset);
                    }

                    Ok(ptr)
                }
            }
            Expression::Match { value, arms, .. } => {
                let value_val = Self::generate_expression_helper(builder, value, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

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
                    VariableType::String | VariableType::Array(_) | VariableType::Enum(_) | VariableType::Class(_) => I64,
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
                        for (binding_idx, (binding_name, _binding_type)) in bindings.iter().enumerate() {
                            if !binding_name.is_empty() {
                                // For now, assume all single-field data variants use packed format
                                // and all multi-field variants use heap format
                                let (field_val, var_type, cranelift_type) = if bindings.len() == 1 {
                                    // Single field: assume packed format (discriminant in high, value in low)
                                    let packed_val = builder.ins().ireduce(I32, value_val);
                                    (packed_val, VariableType::Int32, I32)
                                } else {
                                    // Multi-field: assume heap format, load from offset
                                    let offset = 4 + (binding_idx * 4) as i32; // 4-byte alignment for i32
                                    let loaded = builder.ins().load(I32, MemFlags::new(), value_val, offset);
                                    (loaded, VariableType::Int32, I32)
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

                    let arm_result = Self::generate_expression_helper(builder, &arm.body, &arm_variables, &arm_variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

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
                let expr_val = Self::generate_expression_helper(builder, expression, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;

                // The ? operator desugars to:
                // match expr {
                //     Option::Some(x) -> x,
                //     Option::None -> return Option::None,
                //     Result::Ok(x) -> x,
                //     Result::Err(e) -> return Result::Err(e),
                // }

                // Extract discriminant (similar to match expression handling)
                // Packed format: discriminant in high 32 bits
                let packed_disc = builder.ins().ushr_imm(expr_val, 32);
                let disc_i32 = builder.ins().ireduce(I32, packed_disc);

                // Check if discriminant is 0 (Some/Ok) or non-zero (None/Err)
                // For both Option and Result, the success variant (Some/Ok) has discriminant != None/Err
                // We need to determine which enum we're working with
                // For simplicity, check if disc is 0 to determine the variant

                // Create blocks for the two paths
                let success_block = builder.create_block(); // Some/Ok
                let error_block = builder.create_block();   // None/Err

                // Determine discriminants for Option and Result
                // Option: None=0, Some=1 (based on variant ordering)
                // Result: Ok=0, Err=1 (based on variant ordering)

                // We need to know if it's Option or Result to know which discriminant is success
                // For now, use a heuristic: check the actual discriminant values
                // Actually, let's use the standard discriminants:
                // - Result::Ok = variant_discriminant("Result", "Ok")
                // - Result::Err = variant_discriminant("Result", "Err")
                // - Option::Some = variant_discriminant("Option", "Some")
                // - Option::None = variant_discriminant("Option", "None")

                // Since we don't know at runtime which enum it is, we'll use a simpler approach:
                // Check if disc matches the "success" pattern
                // For both Option and Result, let's assume None/Err both have non-zero discriminant
                // and Some/Ok have specific discriminants

                // Compute discriminants
                let ok_disc = Self::variant_discriminant("Result", "Ok");
                let some_disc = Self::variant_discriminant("Option", "Some");

                // Check if it matches either success discriminant
                let ok_const = builder.ins().iconst(I32, ok_disc as i64);
                let some_const = builder.ins().iconst(I32, some_disc as i64);
                let is_ok = builder.ins().icmp(IntCC::Equal, disc_i32, ok_const);
                let is_some = builder.ins().icmp(IntCC::Equal, disc_i32, some_const);
                let is_success = builder.ins().bor(is_ok, is_some);

                // Branch: if success, go to success_block; otherwise error_block
                builder.ins().brif(is_success, success_block, &[], error_block, &[]);

                // Success block: extract the value and continue
                builder.switch_to_block(success_block);
                builder.seal_block(success_block);
                // For packed format: value is in low 32 bits
                let success_val = builder.ins().ireduce(I32, expr_val);

                // Create a continuation block to merge the success path
                let cont_block = builder.create_block();
                builder.append_block_param(cont_block, I32);
                builder.ins().jump(cont_block, &[success_val]);

                // Error block: return the enum value as-is
                builder.switch_to_block(error_block);
                builder.seal_block(error_block);
                // Just return the original enum value (which contains None or Err)
                // The return type should be i64 for enums
                builder.ins().return_(&[expr_val]);

                // Continuation block (only reached from success path)
                builder.switch_to_block(cont_block);
                builder.seal_block(cont_block);

                let result = builder.block_params(cont_block)[0];
                Ok(result)
            }
            Expression::MemberAccess { object, member, .. } => {
                // Generate code for reading a field from a class instance
                // Use direct memory loads at computed offsets

                // First, evaluate the object expression to get the class pointer
                let object_val = Self::generate_expression_helper(
                    builder, object, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode
                )?;

                // Determine class name from the object type
                let class_name = Self::get_class_name(object, variable_types)
                    .ok_or_else(|| CodegenError::UnsupportedFeature(
                        format!("Cannot determine class type for member access")
                    ))?;

                // Look up field offset and type from class metadata
                let (offset, field_type) = Self::get_field_info_static(class_metadata, &class_name, member)?;

                // Load the value from the computed offset
                let field_value = builder.ins().load(field_type, MemFlags::new(), object_val, offset);

                Ok(field_value)
            }
            Expression::ConstructorCall { class_name, args, .. } => {
                // Create a new class instance using direct memory allocation
                // Look up class size from metadata
                let metadata = class_metadata.get(class_name)
                    .ok_or_else(|| CodegenError::UnsupportedFeature(
                        format!("Unknown class '{}' in constructor", class_name)
                    ))?;
                let class_size = metadata.size as i64;
                let has_vtable = metadata.has_vtable;

                // Allocate memory using GC
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

                let size_val = builder.ins().iconst(I64, class_size);
                let call = builder.ins().call(gc_alloc_ref, &[size_val]);
                let class_ptr = builder.inst_results(call)[0];

                // If this class has a vtable, store the vtable pointer at offset 0
                if has_vtable {
                    let vtable_name = format!("{}_vtable", class_name);

                    // Get the address of the vtable global
                    let vtable_data_id = module.declare_data(
                        &vtable_name,
                        Linkage::Export,
                        true,
                        false,
                    ).map_err(CodegenError::ModuleError)?;

                    let vtable_ref = module.declare_data_in_func(vtable_data_id, builder.func);
                    let vtable_addr = builder.ins().global_value(I64, vtable_ref);

                    // Store vtable pointer at offset 0
                    builder.ins().store(MemFlags::new(), vtable_addr, class_ptr, 0);

                    eprintln!("DEBUG: Stored vtable pointer for class '{}' at offset 0", class_name);
                }

                // Set each field from the named arguments using direct memory stores
                for arg in args {
                    let field_name = &arg.name;
                    let field_value_expr = &arg.value;

                    // Evaluate the field value
                    let field_value = Self::generate_expression_helper(
                        builder, field_value_expr, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode
                    )?;

                    // Look up field offset from class metadata
                    let (offset, _field_type) = Self::get_field_info_static(class_metadata, class_name, field_name)?;

                    // Store the value at the computed offset
                    builder.ins().store(MemFlags::new(), field_value, class_ptr, offset);
                }

                // Return the class pointer
                Ok(class_ptr)
            }
            Expression::Self_ { .. } => {
                // Look up 'self' in the variables map
                if let Some(&self_var) = variables.get("self") {
                    Ok(builder.use_var(self_var))
                } else {
                    Err(CodegenError::UndefinedVariable("self".to_string()))
                }
            }
            Expression::Block(_block) => {
                // For now, return an error since we need to implement block expressions
                Err(CodegenError::UnsupportedFeature("Block expressions not yet implemented".to_string()))
            }
            Expression::If { condition, then_branch, else_branch, .. } => {
                // Create blocks for the branches
                let then_block = builder.create_block();
                let else_block = builder.create_block();
                let cont_block = builder.create_block();

                // Evaluate condition
                let cond_val = Self::generate_expression_helper(
                    builder, condition, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode
                )?;

                // Convert i32 bool to i8 for conditional branch
                let cond_bool = builder.ins().icmp_imm(IntCC::NotEqual, cond_val, 0);

                // Branch based on condition
                builder.ins().brif(cond_bool, then_block, &[], else_block, &[]);

                // Generate then branch
                builder.switch_to_block(then_block);
                builder.seal_block(then_block);
                let then_val = Self::generate_expression_helper(
                    builder, then_branch, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode
                )?;
                builder.ins().jump(cont_block, &[then_val]);

                // Generate else branch (or default to unit value)
                builder.switch_to_block(else_block);
                builder.seal_block(else_block);
                let else_val = if let Some(else_expr) = else_branch {
                    Self::generate_expression_helper(
                        builder, else_expr, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode
                    )?
                } else {
                    // If no else branch, default to 0 (unit value represented as i32)
                    builder.ins().iconst(I32, 0)
                };
                builder.ins().jump(cont_block, &[else_val]);

                // Continue block - add parameter for the result
                builder.switch_to_block(cont_block);
                builder.append_block_param(cont_block, I32);
                builder.seal_block(cont_block);

                let result = builder.block_params(cont_block)[0];
                Ok(result)
            }
            Expression::Cast { value, target_type, .. } => {
                // Generate the value to cast
                let value_val = Self::generate_expression_helper(
                    builder, value, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode
                )?;

                // Determine source type
                let source_type = Self::infer_expression_type(value, variable_types);

                // Perform the cast based on source and target types
                let result = match (&source_type, target_type) {
                    // Float to int conversions (truncate towards zero)
                    (VariableType::Float8 | VariableType::Float16 | VariableType::Float32, AstType::Int8) => {
                        builder.ins().fcvt_to_sint(I8, value_val)
                    }
                    (VariableType::Float8 | VariableType::Float16 | VariableType::Float32, AstType::Int16) => {
                        builder.ins().fcvt_to_sint(I16, value_val)
                    }
                    (VariableType::Float8 | VariableType::Float16 | VariableType::Float32, AstType::Int32) => {
                        builder.ins().fcvt_to_sint(I32, value_val)
                    }
                    (VariableType::Float8 | VariableType::Float16 | VariableType::Float32, AstType::Int64) => {
                        builder.ins().fcvt_to_sint(I64, value_val)
                    }
                    (VariableType::Float64, AstType::Int8) => {
                        builder.ins().fcvt_to_sint(I8, value_val)
                    }
                    (VariableType::Float64, AstType::Int16) => {
                        builder.ins().fcvt_to_sint(I16, value_val)
                    }
                    (VariableType::Float64, AstType::Int32) => {
                        builder.ins().fcvt_to_sint(I32, value_val)
                    }
                    (VariableType::Float64, AstType::Int64) => {
                        builder.ins().fcvt_to_sint(I64, value_val)
                    }

                    // Int to float conversions
                    (VariableType::Int8 | VariableType::Int16 | VariableType::Int32 | VariableType::Int64, AstType::Float8 | AstType::Float16 | AstType::Float32) => {
                        builder.ins().fcvt_from_sint(F32, value_val)
                    }
                    (VariableType::Int8 | VariableType::Int16 | VariableType::Int32 | VariableType::Int64, AstType::Float64) => {
                        builder.ins().fcvt_from_sint(F64, value_val)
                    }

                    // Int to int conversions (with wrapping for overflow)
                    (VariableType::Int8, AstType::Int8) => value_val,
                    (VariableType::Int8, AstType::Int16) => builder.ins().sextend(I16, value_val),
                    (VariableType::Int8, AstType::Int32) => builder.ins().sextend(I32, value_val),
                    (VariableType::Int8, AstType::Int64) => builder.ins().sextend(I64, value_val),
                    (VariableType::Int16, AstType::Int8) => builder.ins().ireduce(I8, value_val),
                    (VariableType::Int16, AstType::Int16) => value_val,
                    (VariableType::Int16, AstType::Int32) => builder.ins().sextend(I32, value_val),
                    (VariableType::Int16, AstType::Int64) => builder.ins().sextend(I64, value_val),
                    (VariableType::Int32, AstType::Int8) => builder.ins().ireduce(I8, value_val),
                    (VariableType::Int32, AstType::Int16) => builder.ins().ireduce(I16, value_val),
                    (VariableType::Int32, AstType::Int32) => value_val,
                    (VariableType::Int32, AstType::Int64) => builder.ins().sextend(I64, value_val),
                    (VariableType::Int64, AstType::Int8) => builder.ins().ireduce(I8, value_val),
                    (VariableType::Int64, AstType::Int16) => builder.ins().ireduce(I16, value_val),
                    (VariableType::Int64, AstType::Int32) => builder.ins().ireduce(I32, value_val),
                    (VariableType::Int64, AstType::Int64) => value_val,

                    // Float to float conversions
                    (VariableType::Float8 | VariableType::Float16 | VariableType::Float32, AstType::Float64) => {
                        builder.ins().fpromote(F64, value_val)
                    }
                    (VariableType::Float64, AstType::Float8 | AstType::Float16 | AstType::Float32) => {
                        builder.ins().fdemote(F32, value_val)
                    }

                    // Same type (no-op, but we still return the value)
                    _ => value_val
                };

                Ok(result)
            }
            Expression::Spawn { body, .. } => {
                // Detect captured variables (variables from outer scope used in spawn body)
                let mut captured_vars = Vec::new();
                let empty_locals = HashMap::new();  // Spawn body starts with no local variables
                Self::find_captured_variables(body, &empty_locals, &mut captured_vars);

                // Filter captured_vars to only include those that exist in outer scope
                captured_vars.retain(|name| variable_types.contains_key(name));

                // Infer the return type of the spawn closure
                let closure_return_type = if let Expression::Block(block) = body.as_ref() {
                    Self::infer_block_return_type(block, variable_types)
                } else {
                    Self::infer_expression_type(body, variable_types)
                };

                // Create a unique closure function name
                let closure_name = format!("__spawn_closure_{}", string_counter);
                *string_counter += 1;

                // Create the closure function signature with the inferred return type
                let cranelift_return_type = Self::variable_type_to_cranelift_type(&closure_return_type);
                let mut sig = module.make_signature();
                sig.call_conv = CallConv::SystemV;

                // If there are captures, add context pointer parameter
                let has_captures = !captured_vars.is_empty();
                if has_captures {
                    sig.params.push(AbiParam::new(I64)); // Context pointer
                }
                sig.returns.push(AbiParam::new(cranelift_return_type));

                // Convert VariableType to AstType for statement generation
                let return_ast_type = match closure_return_type {
                    VariableType::Bool => AstType::Bool,
                    VariableType::Int32 => AstType::Int32,
                    VariableType::Int64 => AstType::Int64,
                    VariableType::Float32 => AstType::Float32,
                    VariableType::Float64 => AstType::Float64,
                    _ => AstType::Int64, // Default
                };

                // Allocate context struct if needed
                let ctx_ptr = if has_captures {
                    // Calculate total size needed for captured variables
                    let mut total_size = 0i64;
                    for var_name in &captured_vars {
                        if let Some(var_type) = variable_types.get(var_name) {
                            let type_size = Self::variable_type_to_cranelift_type(var_type);
                            total_size += type_size.bytes() as i64;
                        }
                    }

                    // Allocate memory for context (using malloc-like function)
                    let malloc_sig = {
                        let mut sig = module.make_signature();
                        sig.call_conv = CallConv::SystemV;
                        sig.params.push(AbiParam::new(I64)); // size
                        sig.returns.push(AbiParam::new(I64)); // pointer
                        sig
                    };
                    let malloc_id = module.declare_function("malloc", Linkage::Import, &malloc_sig)
                        .map_err(CodegenError::ModuleError)?;
                    let malloc_ref = module.declare_func_in_func(malloc_id, builder.func);

                    let size_val = builder.ins().iconst(I64, total_size);
                    let call = builder.ins().call(malloc_ref, &[size_val]);
                    let ptr = builder.inst_results(call)[0];

                    // Store captured values in the context
                    let mut offset = 0i32;
                    for var_name in &captured_vars {
                        if let Some(var) = variables.get(var_name) {
                            let val = builder.use_var(*var);
                            builder.ins().store(MemFlags::trusted(), val, ptr, offset);

                            if let Some(var_type) = variable_types.get(var_name) {
                                let type_size = Self::variable_type_to_cranelift_type(var_type);
                                offset += type_size.bytes() as i32;
                            }
                        }
                    }

                    Some(ptr)
                } else {
                    None
                };

                // Declare the closure function
                let closure_func_id = module.declare_function(&closure_name, Linkage::Local, &sig)
                    .map_err(CodegenError::ModuleError)?;

                // Generate the closure function body
                {
                    let mut ctx = module.make_context();
                    let mut fn_builder_ctx = FunctionBuilderContext::new();
                    ctx.func.signature = sig.clone();

                    let mut closure_builder = FunctionBuilder::new(&mut ctx.func, &mut fn_builder_ctx);
                    let entry_block = closure_builder.create_block();
                    closure_builder.switch_to_block(entry_block);

                    // If there are captures, append block parameter for context
                    let ctx_param = if has_captures {
                        Some(closure_builder.append_block_param(entry_block, I64))
                    } else {
                        None
                    };

                    closure_builder.seal_block(entry_block);

                    // Generate the body
                    let mut closure_variables = HashMap::new();
                    let mut closure_variable_types = HashMap::new();
                    let mut closure_variable_counter = 0;

                    // Extract captured variables from context
                    if let Some(ctx_val) = ctx_param {
                        let mut offset = 0i32;
                        for var_name in &captured_vars {
                            if let Some(var_type) = variable_types.get(var_name) {
                                let cranelift_type = Self::variable_type_to_cranelift_type(var_type);
                                let loaded_val = closure_builder.ins().load(cranelift_type, MemFlags::trusted(), ctx_val, offset);

                                let var = Variable::from_u32(closure_variable_counter);
                                closure_variable_counter += 1;
                                closure_builder.declare_var(var, cranelift_type);
                                closure_builder.def_var(var, loaded_val);
                                closure_variables.insert(var_name.clone(), var);
                                closure_variable_types.insert(var_name.clone(), var_type.clone());

                                offset += cranelift_type.bytes() as i32;
                            }
                        }
                    }

                    // Special handling for Block expressions (the common case for spawn blocks)
                    if let Expression::Block(block) = body.as_ref() {
                        // Generate statements in the block
                        let empty_type_aliases = HashMap::new(); // No type aliases in closure scope
                        let mut has_return = false;
                        for stmt in &block.statements {
                            has_return |= Self::generate_statement_helper(
                                &mut closure_builder,
                                stmt,
                                &mut closure_variables,
                                &mut closure_variable_types,
                                &mut closure_variable_counter,
                                functions,
                                module,
                                string_counter,
                                class_metadata,
                                &empty_type_aliases,
                                &closure_name,
                                &Some(return_ast_type.clone()),
                                test_mode
                            )?;
                        }

                        // If the block didn't have a return, add a default return
                        if !has_return {
                            let default_val = match cranelift_return_type {
                                I32 => closure_builder.ins().iconst(I32, 0),
                                I64 => closure_builder.ins().iconst(I64, 0),
                                F32 => closure_builder.ins().f32const(0.0),
                                F64 => closure_builder.ins().f64const(0.0),
                                _ => closure_builder.ins().iconst(I64, 0),
                            };
                            closure_builder.ins().return_(&[default_val]);
                        }
                    } else {
                        // For non-block expressions, generate as expression
                        let result_val = Self::generate_expression_helper(
                            &mut closure_builder,
                            body,
                            &closure_variables,
                            &closure_variable_types,
                            functions,
                            module,
                            string_counter,
                            &mut closure_variable_counter,
                            class_metadata,
                            test_mode
                        )?;

                        closure_builder.ins().return_(&[result_val]);
                    }

                    // Finalize the closure function
                    closure_builder.finalize();

                    module.define_function(closure_func_id, &mut ctx)
                        .map_err(CodegenError::ModuleError)?;
                }

                // Get the appropriate spawn function name based on return type and captures
                let spawn_func_name = if has_captures {
                    match closure_return_type {
                        VariableType::Bool => "plat_spawn_task_bool_ctx",
                        VariableType::Int32 => "plat_spawn_task_i32_ctx",
                        VariableType::Int64 => "plat_spawn_task_i64_ctx",
                        VariableType::Float32 => "plat_spawn_task_f32_ctx",
                        VariableType::Float64 => "plat_spawn_task_f64_ctx",
                        _ => "plat_spawn_task_i64_ctx", // Default
                    }
                } else {
                    Self::get_spawn_function_name(&closure_return_type)
                };

                let spawn_func_id = if let Some(&func_id) = functions.get(spawn_func_name) {
                    func_id
                } else {
                    // Declare the spawn function
                    let mut spawn_sig = module.make_signature();
                    spawn_sig.call_conv = CallConv::SystemV;
                    spawn_sig.params.push(AbiParam::new(I64)); // Function pointer
                    if has_captures {
                        spawn_sig.params.push(AbiParam::new(I64)); // Context pointer
                    }
                    spawn_sig.returns.push(AbiParam::new(I64)); // Task handle

                    let func_id = module.declare_function(spawn_func_name, Linkage::Import, &spawn_sig)
                        .map_err(CodegenError::ModuleError)?;
                    func_id
                };

                // Get the closure function pointer
                let closure_func_ref = module.declare_func_in_func(closure_func_id, builder.func);
                let closure_ptr = builder.ins().func_addr(I64, closure_func_ref);

                // Call spawn function
                let spawn_func_ref = module.declare_func_in_func(spawn_func_id, builder.func);
                let spawn_args = if let Some(ctx) = ctx_ptr {
                    vec![closure_ptr, ctx]
                } else {
                    vec![closure_ptr]
                };
                let call = builder.ins().call(spawn_func_ref, &spawn_args);
                let task_handle = builder.inst_results(call)[0];

                Ok(task_handle)
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
        variable_counter: &mut u32,
        class_metadata: &HashMap<String, ClassMetadata>,
        test_mode: bool
    ) -> Result<Value, CodegenError> {
        if elements.is_empty() {
            // For empty arrays, determine type from annotation or default to i32
            let element_type = if let Some(AstType::List(element_type)) = expected_type {
                element_type.as_ref()
            } else {
                &AstType::Int32 // default
            };

            let function_name = match element_type {
                AstType::Bool => "plat_array_create_bool",
                AstType::Int8 => "plat_array_create_i8",
                AstType::Int16 => "plat_array_create_i16",
                AstType::Int32 => "plat_array_create_i32",
                AstType::Int64 => "plat_array_create_i64",
                AstType::Float8 => "plat_array_create_f32", // Using f32 for 8-bit float
                AstType::Float16 => "plat_array_create_f32", // Using f32 for 16-bit float
                AstType::Float32 => "plat_array_create_f32",
                AstType::Float64 => "plat_array_create_f64",
                AstType::String => "plat_array_create_string",
                AstType::Named(_, _) => "plat_array_create_class", // Custom class types
                _ => "plat_array_create_i32", // fallback for unknown types
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
                Expression::Literal(Literal::Integer(value, _, _)) => {
                    if *value > i32::MAX as i64 || *value < i32::MIN as i64 {
                        &AstType::Int64
                    } else {
                        &AstType::Int32
                    }
                },
                _ => &AstType::Int32,
            }
        };

        let (element_size, function_name) = match element_type {
            AstType::Bool => (std::mem::size_of::<bool>(), "plat_array_create_bool"),
            AstType::Int8 => (1, "plat_array_create_i8"),
            AstType::Int16 => (2, "plat_array_create_i16"),
            AstType::Int32 => (std::mem::size_of::<i32>(), "plat_array_create_i32"),
            AstType::Int64 => (std::mem::size_of::<i64>(), "plat_array_create_i64"),
            AstType::Float8 => (std::mem::size_of::<f32>(), "plat_array_create_f32"), // Using f32 for 8-bit float
            AstType::Float16 => (std::mem::size_of::<f32>(), "plat_array_create_f32"), // Using f32 for 16-bit float
            AstType::Float32 => (std::mem::size_of::<f32>(), "plat_array_create_f32"),
            AstType::Float64 => (std::mem::size_of::<f64>(), "plat_array_create_f64"),
            AstType::String => (std::mem::size_of::<*const u8>(), "plat_array_create_string"),
            AstType::Named(_, _) => (std::mem::size_of::<*const u8>(), "plat_array_create_class"), // Custom class pointers
            _ => (std::mem::size_of::<i32>(), "plat_array_create_i32"), // fallback
        };

        // Generate all element values
        let mut element_values = Vec::new();
        for element in elements {
            let element_val = Self::generate_expression_helper(builder, element, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
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
        variable_counter: &mut u32,
        class_metadata: &HashMap<String, ClassMetadata>,
        test_mode: bool
    ) -> Result<Value, CodegenError> {
        match literal {
            Literal::Bool(b, _) => {
                let val = if *b { 1i64 } else { 0i64 };
                Ok(builder.ins().iconst(I32, val))
            }
            Literal::Integer(i, int_type, _) => {
                match int_type {
                    IntType::I32 => Ok(builder.ins().iconst(I32, *i)),
                    IntType::I64 => Ok(builder.ins().iconst(I64, *i)),
                }
            }
            Literal::Float(f, float_type, _) => {
                match float_type {
                    FloatType::F32 => Ok(builder.ins().f32const(*f as f32)),
                    FloatType::F64 => Ok(builder.ins().f64const(*f)),
                }
            }
            Literal::String(s, _) => {
                // Allocate string on GC heap using atomic allocation (strings are pointer-free)

                // Declare plat_gc_alloc_atomic function - optimized for pointer-free data
                let gc_alloc_name = "plat_gc_alloc_atomic";
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

                // Call plat_gc_alloc_atomic to allocate memory (no pointer scanning needed)
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
                                builder, expr, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode
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
                        // Float literals need to be converted to strings
                        Expression::Literal(Literal::Float(_, FloatType::F32, _)) => {
                            let convert_sig = {
                                let mut sig = module.make_signature();
                                sig.call_conv = CallConv::SystemV;
                                sig.params.push(AbiParam::new(F32));
                                sig.returns.push(AbiParam::new(I64));
                                sig
                            };
                            let convert_id = module.declare_function("plat_f32_to_string", Linkage::Import, &convert_sig)
                                .map_err(CodegenError::ModuleError)?;
                            let convert_ref = module.declare_func_in_func(convert_id, builder.func);
                            let call = builder.ins().call(convert_ref, &[expr_val]);
                            builder.inst_results(call)[0]
                        }
                        Expression::Literal(Literal::Float(_, FloatType::F64, _)) => {
                            let convert_sig = {
                                let mut sig = module.make_signature();
                                sig.call_conv = CallConv::SystemV;
                                sig.params.push(AbiParam::new(F64));
                                sig.returns.push(AbiParam::new(I64));
                                sig
                            };
                            let convert_id = module.declare_function("plat_f64_to_string", Linkage::Import, &convert_sig)
                                .map_err(CodegenError::ModuleError)?;
                            let convert_ref = module.declare_func_in_func(convert_id, builder.func);
                            let call = builder.ins().call(convert_ref, &[expr_val]);
                            builder.inst_results(call)[0]
                        }
                        Expression::Identifier { name, .. } => {
                            // Use the variable type information to determine conversion
                            match variable_types.get(name) {
                                Some(VariableType::String) => {
                                    // String variable, use directly
                                    expr_val
                                }
                                Some(VariableType::Array(_)) => {
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
                                Some(VariableType::Int8) | Some(VariableType::Int16) | Some(VariableType::Int32) | Some(VariableType::Bool) => {
                                    // I8/I16/I32/boolean variable, convert to string
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
                                Some(VariableType::Int64) => {
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
                                Some(VariableType::Float8) | Some(VariableType::Float16) | Some(VariableType::Float32) => {
                                    // F8/F16/F32 variable, convert to string (using f32)
                                    let convert_sig = {
                                        let mut sig = module.make_signature();
                                        sig.call_conv = CallConv::SystemV;
                                        sig.params.push(AbiParam::new(F32));
                                        sig.returns.push(AbiParam::new(I64));
                                        sig
                                    };
                                    let convert_id = module.declare_function("plat_f32_to_string", Linkage::Import, &convert_sig)
                                        .map_err(CodegenError::ModuleError)?;
                                    let convert_ref = module.declare_func_in_func(convert_id, builder.func);
                                    let call = builder.ins().call(convert_ref, &[expr_val]);
                                    builder.inst_results(call)[0]
                                }
                                Some(VariableType::Float64) => {
                                    // F64 variable, convert to string
                                    let convert_sig = {
                                        let mut sig = module.make_signature();
                                        sig.call_conv = CallConv::SystemV;
                                        sig.params.push(AbiParam::new(F64));
                                        sig.returns.push(AbiParam::new(I64));
                                        sig
                                    };
                                    let convert_id = module.declare_function("plat_f64_to_string", Linkage::Import, &convert_sig)
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
                                Some(VariableType::Task(_)) => {
                                    // Task variable (task handle), convert to string as i64
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
                            } else if val_type == F32 {
                                // F32 value, convert to string
                                let convert_sig = {
                                    let mut sig = module.make_signature();
                                    sig.call_conv = CallConv::SystemV;
                                    sig.params.push(AbiParam::new(F32));
                                    sig.returns.push(AbiParam::new(I64));
                                    sig
                                };
                                let convert_id = module.declare_function("plat_f32_to_string", Linkage::Import, &convert_sig)
                                    .map_err(CodegenError::ModuleError)?;
                                let convert_ref = module.declare_func_in_func(convert_id, builder.func);
                                let call = builder.ins().call(convert_ref, &[expr_val]);
                                builder.inst_results(call)[0]
                            } else if val_type == F64 {
                                // F64 value, convert to string
                                let convert_sig = {
                                    let mut sig = module.make_signature();
                                    sig.call_conv = CallConv::SystemV;
                                    sig.params.push(AbiParam::new(F64));
                                    sig.returns.push(AbiParam::new(I64));
                                    sig
                                };
                                let convert_id = module.declare_function("plat_f64_to_string", Linkage::Import, &convert_sig)
                                    .map_err(CodegenError::ModuleError)?;
                                let convert_ref = module.declare_func_in_func(convert_id, builder.func);
                                let call = builder.ins().call(convert_ref, &[expr_val]);
                                builder.inst_results(call)[0]
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
                    let element_val = Self::generate_expression_helper(builder, element, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
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
                    let key_val = Self::generate_expression_helper(builder, key_expr, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
                    keys.push(key_val);

                    // Evaluate value
                    let value_val = Self::generate_expression_helper(builder, value_expr, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
                    values.push(value_val);

                    // Determine value type (simplified - assuming i32 values for now)
                    let type_val = match value_expr {
                        Expression::Literal(Literal::Bool(_, _)) => 2u8, // DICT_VALUE_TYPE_BOOL
                        Expression::Literal(Literal::Integer(val, _, _)) => {
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
                    let value_val = Self::generate_expression_helper(builder, element_expr, variables, variable_types, functions, module, string_counter, variable_counter, class_metadata, test_mode)?;
                    values.push(value_val);

                    // Determine value type
                    let type_val = match element_expr {
                        Expression::Literal(Literal::Bool(_, _)) => 2u8, // SET_VALUE_TYPE_BOOL
                        Expression::Literal(Literal::Integer(val, _, _)) => {
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

    /// Check if a type is Result<Int*, E> or Option<Int*>
    fn is_result_or_option_with_int_return(ty: &AstType) -> bool {
        match ty {
            AstType::Named(name, type_params) if name == "Result" && type_params.len() >= 1 => {
                // Check if first type parameter is an integer type
                matches!(&type_params[0], AstType::Int8 | AstType::Int16 | AstType::Int32 | AstType::Int64)
            }
            AstType::Named(name, type_params) if name == "Option" && type_params.len() == 1 => {
                // Check if type parameter is an integer type
                matches!(&type_params[0], AstType::Int8 | AstType::Int16 | AstType::Int32 | AstType::Int64)
            }
            _ => false
        }
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
            Expression::Literal(Literal::Integer(_, _, _)) => DICT_VALUE_TYPE_I32,
            Expression::Literal(Literal::String(_, _)) | Expression::Literal(Literal::InterpolatedString(_, _)) => DICT_VALUE_TYPE_STRING,
            Expression::Literal(Literal::Bool(_, _)) => DICT_VALUE_TYPE_BOOL,
            Expression::Identifier { name, .. } => {
                // Look up variable type
                if let Some(var_type) = variable_types.get(name) {
                    match var_type {
                        VariableType::Int8 | VariableType::Int16 | VariableType::Int32 => DICT_VALUE_TYPE_I32,
                        VariableType::Int64 => DICT_VALUE_TYPE_I64,
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
                    matches!(var_type, VariableType::Array(_))
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
            Expression::Self_ { .. } => {
                // Look up 'self' in variable_types
                if let Some(VariableType::Class(class_name)) = variable_types.get("self") {
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
            Expression::Literal(Literal::Integer(_, _, _)) => SET_VALUE_TYPE_I32,
            Expression::Literal(Literal::String(_, _)) | Expression::Literal(Literal::InterpolatedString(_, _)) => SET_VALUE_TYPE_STRING,
            Expression::Literal(Literal::Bool(_, _)) => SET_VALUE_TYPE_BOOL,
            Expression::Identifier { name, .. } => {
                // Look up variable type
                if let Some(var_type) = variable_types.get(name) {
                    match var_type {
                        VariableType::Int8 | VariableType::Int16 | VariableType::Int32 => SET_VALUE_TYPE_I32,
                        VariableType::Int64 => SET_VALUE_TYPE_I64,
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
    AssertError(String),
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
            CodegenError::AssertError(msg) => write!(f, "Assert error: {}", msg),
        }
    }
}

impl std::error::Error for CodegenError {}
