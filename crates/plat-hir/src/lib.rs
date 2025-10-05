#[cfg(test)]
mod tests;

use plat_ast::*;
use plat_diags::{Diagnostic, DiagnosticError};
use plat_lexer::Span;
use std::collections::{HashMap, HashSet};

/// Validates that a name follows snake_case convention
fn is_snake_case(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }

    // Must start with lowercase letter or underscore
    let first_char = name.chars().next().unwrap();
    if !first_char.is_lowercase() && first_char != '_' {
        return false;
    }

    // Can only contain lowercase letters, digits, and underscores
    name.chars().all(|c| c.is_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Validates that a name follows TitleCase convention
fn is_title_case(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }

    // Must start with uppercase letter
    let first_char = name.chars().next().unwrap();
    if !first_char.is_uppercase() {
        return false;
    }

    // Can contain letters and digits, no underscores
    name.chars().all(|c| c.is_alphanumeric())
}

/// Convert a name to snake_case (simple heuristic)
fn to_snake_case(name: &str) -> String {
    let mut result = String::new();
    for (i, ch) in name.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(ch.to_lowercase().next().unwrap());
        } else {
            result.push(ch);
        }
    }
    result
}

/// Module-aware symbol table for tracking declarations across modules
#[derive(Debug, Clone)]
pub struct ModuleSymbolTable {
    /// Current module path (e.g., "database::connection")
    pub current_module: String,
    /// Imported modules (from `use` statements)
    pub imports: Vec<String>,
    /// Map of qualified names to their types (e.g., "database::connect" -> function signature)
    pub global_symbols: HashMap<String, Symbol>,
}

#[derive(Debug, Clone)]
pub enum Symbol {
    Function(FunctionSignature),
    Enum(EnumInfo),
    Class(ClassInfo),
}

impl ModuleSymbolTable {
    pub fn new(module_path: String) -> Self {
        Self {
            current_module: module_path,
            imports: Vec::new(),
            global_symbols: HashMap::new(),
        }
    }

    /// Add an import statement
    pub fn add_import(&mut self, module_path: String) {
        self.imports.push(module_path);
    }

    /// Register a symbol in the current module
    pub fn register(&mut self, name: &str, symbol: Symbol) {
        let qualified_name = if self.current_module.is_empty() {
            name.to_string()
        } else {
            format!("{}::{}", self.current_module, name)
        };
        self.global_symbols.insert(qualified_name, symbol);
    }

    /// Resolve a name to a qualified name using imports
    pub fn resolve(&self, name: &str) -> Option<String> {
        // Check if it's already qualified
        if name.contains("::") {
            return Some(name.to_string());
        }

        // Check in current module first
        let current_qualified = if self.current_module.is_empty() {
            name.to_string()
        } else {
            format!("{}::{}", self.current_module, name)
        };

        if self.global_symbols.contains_key(&current_qualified) {
            return Some(current_qualified);
        }

        // Check in imported modules
        for import in &self.imports {
            let qualified = format!("{}::{}", import, name);
            if self.global_symbols.contains_key(&qualified) {
                return Some(qualified);
            }
        }

        // Check global scope (no module prefix)
        if self.global_symbols.contains_key(name) {
            return Some(name.to_string());
        }

        None
    }
}

pub struct TypeChecker {
    scopes: Vec<HashMap<String, HirType>>,
    functions: HashMap<String, FunctionSignature>,
    enums: HashMap<String, EnumInfo>,
    classes: HashMap<String, ClassInfo>,
    type_aliases: HashMap<String, HirType>, // Type alias name -> resolved type
    newtypes: HashMap<String, HirType>, // Newtype name -> underlying type (distinct from aliases)
    current_function_return_type: Option<HirType>,
    current_class_context: Option<String>, // Track which class we're currently type-checking
    current_method_is_init: bool, // Track if we're currently in an init method
    type_parameters: Vec<String>, // Track current type parameters in scope (like T, U)
    monomorphizer: Monomorphizer, // For generic type specialization
    module_table: ModuleSymbolTable, // Module-aware symbol table
    require_main: bool, // Whether to require a main function (false for library modules)
    test_mode: bool, // Whether we're in test mode (compiling tests)
    bench_mode: bool, // Whether we're in bench mode (compiling benchmarks)
    test_block_names: HashSet<String>, // Track test block names for uniqueness validation
    bench_block_names: HashSet<String>, // Track bench block names for uniqueness validation
    in_concurrent_block: bool, // Track if we're currently inside a concurrent block (for spawn validation)
    filename: String, // Source filename for error reporting
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HirType {
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
    List(Box<HirType>),
    Dict(Box<HirType>, Box<HirType>), // key type, value type
    Set(Box<HirType>), // element type
    Enum(String, Vec<HirType>), // name, type parameters
    Class(String, Vec<HirType>), // name, type parameters
    TypeParameter(String), // For generic type parameters like T, U, etc.
    Newtype(String), // Distinct type wrapping another type
    Task(Box<HirType>), // Task<T> for concurrent spawn expressions
    Channel(Box<HirType>), // Channel<T> for message passing between tasks
    Unit, // For functions that don't return anything
}

#[derive(Debug, Clone)]
pub struct FunctionSignature {
    pub type_params: Vec<String>, // Generic type parameters
    pub params: Vec<(String, HirType)>, // (param_name, param_type)
    pub default_values: Vec<Option<Expression>>, // default values for parameters
    pub return_type: HirType,
    pub is_mutable: bool,
    pub is_public: bool, // true if function/method is public
}

#[derive(Debug, Clone)]
pub struct EnumInfo {
    pub name: String,
    pub type_params: Vec<String>,
    pub variants: HashMap<String, Vec<HirType>>, // variant name -> field types
    pub methods: HashMap<String, FunctionSignature>,
    pub is_public: bool, // true if enum is public
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
    pub is_public: bool, // true if class is public
}

#[derive(Debug, Clone)]
pub struct FieldInfo {
    pub ty: HirType,
    pub is_mutable: bool,
    pub is_public: bool, // true if field is public
}

impl TypeChecker {
    pub fn new() -> Self {
        Self::with_module(String::new())
    }

    /// Create a TypeChecker for a specific module
    pub fn with_module(module_path: String) -> Self {
        let mut checker = Self {
            scopes: vec![HashMap::new()], // Global scope
            functions: HashMap::new(),
            enums: HashMap::new(),
            classes: HashMap::new(),
            type_aliases: HashMap::new(),
            newtypes: HashMap::new(),
            current_function_return_type: None,
            current_class_context: None,
            current_method_is_init: false,
            type_parameters: Vec::new(),
            monomorphizer: Monomorphizer::new(),
            module_table: ModuleSymbolTable::new(module_path),
            require_main: true, // Default: require main function
            test_mode: false, // Default: not in test mode
            bench_mode: false, // Default: not in bench mode
            test_block_names: HashSet::new(), // Track test block names
            bench_block_names: HashSet::new(), // Track bench block names
            in_concurrent_block: false, // Default: not in concurrent block
            filename: "<unknown>".to_string(), // Default filename
        };

        // Register built-in Option<T> type
        checker.register_builtin_option();

        // Register built-in Result<T, E> type
        checker.register_builtin_result();

        checker
    }

    /// Create a TypeChecker with pre-populated global symbols
    pub fn with_symbols(module_table: ModuleSymbolTable) -> Self {
        let mut checker = Self {
            scopes: vec![HashMap::new()], // Global scope
            functions: HashMap::new(),
            enums: HashMap::new(),
            classes: HashMap::new(),
            type_aliases: HashMap::new(),
            newtypes: HashMap::new(),
            current_function_return_type: None,
            current_class_context: None,
            current_method_is_init: false,
            type_parameters: Vec::new(),
            monomorphizer: Monomorphizer::new(),
            module_table,
            require_main: false, // Multi-module: don't require main in every module
            test_mode: false, // Default: not in test mode
            bench_mode: false, // Default: not in bench mode
            test_block_names: HashSet::new(), // Track test block names
            bench_block_names: HashSet::new(), // Track bench block names
            in_concurrent_block: false, // Default: not in concurrent block
            filename: "<unknown>".to_string(), // Default filename
        };

        // Register built-in Option<T> type
        checker.register_builtin_option();

        // Register built-in Result<T, E> type
        checker.register_builtin_result();

        // Load all symbols from the module table into local maps
        checker.load_symbols_from_module_table();

        checker
    }

    /// Enable test mode (allows test blocks and disables main function requirement)
    pub fn with_test_mode(mut self) -> Self {
        self.test_mode = true;
        self.require_main = false; // Tests don't need main function
        self
    }

    pub fn with_bench_mode(mut self) -> Self {
        self.bench_mode = true;
        self.require_main = false; // Benchmarks don't need main function
        self
    }

    /// Set the filename for error reporting
    pub fn with_filename(mut self, filename: impl Into<String>) -> Self {
        self.filename = filename.into();
        self
    }

    /// Load symbols from the module table into local type checker maps
    /// Only loads symbols from the current module and imported modules
    fn load_symbols_from_module_table(&mut self) {
        let current_module = &self.module_table.current_module;
        let imports = &self.module_table.imports;

        for (qualified_name, symbol) in &self.module_table.global_symbols {
            // Check if this symbol is from the current module or an imported module
            let should_load = if current_module.is_empty() {
                // Root module: load all unqualified symbols
                !qualified_name.contains("::")
            } else {
                // Check if symbol is from current module or imported modules
                qualified_name.starts_with(&format!("{}::", current_module))
                    || imports.iter().any(|imp| qualified_name.starts_with(&format!("{}::", imp)))
            };

            if should_load {
                match symbol {
                    Symbol::Function(sig) => {
                        self.functions.insert(qualified_name.clone(), sig.clone());
                    }
                    Symbol::Enum(info) => {
                        self.enums.insert(qualified_name.clone(), info.clone());
                    }
                    Symbol::Class(info) => {
                        self.classes.insert(qualified_name.clone(), info.clone());
                    }
                }
            }
        }
    }

    /// Add an import to the module table
    pub fn add_import(&mut self, module_path: String) {
        self.module_table.add_import(module_path);
    }

    /// Check if we can access a field from the current context
    /// Fields are private by default and only accessible from within the same class
    fn can_access_field(&self, class_name: &str, field_is_public: bool) -> bool {
        // Public fields are always accessible
        if field_is_public {
            return true;
        }

        // Private fields are only accessible from within the same class
        match &self.current_class_context {
            Some(current_class) => current_class == class_name,
            None => false,
        }
    }

    /// Check if we can access a method from the current context
    /// Methods are private by default and only accessible from within the same class or module
    fn can_access_method(&self, class_name: &str, method_is_public: bool) -> bool {
        // Public methods are always accessible
        if method_is_public {
            return true;
        }

        // Private methods are only accessible from within the same class
        match &self.current_class_context {
            Some(current_class) => current_class == class_name,
            None => false,
        }
    }

    /// Check if we can access a symbol from the current module context
    /// Symbols are private by default and only accessible from within the same module
    fn can_access_symbol(&self, symbol_module: &str, is_public: bool) -> bool {
        // Public symbols are always accessible
        if is_public {
            return true;
        }

        // Private symbols are only accessible from within the same module
        self.module_table.current_module == symbol_module
    }

    /// Extract the module path from a qualified name (e.g., "database::connect" -> "database")
    fn get_module_from_qualified_name(&self, qualified_name: &str) -> String {
        if let Some(pos) = qualified_name.rfind("::") {
            qualified_name[..pos].to_string()
        } else {
            // No module qualifier, assume current module
            self.module_table.current_module.clone()
        }
    }

    /// Collect all top-level symbols from a program into the module symbol table
    pub fn collect_symbols_from_program(
        &mut self,
        program: &Program,
        module_path: &str,
        global_symbols: &mut ModuleSymbolTable,
    ) -> Result<(), DiagnosticError> {
        // Update the module table's current module
        global_symbols.current_module = module_path.to_string();

        // Collect all function declarations
        for func in &program.functions {
            // Build function signature
            let params: Result<Vec<(String, HirType)>, _> = func.params.iter()
                .map(|p| Ok((p.name.clone(), self.ast_type_to_hir_type(&p.ty)?)))
                .collect();
            let params = params?;

            let return_type = if let Some(ref rt) = func.return_type {
                self.ast_type_to_hir_type(rt)?
            } else {
                HirType::Unit
            };

            let default_values: Vec<Option<Expression>> = func.params.iter()
                .map(|p| p.default_value.clone())
                .collect();

            let sig = FunctionSignature {
                type_params: func.type_params.clone(),
                params,
                default_values,
                return_type,
                is_mutable: func.is_mutable,
                is_public: func.is_public,
            };

            global_symbols.register(&func.name, Symbol::Function(sig));
        }

        // Collect test functions if in test mode
        if self.test_mode {
            for test_block in &program.test_blocks {
                for func in &test_block.functions {
                    // Build function signature
                    let params: Result<Vec<(String, HirType)>, _> = func.params.iter()
                        .map(|p| Ok((p.name.clone(), self.ast_type_to_hir_type(&p.ty)?)))
                        .collect();
                    let params = params?;

                    let return_type = if let Some(ref rt) = func.return_type {
                        self.ast_type_to_hir_type(rt)?
                    } else {
                        HirType::Unit
                    };

                    let default_values: Vec<Option<Expression>> = func.params.iter()
                        .map(|p| p.default_value.clone())
                        .collect();

                    let sig = FunctionSignature {
                        type_params: func.type_params.clone(),
                        params,
                        default_values,
                        return_type,
                        is_mutable: func.is_mutable,
                        is_public: func.is_public,
                    };

                    global_symbols.register(&func.name, Symbol::Function(sig));
                }
            }
        }

        // Collect bench functions if in bench mode
        if self.bench_mode {
            for bench_block in &program.bench_blocks {
                for func in &bench_block.functions {
                    // Build function signature
                    let params: Result<Vec<(String, HirType)>, _> = func.params.iter()
                        .map(|p| Ok((p.name.clone(), self.ast_type_to_hir_type(&p.ty)?)))
                        .collect();
                    let params = params?;

                    let return_type = if let Some(ref rt) = func.return_type {
                        self.ast_type_to_hir_type(rt)?
                    } else {
                        HirType::Unit
                    };

                    let default_values: Vec<Option<Expression>> = func.params.iter()
                        .map(|p| p.default_value.clone())
                        .collect();

                    let sig = FunctionSignature {
                        type_params: func.type_params.clone(),
                        params,
                        default_values,
                        return_type,
                        is_mutable: func.is_mutable,
                        is_public: func.is_public,
                    };

                    global_symbols.register(&func.name, Symbol::Function(sig));
                }
            }
        }

        // Collect all enum declarations
        for enum_decl in &program.enums {
            // Build enum info (simplified for now)
            let mut variants = HashMap::new();
            for variant in &enum_decl.variants {
                let field_types: Result<Vec<HirType>, _> = variant.fields.iter()
                    .map(|f| self.ast_type_to_hir_type(f))
                    .collect();
                let field_types = field_types?;
                variants.insert(variant.name.clone(), field_types);
            }

            let enum_info = EnumInfo {
                name: enum_decl.name.clone(),
                type_params: enum_decl.type_params.clone(),
                variants,
                methods: HashMap::new(), // Methods will be populated later
                is_public: enum_decl.is_public,
            };

            global_symbols.register(&enum_decl.name, Symbol::Enum(enum_info));
        }

        // Collect all class declarations
        for class_decl in &program.classes {
            // Build class info (simplified for now)
            let mut fields = HashMap::new();
            for field in &class_decl.fields {
                let field_info = FieldInfo {
                    ty: self.ast_type_to_hir_type(&field.ty)?,
                    is_mutable: field.is_mutable,
                    is_public: field.is_public,
                };
                fields.insert(field.name.clone(), field_info);
            }

            let class_info = ClassInfo {
                name: class_decl.name.clone(),
                type_params: class_decl.type_params.clone(),
                parent_class: class_decl.parent_class.clone(),
                fields,
                methods: HashMap::new(), // Methods will be populated later
                virtual_methods: HashMap::new(),
                is_public: class_decl.is_public,
            };

            global_symbols.register(&class_decl.name, Symbol::Class(class_info));
        }

        Ok(())
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
            is_public: true, // Built-in types are always public
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
            is_public: true, // Built-in types are always public
        };

        self.enums.insert("Result".to_string(), result_info);
    }

    pub fn check_program(mut self, program: &mut Program) -> Result<(), DiagnosticError> {
        // Process module declaration (if present)
        if let Some(module_decl) = &program.module_decl {
            // Validate module path components follow snake_case
            for component in &module_decl.path {
                if !is_snake_case(component) {
                    return Err(DiagnosticError::Type(
                        format!("Module name '{}' must be snake_case", component)
                    ));
                }
            }
            self.module_table.current_module = module_decl.path.join("::");
        }

        // Process use declarations (imports)
        for use_decl in &program.use_decls {
            // Validate imported module path components follow snake_case
            for component in &use_decl.path {
                if !is_snake_case(component) {
                    return Err(DiagnosticError::Type(
                        format!("Module name '{}' must be snake_case", component)
                    ));
                }
            }
            self.module_table.add_import(use_decl.path.join("::"));
        }

        // Process type aliases
        for type_alias in &program.type_aliases {
            self.collect_type_alias(type_alias)?;
        }

        // Process newtypes
        for newtype in &program.newtypes {
            self.collect_newtype(newtype)?;
        }

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

        // Collect test function signatures if in test mode
        if self.test_mode {
            for test_block in &program.test_blocks {
                for function in &test_block.functions {
                    self.collect_function_signature(function)?;
                }
            }
        }

        // Collect bench function signatures if in bench mode
        if self.bench_mode {
            for bench_block in &program.bench_blocks {
                for function in &bench_block.functions {
                    self.collect_function_signature(function)?;
                }
            }
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

        // Check that main function exists (only if required)
        if self.require_main && !self.functions.contains_key("main") {
            return Err(DiagnosticError::Type(
                "Program must have a main function".to_string()
            ));
        }

        // Validate main function signature (only if main exists)
        if self.functions.contains_key("main") {
            let main_sig = &self.functions["main"];
            if !main_sig.params.is_empty() {
                return Err(DiagnosticError::Type(
                    "Main function must have no parameters".to_string()
                ));
            }
            // Main can return Unit, Int32, Option<Int32>, Result<Int32, E>, or Result<(), E>
            let valid_return_type = match &main_sig.return_type {
                HirType::Unit => true,
                HirType::Int32 => true,
                HirType::Enum(name, type_params) if name == "Option" => {
                    // Allow Option<Int32> or Option<()>
                    type_params.len() == 1 && (type_params[0] == HirType::Int32 || type_params[0] == HirType::Unit)
                }
                HirType::Enum(name, type_params) if name == "Result" => {
                    // Allow Result<Int32, E> or Result<(), E> for any error type E
                    type_params.len() == 2 && (type_params[0] == HirType::Int32 || type_params[0] == HirType::Unit)
                }
                _ => false,
            };

            if !valid_return_type {
                return Err(DiagnosticError::Type(
                    format!("Main function must return Unit, Int32, Option<Int32>, Option<()>, Result<Int32, E>, or Result<(), E>, got {:?}", main_sig.return_type)
                ));
            }
        }

        // Fill in default arguments for all calls before type checking
        self.fill_default_arguments(program);

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

        // Type check test blocks (only in test mode)
        if self.test_mode {
            for test_block in &program.test_blocks {
                self.check_test_block(test_block)?;
            }
        }

        // Type check bench blocks (only in bench mode)
        if self.bench_mode {
            for bench_block in &program.bench_blocks {
                self.check_bench_block(bench_block)?;
            }
        }

        Ok(())
    }

    fn collect_type_alias(&mut self, type_alias: &TypeAlias) -> Result<(), DiagnosticError> {
        // Validate type alias name follows TitleCase
        if !is_title_case(&type_alias.name) {
            return Err(DiagnosticError::Type(
                format!("Type alias name '{}' must be TitleCase", type_alias.name)
            ));
        }

        // Check for duplicate type alias definitions
        if self.type_aliases.contains_key(&type_alias.name) {
            return Err(DiagnosticError::Type(
                format!("Type alias '{}' is already defined", type_alias.name)
            ));
        }

        // Check for conflicts with enums and classes
        if self.enums.contains_key(&type_alias.name) {
            return Err(DiagnosticError::Type(
                format!("Type alias '{}' conflicts with an existing enum", type_alias.name)
            ));
        }
        if self.classes.contains_key(&type_alias.name) {
            return Err(DiagnosticError::Type(
                format!("Type alias '{}' conflicts with an existing class", type_alias.name)
            ));
        }

        // Resolve the aliased type
        let resolved_type = self.ast_type_to_hir_type(&type_alias.ty)?;
        self.type_aliases.insert(type_alias.name.clone(), resolved_type);

        Ok(())
    }

    fn collect_newtype(&mut self, newtype: &NewtypeDecl) -> Result<(), DiagnosticError> {
        // Validate newtype name follows TitleCase
        if !is_title_case(&newtype.name) {
            return Err(DiagnosticError::Type(
                format!("Newtype name '{}' must be TitleCase", newtype.name)
            ));
        }

        // Check for duplicate newtype definitions
        if self.newtypes.contains_key(&newtype.name) {
            return Err(DiagnosticError::Type(
                format!("Newtype '{}' is already defined", newtype.name)
            ));
        }

        // Check for conflicts with enums, classes, and type aliases
        if self.enums.contains_key(&newtype.name) {
            return Err(DiagnosticError::Type(
                format!("Newtype '{}' conflicts with an existing enum", newtype.name)
            ));
        }
        if self.classes.contains_key(&newtype.name) {
            return Err(DiagnosticError::Type(
                format!("Newtype '{}' conflicts with an existing class", newtype.name)
            ));
        }
        if self.type_aliases.contains_key(&newtype.name) {
            return Err(DiagnosticError::Type(
                format!("Newtype '{}' conflicts with an existing type alias", newtype.name)
            ));
        }

        // Resolve the underlying type
        let underlying_type = self.ast_type_to_hir_type(&newtype.underlying_type)?;
        self.newtypes.insert(newtype.name.clone(), underlying_type);

        Ok(())
    }

    fn collect_enum_info(&mut self, enum_decl: &EnumDecl) -> Result<(), DiagnosticError> {
        // Validate enum name follows TitleCase
        if !is_title_case(&enum_decl.name) {
            return Err(DiagnosticError::Type(
                format!("Enum name '{}' must be TitleCase", enum_decl.name)
            ));
        }

        // Validate type parameters follow TitleCase
        for type_param in &enum_decl.type_params {
            if !is_title_case(type_param) {
                return Err(DiagnosticError::Type(
                    format!("Type parameter '{}' must be TitleCase", type_param)
                ));
            }
        }

        let mut variants = HashMap::new();
        let mut methods = HashMap::new();

        // Collect variant information
        for variant in &enum_decl.variants {
            // Validate variant name follows TitleCase
            if !is_title_case(&variant.name) {
                return Err(DiagnosticError::Type(
                    format!("Enum variant '{}' must be TitleCase", variant.name)
                ));
            }

            let field_types: Result<Vec<HirType>, DiagnosticError> = variant.fields
                .iter()
                .map(|field_type| self.ast_type_to_hir_type(field_type))
                .collect();

            let field_types = field_types?;

            variants.insert(variant.name.clone(), field_types);
        }

        // Collect method signatures
        for method in &enum_decl.methods {
            let param_types: Result<Vec<(String, HirType)>, DiagnosticError> = method.params
                .iter()
                .map(|param| Ok((param.name.clone(), self.ast_type_to_hir_type(&param.ty)?)))
                .collect();

            let return_type = match &method.return_type {
                Some(ty) => self.ast_type_to_hir_type(ty)?,
                None => HirType::Unit,
            };

            let default_values: Vec<Option<Expression>> = method.params.iter()
                .map(|p| p.default_value.clone())
                .collect();

            let signature = FunctionSignature {
                type_params: method.type_params.clone(), // Store generic type parameters
                params: param_types?,
                default_values,
                return_type,
                is_mutable: method.is_mutable,
                is_public: method.is_public,
            };

            methods.insert(method.name.clone(), signature);
        }

        let enum_info = EnumInfo {
            name: enum_decl.name.clone(),
            type_params: enum_decl.type_params.clone(),
            variants,
            methods,
            is_public: enum_decl.is_public,
        };

        if self.enums.insert(enum_decl.name.clone(), enum_info).is_some() {
            return Err(DiagnosticError::Type(
                format!("Enum '{}' is defined multiple times", enum_decl.name)
            ));
        }

        Ok(())
    }

    fn register_class_name(&mut self, class_decl: &ClassDecl) -> Result<(), DiagnosticError> {
        // Validate class name follows TitleCase
        if !is_title_case(&class_decl.name) {
            return Err(DiagnosticError::Type(
                format!("Class name '{}' must be TitleCase", class_decl.name)
            ));
        }

        // Validate type parameters follow TitleCase
        for type_param in &class_decl.type_params {
            if !is_title_case(type_param) {
                return Err(DiagnosticError::Type(
                    format!("Type parameter '{}' must be TitleCase", type_param)
                ));
            }
        }

        // Just register the class name with empty info for now
        let class_info = ClassInfo {
            name: class_decl.name.clone(),
            type_params: class_decl.type_params.clone(),
            parent_class: class_decl.parent_class.clone(),
            fields: HashMap::new(),
            methods: HashMap::new(),
            virtual_methods: HashMap::new(),
            is_public: class_decl.is_public,
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
            // Validate field name follows snake_case
            if !is_snake_case(&field.name) {
                return Err(DiagnosticError::Type(
                    format!("Field name '{}' must be snake_case", field.name)
                ));
            }

            let field_type = self.ast_type_to_hir_type(&field.ty)?;
            let field_info = FieldInfo {
                ty: field_type,
                is_mutable: field.is_mutable,
                is_public: field.is_public,
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
                param_types.push((param.name.clone(), param_type));
            }

            let return_type = match &method.return_type {
                Some(ty) => self.ast_type_to_hir_type(ty)?,
                None => HirType::Unit,
            };

            let default_values: Vec<Option<Expression>> = method.params.iter()
                .map(|p| p.default_value.clone())
                .collect();

            let signature = FunctionSignature {
                type_params: method.type_params.clone(), // Store generic type parameters
                params: param_types,
                default_values,
                return_type,
                is_mutable: method.is_mutable,
                is_public: method.is_public,
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

        // If no init method was defined, generate a default one
        if !methods.contains_key("init") {
            // Create a default init signature that takes all fields as parameters (in declaration order)
            let mut param_types = Vec::new();
            for field in &class_decl.fields {
                let field_type = self.ast_type_to_hir_type(&field.ty)?;
                param_types.push((field.name.clone(), field_type));
            }

            // Default init returns the class type
            let type_args: Vec<HirType> = class_decl.type_params.iter()
                .map(|param| HirType::TypeParameter(param.clone()))
                .collect();
            let class_type = HirType::Class(class_decl.name.clone(), type_args);

            let default_init_signature = FunctionSignature {
                type_params: vec![], // init methods don't have their own type parameters
                params: param_types.clone(),
                default_values: vec![None; param_types.len()], // no defaults for auto-generated init
                return_type: class_type,
                is_mutable: false,
                is_public: true, // Auto-generated init methods are always public
            };

            // Store in class methods
            methods.insert("init".to_string(), default_init_signature.clone());

            // Also store in global functions map with qualified name
            let method_name = format!("{}::init", class_decl.name);
            self.functions.insert(method_name, default_init_signature);
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
            is_public: class_decl.is_public,
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

    /// Check if derived_class is a subclass of base_class (including transitive inheritance)
    fn is_derived_from(&self, derived_class: &str, base_class: &str) -> bool {
        if derived_class == base_class {
            return true;
        }

        let mut current = Some(derived_class.to_string());
        while let Some(class_name) = current {
            if class_name == base_class {
                return true;
            }
            current = self.classes.get(&class_name)
                .and_then(|info| info.parent_class.clone());
        }

        false
    }

    /// Check if value_type can be assigned to expected_type (with upcasting support)
    fn is_assignable(&self, expected_type: &HirType, value_type: &HirType) -> bool {
        // Exact match is always assignable
        if expected_type == value_type {
            return true;
        }

        // Check for class upcasting: derived class -> base class
        match (expected_type, value_type) {
            (HirType::Class(base_name, base_type_args), HirType::Class(derived_name, derived_type_args)) => {
                // For now, require type arguments to match exactly (no variance)
                // In the future, we could support covariant type parameters
                if base_type_args == derived_type_args {
                    self.is_derived_from(derived_name, base_name)
                } else {
                    false
                }
            }
            _ => false
        }
    }

    fn is_numeric_type(&self, ty: &HirType) -> bool {
        matches!(
            ty,
            HirType::Int8 | HirType::Int16 | HirType::Int32 | HirType::Int64 |
            HirType::Float8 | HirType::Float16 | HirType::Float32 | HirType::Float64
        )
    }

    fn collect_function_signature(&mut self, function: &Function) -> Result<(), DiagnosticError> {
        self.collect_function_signature_with_name(&function.name, function)
    }

    fn collect_function_signature_with_name(&mut self, name: &str, function: &Function) -> Result<(), DiagnosticError> {
        // Validate function name follows snake_case
        let simple_name = name.split("::").last().unwrap_or(name);
        if !is_snake_case(simple_name) {
            return Err(DiagnosticError::Type(
                format!("Function name '{}' must be snake_case", simple_name)
            ));
        }

        // Validate parameter names follow snake_case
        for param in &function.params {
            if !is_snake_case(&param.name) {
                return Err(DiagnosticError::Type(
                    format!("Parameter name '{}' must be snake_case", param.name)
                ));
            }
        }

        // Validate type parameters follow TitleCase
        for type_param in &function.type_params {
            if !is_title_case(type_param) {
                return Err(DiagnosticError::Type(
                    format!("Type parameter '{}' must be TitleCase", type_param)
                ));
            }
        }

        // In multi-module mode, skip if function is already registered from global symbol table
        // In single-module mode, we need to check for duplicates
        if self.functions.contains_key(name) {
            if !self.require_main {
                // Multi-module compilation: function is from global symbol table, skip
                return Ok(());
            } else {
                // Single-module compilation: duplicate definition error
                return Err(DiagnosticError::Type(
                    format!("Function '{}' is defined multiple times", name)
                ));
            }
        }

        // Add function type parameters to scope temporarily
        let old_type_params = self.type_parameters.clone();
        self.type_parameters.extend(function.type_params.iter().cloned());

        // Validate parameter ordering: parameters with defaults must come after parameters without defaults
        let mut seen_default = false;
        for param in &function.params {
            if param.default_value.is_some() {
                seen_default = true;
            } else if seen_default {
                return Err(DiagnosticError::Type(
                    format!("Parameter '{}' without default value cannot follow parameters with default values", param.name)
                ));
            }
        }

        let param_types: Result<Vec<(String, HirType)>, DiagnosticError> = function.params
            .iter()
            .map(|param| Ok((param.name.clone(), self.ast_type_to_hir_type(&param.ty)?)))
            .collect();

        let param_types = param_types?;

        // Type-check default values
        let mut default_values = Vec::new();
        for (param, (_, param_type)) in function.params.iter().zip(param_types.iter()) {
            if let Some(default_expr) = &param.default_value {
                // Check that the default value type matches the parameter type
                let default_type = self.check_expression(default_expr)?;
                if !self.is_assignable(param_type, &default_type) {
                    return Err(DiagnosticError::Type(
                        format!("Default value for parameter '{}' has type {:?}, expected {:?}",
                            param.name, default_type, param_type)
                    ));
                }
                default_values.push(Some(default_expr.clone()));
            } else {
                default_values.push(None);
            }
        }

        let return_type = match &function.return_type {
            Some(ty) => self.ast_type_to_hir_type(ty)?,
            None => HirType::Unit,
        };

        // Restore old type parameters
        self.type_parameters = old_type_params;

        let signature = FunctionSignature {
            type_params: function.type_params.clone(), // Store generic type parameters
            params: param_types,
            default_values,
            return_type,
            is_mutable: function.is_mutable,
            is_public: function.is_public,
        };

        self.functions.insert(name.to_string(), signature);

        Ok(())
    }

    fn check_function(&mut self, function: &Function) -> Result<(), DiagnosticError> {
        // Add function type parameters to scope
        let old_type_params = self.type_parameters.clone();
        self.type_parameters.extend(function.type_params.iter().cloned());

        // Set up function scope
        self.push_scope();

        let signature = self.functions[&function.name].clone();
        self.current_function_return_type = Some(signature.return_type.clone());

        // Add parameters to scope
        for (param, (param_name, param_type)) in function.params.iter().zip(signature.params.iter()) {
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

        // Restore old type parameters
        self.type_parameters = old_type_params;
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
            Statement::Let { name, ty, value, span } => {
                // Validate variable name follows snake_case
                if !is_snake_case(name) {
                    return Err(DiagnosticError::Rich(
                        Diagnostic::syntax_error(
                            &self.filename,
                            *span,
                            format!("Variable name '{}' must be snake_case", name)
                        )
                        .with_label("variable names must use lowercase and underscores")
                        .with_help(format!("Try renaming to: {}", to_snake_case(name)))
                    ));
                }

                let value_type = self.check_expression(value)?;

                // Type annotation is mandatory - convert to HIR type
                let explicit_hir_type = self.ast_type_to_hir_type(ty)?;

                // Check if value type is compatible with explicit type (allows upcasting)
                if !self.is_assignable(&explicit_hir_type, &value_type) {
                    return Err(DiagnosticError::Rich(
                        Diagnostic::type_mismatch(
                            &self.filename,
                            *span,
                            &format!("{:?}", explicit_hir_type),
                            &format!("{:?}", value_type)
                        )
                        .with_label("type annotation doesn't match value type")
                        .with_help(format!("Change the type annotation to {:?} or convert the value", value_type))
                    ));
                }

                // Check for shadowing (not allowed with let)
                if self.scopes.last().unwrap().contains_key(name) {
                    return Err(DiagnosticError::Rich(
                        Diagnostic::syntax_error(
                            &self.filename,
                            *span,
                            format!("Variable '{}' is already defined in this scope", name)
                        )
                        .with_label("redefinition not allowed")
                        .with_help("Variables declared with 'let' cannot be redefined in the same scope")
                    ));
                }

                self.scopes.last_mut().unwrap().insert(name.clone(), explicit_hir_type);
            }
            Statement::Var { name, ty, value, span } => {
                // Validate variable name follows snake_case
                if !is_snake_case(name) {
                    return Err(DiagnosticError::Rich(
                        Diagnostic::syntax_error(
                            &self.filename,
                            *span,
                            format!("Variable name '{}' must be snake_case", name)
                        )
                        .with_label("variable names must use lowercase and underscores")
                        .with_help(format!("Try renaming to: {}", to_snake_case(name)))
                    ));
                }

                let value_type = self.check_expression(value)?;

                // Type annotation is mandatory - convert to HIR type
                let explicit_hir_type = self.ast_type_to_hir_type(ty)?;

                // Check if value type is compatible with explicit type (allows upcasting)
                if !self.is_assignable(&explicit_hir_type, &value_type) {
                    return Err(DiagnosticError::Rich(
                        Diagnostic::type_mismatch(
                            &self.filename,
                            *span,
                            &format!("{:?}", explicit_hir_type),
                            &format!("{:?}", value_type)
                        )
                        .with_label("type annotation doesn't match value type")
                        .with_help(format!("Change the type annotation to {:?} or convert the value", value_type))
                    ));
                }

                // Check for shadowing (not allowed with var)
                if self.scopes.last().unwrap().contains_key(name) {
                    return Err(DiagnosticError::Rich(
                        Diagnostic::syntax_error(
                            &self.filename,
                            *span,
                            format!("Variable '{}' is already defined in this scope", name)
                        )
                        .with_label("redefinition not allowed")
                        .with_help("Variables declared with 'var' cannot be redefined in the same scope")
                    ));
                }

                self.scopes.last_mut().unwrap().insert(name.clone(), explicit_hir_type);
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
            Statement::For { variable, variable_type, iterable, body, .. } => {
                // Validate loop variable name follows snake_case
                if !is_snake_case(variable) {
                    return Err(DiagnosticError::Type(
                        format!("Loop variable '{}' must be snake_case", variable)
                    ));
                }

                // Convert the explicit variable type annotation to HIR type
                let explicit_var_type = self.ast_type_to_hir_type(variable_type)?;

                // Check if the iterable is a Range expression
                let element_type = if let Expression::Range { .. } = iterable {
                    // Range expressions yield integers, get the type from the range
                    let range_type = self.check_expression(iterable)?;
                    range_type // The range returns its element type (I32 or I64)
                } else {
                    // Regular collection iteration
                    let iterable_type = self.check_expression(iterable)?;

                    // Extract element type from List
                    match iterable_type {
                        HirType::List(element_type) => *element_type,
                        _ => return Err(DiagnosticError::Type(
                            format!("For loop can only iterate over List or Range types, found {:?}", iterable_type)
                        )),
                    }
                };

                // Verify that the explicit variable type matches the iterable's element type
                if explicit_var_type != element_type {
                    return Err(DiagnosticError::Type(
                        format!("Loop variable type {:?} does not match iterable element type {:?}", explicit_var_type, element_type)
                    ));
                }

                // Create new scope for loop body and add loop variable
                self.push_scope();

                // Check if variable already exists in current scope
                if self.scopes.last().unwrap().contains_key(variable) {
                    return Err(DiagnosticError::Type(
                        format!("Loop variable '{}' is already defined in this scope", variable)
                    ));
                }

                self.scopes.last_mut().unwrap().insert(variable.clone(), explicit_var_type);
                self.check_block(body)?;
                self.pop_scope();
            }
            Statement::Print { value, .. } => {
                let value_type = self.check_expression(value)?;
                // Print accepts any type (will be converted to string)
                match value_type {
                    HirType::Bool | HirType::Int8 | HirType::Int16 | HirType::Int32 | HirType::Int64 | HirType::Float8 | HirType::Float16 | HirType::Float32 | HirType::Float64 | HirType::String => {},
                    _ => return Err(DiagnosticError::Type(
                        format!("Cannot print value of type {:?}", value_type)
                    )),
                }
            }
            Statement::Concurrent { body, .. } => {
                // Mark that we're entering a concurrent block
                let was_in_concurrent = self.in_concurrent_block;
                self.in_concurrent_block = true;

                // Type check the concurrent block body
                self.push_scope();
                self.check_block(body)?;
                self.pop_scope();

                // Restore the previous concurrent block state
                self.in_concurrent_block = was_in_concurrent;
            }
        }
        Ok(())
    }

    fn check_expression(&mut self, expression: &Expression) -> Result<HirType, DiagnosticError> {
        match expression {
            Expression::Literal(literal) => self.check_literal(literal),
            Expression::Identifier { name, span } => {
                self.lookup_variable(name).map_err(|_| {
                    DiagnosticError::Rich(
                        Diagnostic::undefined_symbol(
                            &self.filename,
                            *span,
                            name
                        )
                        .with_help("Check that the variable is declared and in scope")
                    )
                })
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
                // Handle built-in assert function
                if function == "assert" {
                    // assert(condition = expr) or assert(condition = expr, message = "...")
                    if args.is_empty() {
                        return Err(DiagnosticError::Type(
                            "assert requires at least one argument: 'condition'".to_string()
                        ));
                    }

                    // Check required 'condition' parameter
                    let condition_arg = args.iter()
                        .find(|arg| arg.name == "condition")
                        .ok_or_else(|| DiagnosticError::Type(
                            "assert requires a 'condition' parameter".to_string()
                        ))?;

                    let condition_type = self.check_expression(&condition_arg.value)?;
                    if condition_type != HirType::Bool {
                        return Err(DiagnosticError::Type(
                            format!("assert 'condition' parameter must be Bool, got {:?}", condition_type)
                        ));
                    }

                    // Check optional 'message' parameter
                    if let Some(message_arg) = args.iter().find(|arg| arg.name == "message") {
                        let message_type = self.check_expression(&message_arg.value)?;
                        if message_type != HirType::String {
                            return Err(DiagnosticError::Type(
                                format!("assert 'message' parameter must be String, got {:?}", message_type)
                            ));
                        }
                    }

                    // Validate we only have 'condition' and optionally 'message'
                    for arg in args {
                        if arg.name != "condition" && arg.name != "message" {
                            return Err(DiagnosticError::Type(
                                format!("assert does not have a parameter named '{}'", arg.name)
                            ));
                        }
                    }

                    if args.len() > 2 {
                        return Err(DiagnosticError::Type(
                            "assert accepts at most 2 parameters: 'condition' and 'message'".to_string()
                        ));
                    }

                    return Ok(HirType::Unit);
                }

                // Handle built-in __test_reset function (test framework internal)
                if function == "__test_reset" {
                    if !args.is_empty() {
                        return Err(DiagnosticError::Type(
                            "__test_reset does not accept any arguments".to_string()
                        ));
                    }
                    return Ok(HirType::Unit);
                }

                // Handle built-in __test_check function (test framework internal)
                if function == "__test_check" {
                    if !args.is_empty() {
                        return Err(DiagnosticError::Type(
                            "__test_check does not accept any arguments".to_string()
                        ));
                    }
                    return Ok(HirType::Bool);
                }

                // Handle built-in tcp_listen function
                if function == "tcp_listen" {
                    // tcp_listen(host: String, port: Int32) -> Result<Int32, String>
                    if args.len() != 2 {
                        return Err(DiagnosticError::Type(
                            "tcp_listen requires exactly 2 arguments: 'host' and 'port'".to_string()
                        ));
                    }

                    let host_arg = args.iter().find(|arg| arg.name == "host")
                        .ok_or_else(|| DiagnosticError::Type("tcp_listen requires a 'host' parameter".to_string()))?;
                    let port_arg = args.iter().find(|arg| arg.name == "port")
                        .ok_or_else(|| DiagnosticError::Type("tcp_listen requires a 'port' parameter".to_string()))?;

                    let host_type = self.check_expression(&host_arg.value)?;
                    let port_type = self.check_expression(&port_arg.value)?;

                    if host_type != HirType::String {
                        return Err(DiagnosticError::Type(
                            format!("tcp_listen 'host' parameter must be String, got {:?}", host_type)
                        ));
                    }
                    if port_type != HirType::Int32 {
                        return Err(DiagnosticError::Type(
                            format!("tcp_listen 'port' parameter must be Int32, got {:?}", port_type)
                        ));
                    }

                    return Ok(HirType::Enum("Result".to_string(), vec![HirType::Int32, HirType::String]));
                }

                // Handle built-in tcp_accept function
                if function == "tcp_accept" {
                    // tcp_accept(listener: Int32) -> Result<Int32, String>
                    if args.len() != 1 {
                        return Err(DiagnosticError::Type(
                            "tcp_accept requires exactly 1 argument: 'listener'".to_string()
                        ));
                    }

                    let listener_arg = args.iter().find(|arg| arg.name == "listener")
                        .ok_or_else(|| DiagnosticError::Type("tcp_accept requires a 'listener' parameter".to_string()))?;

                    let listener_type = self.check_expression(&listener_arg.value)?;
                    if listener_type != HirType::Int32 {
                        return Err(DiagnosticError::Type(
                            format!("tcp_accept 'listener' parameter must be Int32, got {:?}", listener_type)
                        ));
                    }

                    return Ok(HirType::Enum("Result".to_string(), vec![HirType::Int32, HirType::String]));
                }

                // Handle built-in tcp_connect function
                if function == "tcp_connect" {
                    // tcp_connect(host: String, port: Int32) -> Result<Int32, String>
                    if args.len() != 2 {
                        return Err(DiagnosticError::Type(
                            "tcp_connect requires exactly 2 arguments: 'host' and 'port'".to_string()
                        ));
                    }

                    let host_arg = args.iter().find(|arg| arg.name == "host")
                        .ok_or_else(|| DiagnosticError::Type("tcp_connect requires a 'host' parameter".to_string()))?;
                    let port_arg = args.iter().find(|arg| arg.name == "port")
                        .ok_or_else(|| DiagnosticError::Type("tcp_connect requires a 'port' parameter".to_string()))?;

                    let host_type = self.check_expression(&host_arg.value)?;
                    let port_type = self.check_expression(&port_arg.value)?;

                    if host_type != HirType::String {
                        return Err(DiagnosticError::Type(
                            format!("tcp_connect 'host' parameter must be String, got {:?}", host_type)
                        ));
                    }
                    if port_type != HirType::Int32 {
                        return Err(DiagnosticError::Type(
                            format!("tcp_connect 'port' parameter must be Int32, got {:?}", port_type)
                        ));
                    }

                    return Ok(HirType::Enum("Result".to_string(), vec![HirType::Int32, HirType::String]));
                }

                // Handle built-in tcp_read function
                if function == "tcp_read" {
                    // tcp_read(socket: Int32, max_bytes: Int32) -> Result<String, String>
                    if args.len() != 2 {
                        return Err(DiagnosticError::Type(
                            "tcp_read requires exactly 2 arguments: 'socket' and 'max_bytes'".to_string()
                        ));
                    }

                    let socket_arg = args.iter().find(|arg| arg.name == "socket")
                        .ok_or_else(|| DiagnosticError::Type("tcp_read requires a 'socket' parameter".to_string()))?;
                    let max_bytes_arg = args.iter().find(|arg| arg.name == "max_bytes")
                        .ok_or_else(|| DiagnosticError::Type("tcp_read requires a 'max_bytes' parameter".to_string()))?;

                    let socket_type = self.check_expression(&socket_arg.value)?;
                    let max_bytes_type = self.check_expression(&max_bytes_arg.value)?;

                    if socket_type != HirType::Int32 {
                        return Err(DiagnosticError::Type(
                            format!("tcp_read 'socket' parameter must be Int32, got {:?}", socket_type)
                        ));
                    }
                    if max_bytes_type != HirType::Int32 {
                        return Err(DiagnosticError::Type(
                            format!("tcp_read 'max_bytes' parameter must be Int32, got {:?}", max_bytes_type)
                        ));
                    }

                    return Ok(HirType::Enum("Result".to_string(), vec![HirType::String, HirType::String]));
                }

                // Handle built-in tcp_write function
                if function == "tcp_write" {
                    // tcp_write(socket: Int32, data: String) -> Result<Int32, String>
                    if args.len() != 2 {
                        return Err(DiagnosticError::Type(
                            "tcp_write requires exactly 2 arguments: 'socket' and 'data'".to_string()
                        ));
                    }

                    let socket_arg = args.iter().find(|arg| arg.name == "socket")
                        .ok_or_else(|| DiagnosticError::Type("tcp_write requires a 'socket' parameter".to_string()))?;
                    let data_arg = args.iter().find(|arg| arg.name == "data")
                        .ok_or_else(|| DiagnosticError::Type("tcp_write requires a 'data' parameter".to_string()))?;

                    let socket_type = self.check_expression(&socket_arg.value)?;
                    let data_type = self.check_expression(&data_arg.value)?;

                    if socket_type != HirType::Int32 {
                        return Err(DiagnosticError::Type(
                            format!("tcp_write 'socket' parameter must be Int32, got {:?}", socket_type)
                        ));
                    }
                    if data_type != HirType::String {
                        return Err(DiagnosticError::Type(
                            format!("tcp_write 'data' parameter must be String, got {:?}", data_type)
                        ));
                    }

                    return Ok(HirType::Enum("Result".to_string(), vec![HirType::Int32, HirType::String]));
                }

                // Handle built-in tcp_close function
                if function == "tcp_close" {
                    // tcp_close(socket: Int32) -> Result<Bool, String>
                    if args.len() != 1 {
                        return Err(DiagnosticError::Type(
                            "tcp_close requires exactly 1 argument: 'socket'".to_string()
                        ));
                    }

                    let socket_arg = args.iter().find(|arg| arg.name == "socket")
                        .ok_or_else(|| DiagnosticError::Type("tcp_close requires a 'socket' parameter".to_string()))?;

                    let socket_type = self.check_expression(&socket_arg.value)?;
                    if socket_type != HirType::Int32 {
                        return Err(DiagnosticError::Type(
                            format!("tcp_close 'socket' parameter must be Int32, got {:?}", socket_type)
                        ));
                    }

                    return Ok(HirType::Enum("Result".to_string(), vec![HirType::Bool, HirType::String]));
                }

                // Handle built-in file_open function
                if function == "file_open" {
                    // file_open(path: String, mode: String) -> Result<Int32, String>
                    if args.len() != 2 {
                        return Err(DiagnosticError::Type(
                            "file_open requires exactly 2 arguments: 'path' and 'mode'".to_string()
                        ));
                    }

                    let path_arg = args.iter().find(|arg| arg.name == "path")
                        .ok_or_else(|| DiagnosticError::Type("file_open requires a 'path' parameter".to_string()))?;
                    let mode_arg = args.iter().find(|arg| arg.name == "mode")
                        .ok_or_else(|| DiagnosticError::Type("file_open requires a 'mode' parameter".to_string()))?;

                    let path_type = self.check_expression(&path_arg.value)?;
                    let mode_type = self.check_expression(&mode_arg.value)?;

                    if path_type != HirType::String {
                        return Err(DiagnosticError::Type(
                            format!("file_open 'path' parameter must be String, got {:?}", path_type)
                        ));
                    }
                    if mode_type != HirType::String {
                        return Err(DiagnosticError::Type(
                            format!("file_open 'mode' parameter must be String, got {:?}", mode_type)
                        ));
                    }

                    return Ok(HirType::Enum("Result".to_string(), vec![HirType::Int32, HirType::String]));
                }

                // Handle built-in file_read function
                if function == "file_read" {
                    // file_read(fd: Int32, max_bytes: Int32) -> Result<String, String>
                    if args.len() != 2 {
                        return Err(DiagnosticError::Type(
                            "file_read requires exactly 2 arguments: 'fd' and 'max_bytes'".to_string()
                        ));
                    }

                    let fd_arg = args.iter().find(|arg| arg.name == "fd")
                        .ok_or_else(|| DiagnosticError::Type("file_read requires a 'fd' parameter".to_string()))?;
                    let max_bytes_arg = args.iter().find(|arg| arg.name == "max_bytes")
                        .ok_or_else(|| DiagnosticError::Type("file_read requires a 'max_bytes' parameter".to_string()))?;

                    let fd_type = self.check_expression(&fd_arg.value)?;
                    let max_bytes_type = self.check_expression(&max_bytes_arg.value)?;

                    if fd_type != HirType::Int32 {
                        return Err(DiagnosticError::Type(
                            format!("file_read 'fd' parameter must be Int32, got {:?}", fd_type)
                        ));
                    }
                    if max_bytes_type != HirType::Int32 {
                        return Err(DiagnosticError::Type(
                            format!("file_read 'max_bytes' parameter must be Int32, got {:?}", max_bytes_type)
                        ));
                    }

                    return Ok(HirType::Enum("Result".to_string(), vec![HirType::String, HirType::String]));
                }

                // Handle built-in file_write function
                if function == "file_write" {
                    // file_write(fd: Int32, data: String) -> Result<Int32, String>
                    if args.len() != 2 {
                        return Err(DiagnosticError::Type(
                            "file_write requires exactly 2 arguments: 'fd' and 'data'".to_string()
                        ));
                    }

                    let fd_arg = args.iter().find(|arg| arg.name == "fd")
                        .ok_or_else(|| DiagnosticError::Type("file_write requires a 'fd' parameter".to_string()))?;
                    let data_arg = args.iter().find(|arg| arg.name == "data")
                        .ok_or_else(|| DiagnosticError::Type("file_write requires a 'data' parameter".to_string()))?;

                    let fd_type = self.check_expression(&fd_arg.value)?;
                    let data_type = self.check_expression(&data_arg.value)?;

                    if fd_type != HirType::Int32 {
                        return Err(DiagnosticError::Type(
                            format!("file_write 'fd' parameter must be Int32, got {:?}", fd_type)
                        ));
                    }
                    if data_type != HirType::String {
                        return Err(DiagnosticError::Type(
                            format!("file_write 'data' parameter must be String, got {:?}", data_type)
                        ));
                    }

                    return Ok(HirType::Enum("Result".to_string(), vec![HirType::Int32, HirType::String]));
                }

                // Handle built-in file_close function
                if function == "file_close" {
                    // file_close(fd: Int32) -> Result<Bool, String>
                    if args.len() != 1 {
                        return Err(DiagnosticError::Type(
                            "file_close requires exactly 1 argument: 'fd'".to_string()
                        ));
                    }

                    let fd_arg = args.iter().find(|arg| arg.name == "fd")
                        .ok_or_else(|| DiagnosticError::Type("file_close requires a 'fd' parameter".to_string()))?;

                    let fd_type = self.check_expression(&fd_arg.value)?;

                    if fd_type != HirType::Int32 {
                        return Err(DiagnosticError::Type(
                            format!("file_close 'fd' parameter must be Int32, got {:?}", fd_type)
                        ));
                    }

                    return Ok(HirType::Enum("Result".to_string(), vec![HirType::Bool, HirType::String]));
                }

                // Handle built-in file_exists function
                if function == "file_exists" {
                    // file_exists(path: String) -> Bool
                    if args.len() != 1 {
                        return Err(DiagnosticError::Type(
                            "file_exists requires exactly 1 argument: 'path'".to_string()
                        ));
                    }

                    let path_arg = args.iter().find(|arg| arg.name == "path")
                        .ok_or_else(|| DiagnosticError::Type("file_exists requires a 'path' parameter".to_string()))?;

                    let path_type = self.check_expression(&path_arg.value)?;

                    if path_type != HirType::String {
                        return Err(DiagnosticError::Type(
                            format!("file_exists 'path' parameter must be String, got {:?}", path_type)
                        ));
                    }

                    return Ok(HirType::Bool);
                }

                // Handle built-in file_size function
                if function == "file_size" {
                    // file_size(path: String) -> Result<Int64, String>
                    if args.len() != 1 {
                        return Err(DiagnosticError::Type(
                            "file_size requires exactly 1 argument: 'path'".to_string()
                        ));
                    }

                    let path_arg = args.iter().find(|arg| arg.name == "path")
                        .ok_or_else(|| DiagnosticError::Type("file_size requires a 'path' parameter".to_string()))?;

                    let path_type = self.check_expression(&path_arg.value)?;

                    if path_type != HirType::String {
                        return Err(DiagnosticError::Type(
                            format!("file_size 'path' parameter must be String, got {:?}", path_type)
                        ));
                    }

                    return Ok(HirType::Enum("Result".to_string(), vec![HirType::Int64, HirType::String]));
                }

                // Handle built-in file_is_dir function
                if function == "file_is_dir" {
                    // file_is_dir(path: String) -> Bool
                    if args.len() != 1 {
                        return Err(DiagnosticError::Type(
                            "file_is_dir requires exactly 1 argument: 'path'".to_string()
                        ));
                    }

                    let path_arg = args.iter().find(|arg| arg.name == "path")
                        .ok_or_else(|| DiagnosticError::Type("file_is_dir requires a 'path' parameter".to_string()))?;

                    let path_type = self.check_expression(&path_arg.value)?;

                    if path_type != HirType::String {
                        return Err(DiagnosticError::Type(
                            format!("file_is_dir 'path' parameter must be String, got {:?}", path_type)
                        ));
                    }

                    return Ok(HirType::Bool);
                }

                // Handle built-in file_delete function
                if function == "file_delete" {
                    // file_delete(path: String) -> Result<Bool, String>
                    if args.len() != 1 {
                        return Err(DiagnosticError::Type(
                            "file_delete requires exactly 1 argument: 'path'".to_string()
                        ));
                    }

                    let path_arg = args.iter().find(|arg| arg.name == "path")
                        .ok_or_else(|| DiagnosticError::Type("file_delete requires a 'path' parameter".to_string()))?;

                    let path_type = self.check_expression(&path_arg.value)?;

                    if path_type != HirType::String {
                        return Err(DiagnosticError::Type(
                            format!("file_delete 'path' parameter must be String, got {:?}", path_type)
                        ));
                    }

                    return Ok(HirType::Enum("Result".to_string(), vec![HirType::Bool, HirType::String]));
                }

                // Handle built-in file_rename function
                if function == "file_rename" {
                    // file_rename(old_path: String, new_path: String) -> Result<Bool, String>
                    if args.len() != 2 {
                        return Err(DiagnosticError::Type(
                            "file_rename requires exactly 2 arguments: 'old_path' and 'new_path'".to_string()
                        ));
                    }

                    let old_path_arg = args.iter().find(|arg| arg.name == "old_path")
                        .ok_or_else(|| DiagnosticError::Type("file_rename requires an 'old_path' parameter".to_string()))?;

                    let new_path_arg = args.iter().find(|arg| arg.name == "new_path")
                        .ok_or_else(|| DiagnosticError::Type("file_rename requires a 'new_path' parameter".to_string()))?;

                    let old_path_type = self.check_expression(&old_path_arg.value)?;
                    let new_path_type = self.check_expression(&new_path_arg.value)?;

                    if old_path_type != HirType::String {
                        return Err(DiagnosticError::Type(
                            format!("file_rename 'old_path' parameter must be String, got {:?}", old_path_type)
                        ));
                    }

                    if new_path_type != HirType::String {
                        return Err(DiagnosticError::Type(
                            format!("file_rename 'new_path' parameter must be String, got {:?}", new_path_type)
                        ));
                    }

                    return Ok(HirType::Enum("Result".to_string(), vec![HirType::Bool, HirType::String]));
                }

                // Handle built-in dir_create function
                if function == "dir_create" {
                    // dir_create(path: String) -> Result<Bool, String>
                    if args.len() != 1 {
                        return Err(DiagnosticError::Type(
                            "dir_create requires exactly 1 argument: 'path'".to_string()
                        ));
                    }

                    let path_arg = args.iter().find(|arg| arg.name == "path")
                        .ok_or_else(|| DiagnosticError::Type("dir_create requires a 'path' parameter".to_string()))?;

                    let path_type = self.check_expression(&path_arg.value)?;
                    if path_type != HirType::String {
                        return Err(DiagnosticError::Type(
                            format!("dir_create 'path' parameter must be String, got {:?}", path_type)
                        ));
                    }

                    return Ok(HirType::Enum("Result".to_string(), vec![HirType::Bool, HirType::String]));
                }

                // Handle built-in dir_create_all function
                if function == "dir_create_all" {
                    // dir_create_all(path: String) -> Result<Bool, String>
                    if args.len() != 1 {
                        return Err(DiagnosticError::Type(
                            "dir_create_all requires exactly 1 argument: 'path'".to_string()
                        ));
                    }

                    let path_arg = args.iter().find(|arg| arg.name == "path")
                        .ok_or_else(|| DiagnosticError::Type("dir_create_all requires a 'path' parameter".to_string()))?;

                    let path_type = self.check_expression(&path_arg.value)?;
                    if path_type != HirType::String {
                        return Err(DiagnosticError::Type(
                            format!("dir_create_all 'path' parameter must be String, got {:?}", path_type)
                        ));
                    }

                    return Ok(HirType::Enum("Result".to_string(), vec![HirType::Bool, HirType::String]));
                }

                // Handle built-in dir_remove function
                if function == "dir_remove" {
                    // dir_remove(path: String) -> Result<Bool, String>
                    if args.len() != 1 {
                        return Err(DiagnosticError::Type(
                            "dir_remove requires exactly 1 argument: 'path'".to_string()
                        ));
                    }

                    let path_arg = args.iter().find(|arg| arg.name == "path")
                        .ok_or_else(|| DiagnosticError::Type("dir_remove requires a 'path' parameter".to_string()))?;

                    let path_type = self.check_expression(&path_arg.value)?;
                    if path_type != HirType::String {
                        return Err(DiagnosticError::Type(
                            format!("dir_remove 'path' parameter must be String, got {:?}", path_type)
                        ));
                    }

                    return Ok(HirType::Enum("Result".to_string(), vec![HirType::Bool, HirType::String]));
                }

                // Handle built-in dir_list function
                if function == "dir_list" {
                    // dir_list(path: String) -> Result<String, String>
                    if args.len() != 1 {
                        return Err(DiagnosticError::Type(
                            "dir_list requires exactly 1 argument: 'path'".to_string()
                        ));
                    }

                    let path_arg = args.iter().find(|arg| arg.name == "path")
                        .ok_or_else(|| DiagnosticError::Type("dir_list requires a 'path' parameter".to_string()))?;

                    let path_type = self.check_expression(&path_arg.value)?;
                    if path_type != HirType::String {
                        return Err(DiagnosticError::Type(
                            format!("dir_list 'path' parameter must be String, got {:?}", path_type)
                        ));
                    }

                    return Ok(HirType::Enum("Result".to_string(), vec![HirType::String, HirType::String]));
                }

                // Handle built-in file_read_binary function
                if function == "file_read_binary" {
                    // file_read_binary(fd: Int32, max_bytes: Int32) -> Result<List[Int8], String>
                    if args.len() != 2 {
                        return Err(DiagnosticError::Type(
                            "file_read_binary requires exactly 2 arguments: 'fd' and 'max_bytes'".to_string()
                        ));
                    }

                    let fd_arg = args.iter().find(|arg| arg.name == "fd")
                        .ok_or_else(|| DiagnosticError::Type("file_read_binary requires a 'fd' parameter".to_string()))?;
                    let max_bytes_arg = args.iter().find(|arg| arg.name == "max_bytes")
                        .ok_or_else(|| DiagnosticError::Type("file_read_binary requires a 'max_bytes' parameter".to_string()))?;

                    let fd_type = self.check_expression(&fd_arg.value)?;
                    let max_bytes_type = self.check_expression(&max_bytes_arg.value)?;

                    if fd_type != HirType::Int32 {
                        return Err(DiagnosticError::Type(
                            format!("file_read_binary 'fd' parameter must be Int32, got {:?}", fd_type)
                        ));
                    }
                    if max_bytes_type != HirType::Int32 {
                        return Err(DiagnosticError::Type(
                            format!("file_read_binary 'max_bytes' parameter must be Int32, got {:?}", max_bytes_type)
                        ));
                    }

                    return Ok(HirType::Enum("Result".to_string(), vec![HirType::List(Box::new(HirType::Int8)), HirType::String]));
                }

                // Handle built-in file_write_binary function
                if function == "file_write_binary" {
                    // file_write_binary(fd: Int32, data: List[Int8]) -> Result<Int32, String>
                    if args.len() != 2 {
                        return Err(DiagnosticError::Type(
                            "file_write_binary requires exactly 2 arguments: 'fd' and 'data'".to_string()
                        ));
                    }

                    let fd_arg = args.iter().find(|arg| arg.name == "fd")
                        .ok_or_else(|| DiagnosticError::Type("file_write_binary requires a 'fd' parameter".to_string()))?;
                    let data_arg = args.iter().find(|arg| arg.name == "data")
                        .ok_or_else(|| DiagnosticError::Type("file_write_binary requires a 'data' parameter".to_string()))?;

                    let fd_type = self.check_expression(&fd_arg.value)?;
                    let data_type = self.check_expression(&data_arg.value)?;

                    if fd_type != HirType::Int32 {
                        return Err(DiagnosticError::Type(
                            format!("file_write_binary 'fd' parameter must be Int32, got {:?}", fd_type)
                        ));
                    }
                    let expected_data_type = HirType::List(Box::new(HirType::Int8));
                    if data_type != expected_data_type {
                        return Err(DiagnosticError::Type(
                            format!("file_write_binary 'data' parameter must be List[Int8], got {:?}", data_type)
                        ));
                    }

                    return Ok(HirType::Enum("Result".to_string(), vec![HirType::Int32, HirType::String]));
                }

                // Handle built-in channel_init function
                if function == "channel_init" {
                    // channel_init<T>(capacity: Int32) -> Channel<T>
                    if args.len() != 1 {
                        return Err(DiagnosticError::Type(
                            "channel_init requires exactly 1 argument: 'capacity'".to_string()
                        ));
                    }

                    let capacity_arg = args.iter().find(|arg| arg.name == "capacity")
                        .ok_or_else(|| DiagnosticError::Type("channel_init requires a 'capacity' parameter".to_string()))?;

                    let capacity_type = self.check_expression(&capacity_arg.value)?;
                    if capacity_type != HirType::Int32 {
                        return Err(DiagnosticError::Type(
                            format!("channel_init 'capacity' parameter must be Int32, got {:?}", capacity_type)
                        ));
                    }

                    // TODO: Infer element type from context (for now default to Int32)
                    // Return Channel<Int32>
                    return Ok(HirType::Channel(Box::new(HirType::Int32)));
                }

                // Try to resolve the function name (handles both local and qualified names)
                let resolved_name = self.module_table.resolve(function)
                    .unwrap_or_else(|| function.clone());

                // Look up in local functions first, then try global symbols
                let signature = if let Some(sig) = self.functions.get(&resolved_name) {
                    sig.clone()
                } else if let Some(sig) = self.functions.get(function) {
                    sig.clone()
                } else if let Some(Symbol::Function(sig)) = self.module_table.global_symbols.get(&resolved_name) {
                    // Check visibility for cross-module function access
                    let function_module = self.get_module_from_qualified_name(&resolved_name);
                    if !self.can_access_symbol(&function_module, sig.is_public) {
                        return Err(DiagnosticError::Type(
                            format!("Function '{}' is private and cannot be called from module '{}'",
                                   function, self.module_table.current_module)
                        ));
                    }
                    sig.clone()
                } else {
                    // If not found as a function, check if it's a class constructor with zero arguments
                    if let Some(class_info) = self.classes.get(function) {
                        // This is a class constructor with no arguments (empty class)
                        // Get the init signature we generated
                        if let Some(init_sig) = class_info.methods.get("init") {
                            if !args.is_empty() {
                                return Err(DiagnosticError::Type(
                                    format!("Constructor for '{}' expects {} arguments, got {}",
                                           function, init_sig.params.len(), args.len())
                                ));
                            }
                            // Return the class type
                            return Ok(init_sig.return_type.clone());
                        }
                    }
                    return Err(DiagnosticError::Type(format!("Unknown function '{}'", function)));
                };

                // Count required parameters (those without defaults)
                let required_params = signature.default_values.iter().take_while(|d| d.is_none()).count();

                // Check argument count is valid
                if args.len() < required_params {
                    return Err(DiagnosticError::Type(
                        format!("Function '{}' expects at least {} arguments, got {}", function, required_params, args.len())
                    ));
                }
                if args.len() > signature.params.len() {
                    return Err(DiagnosticError::Type(
                        format!("Function '{}' expects at most {} arguments, got {}", function, signature.params.len(), args.len())
                    ));
                }

                // Validate named arguments match parameter names and types
                for arg in args {
                    let param = signature.params.iter()
                        .find(|(param_name, _)| param_name == &arg.name)
                        .ok_or_else(|| DiagnosticError::Type(
                            format!("Function '{}' has no parameter named '{}'", function, arg.name)
                        ))?;

                    let arg_type = self.check_expression(&arg.value)?;
                    if arg_type != param.1 {
                        return Err(DiagnosticError::Type(
                            format!("Function '{}' parameter '{}' expects type {:?}, got {:?}", function, arg.name, param.1, arg_type)
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

                        // Check if assignment is type-compatible (allows upcasting)
                        if !self.is_assignable(&variable_type, &value_type) {
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
                                    // Check visibility
                                    if !self.can_access_field(class_name, field_info.is_public) {
                                        return Err(DiagnosticError::Type(
                                            format!("Field '{}' is private and cannot be accessed from outside class '{}'",
                                                   member, class_name)
                                        ));
                                    }

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
                                    // Check if assignment is type-compatible (allows upcasting)
                                    if !self.is_assignable(&field_info.ty, &value_type) {
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
                if index_type != HirType::Int32 {
                    return Err(DiagnosticError::Type(
                        format!("Array index must be i32, got {:?}", index_type)
                    ));
                }

                // Object must be List - returns Option<element_type>
                match object_type {
                    HirType::List(element_type) => {
                        // Return Option<T> for safe indexing
                        Ok(HirType::Enum("Option".to_string(), vec![*element_type]))
                    }
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
                        Ok(HirType::Int32)
                    }
                    (HirType::List(_), "length") => {
                        if !args.is_empty() {
                            return Err(DiagnosticError::Type(
                                "length() method takes no arguments".to_string()
                            ));
                        }
                        Ok(HirType::Int32)
                    }
                    (HirType::List(element_type), "get") => {
                        if args.len() != 1 {
                            return Err(DiagnosticError::Type(
                                "get() method takes exactly one argument".to_string()
                            ));
                        }
                        let index_type = self.check_expression(&args[0].value)?;
                        if index_type != HirType::Int32 {
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
                        let index_type = self.check_expression(&args[0].value)?;
                        let value_type = self.check_expression(&args[1].value)?;
                        if index_type != HirType::Int32 {
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
                        let value_type = self.check_expression(&args[0].value)?;
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
                        let index_type = self.check_expression(&args[0].value)?;
                        let value_type = self.check_expression(&args[1].value)?;
                        if index_type != HirType::Int32 {
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
                        let index_type = self.check_expression(&args[0].value)?;
                        if index_type != HirType::Int32 {
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
                        let value_type = self.check_expression(&args[0].value)?;
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
                        let value_type = self.check_expression(&args[0].value)?;
                        if value_type != **element_type {
                            return Err(DiagnosticError::Type(
                                format!("index_of() method expects value of type {:?}, got {:?}", element_type, value_type)
                            ));
                        }
                        // Returns Option<i32>
                        Ok(HirType::Enum("Option".to_string(), vec![HirType::Int32]))
                    }
                    (HirType::List(element_type), "count") => {
                        if args.len() != 1 {
                            return Err(DiagnosticError::Type(
                                "count() method takes exactly one argument".to_string()
                            ));
                        }
                        let value_type = self.check_expression(&args[0].value)?;
                        if value_type != **element_type {
                            return Err(DiagnosticError::Type(
                                format!("count() method expects value of type {:?}, got {:?}", element_type, value_type)
                            ));
                        }
                        Ok(HirType::Int32)
                    }
                    (HirType::List(element_type), "slice") => {
                        if args.len() != 2 {
                            return Err(DiagnosticError::Type(
                                "slice() method takes exactly two arguments".to_string()
                            ));
                        }
                        let start_type = self.check_expression(&args[0].value)?;
                        let end_type = self.check_expression(&args[1].value)?;
                        if start_type != HirType::Int32 {
                            return Err(DiagnosticError::Type(
                                format!("slice() method expects i32 start index, got {:?}", start_type)
                            ));
                        }
                        if end_type != HirType::Int32 {
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
                        let other_type = self.check_expression(&args[0].value)?;
                        match other_type {
                            HirType::List(other_element_type) if *other_element_type == **element_type => {
                                Ok(HirType::List(element_type.clone()))
                            }
                            _ => Err(DiagnosticError::Type(
                                format!("concat() method expects List<{:?}>, got {:?}", element_type, other_type)
                            ))
                        }
                    }
                    (HirType::List(_element_type), "all") => {
                        if args.len() != 1 {
                            return Err(DiagnosticError::Type(
                                "all() method takes exactly one argument".to_string()
                            ));
                        }
                        // For now, accept any function - in a more advanced type system,
                        // we'd check that it's a function T -> bool
                        let _predicate_type = self.check_expression(&args[0].value)?;
                        Ok(HirType::Bool)
                    }
                    (HirType::List(_element_type), "any") => {
                        if args.len() != 1 {
                            return Err(DiagnosticError::Type(
                                "any() method takes exactly one argument".to_string()
                            ));
                        }
                        // For now, accept any function - in a more advanced type system,
                        // we'd check that it's a function T -> bool
                        let _predicate_type = self.check_expression(&args[0].value)?;
                        Ok(HirType::Bool)
                    }
                    // String methods
                    (HirType::String, "length") => {
                        if !args.is_empty() {
                            return Err(DiagnosticError::Type(
                                "length() method takes no arguments".to_string()
                            ));
                        }
                        Ok(HirType::Int32)
                    }
                    (HirType::String, "concat") => {
                        if args.len() != 1 {
                            return Err(DiagnosticError::Type(
                                "concat() method takes exactly one argument".to_string()
                            ));
                        }
                        let arg_type = self.check_expression(&args[0].value)?;
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
                        let arg_type = self.check_expression(&args[0].value)?;
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
                        let arg_type = self.check_expression(&args[0].value)?;
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
                        let arg_type = self.check_expression(&args[0].value)?;
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
                        let from_type = self.check_expression(&args[0].value)?;
                        let to_type = self.check_expression(&args[1].value)?;
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
                        let from_type = self.check_expression(&args[0].value)?;
                        let to_type = self.check_expression(&args[1].value)?;
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
                        let arg_type = self.check_expression(&args[0].value)?;
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
                    (HirType::String, "parse_int") => {
                        if !args.is_empty() {
                            return Err(DiagnosticError::Type(
                                "parse_int() method takes no arguments".to_string()
                            ));
                        }
                        // Returns Result<Int32, String>
                        Ok(HirType::Enum("Result".to_string(), vec![HirType::Int32, HirType::String]))
                    }
                    (HirType::String, "parse_int64") => {
                        if !args.is_empty() {
                            return Err(DiagnosticError::Type(
                                "parse_int64() method takes no arguments".to_string()
                            ));
                        }
                        // Returns Result<Int64, String>
                        Ok(HirType::Enum("Result".to_string(), vec![HirType::Int64, HirType::String]))
                    }
                    (HirType::String, "parse_float") => {
                        if !args.is_empty() {
                            return Err(DiagnosticError::Type(
                                "parse_float() method takes no arguments".to_string()
                            ));
                        }
                        // Returns Result<Float64, String>
                        Ok(HirType::Enum("Result".to_string(), vec![HirType::Float64, HirType::String]))
                    }
                    (HirType::String, "parse_bool") => {
                        if !args.is_empty() {
                            return Err(DiagnosticError::Type(
                                "parse_bool() method takes no arguments".to_string()
                            ));
                        }
                        // Returns Result<Bool, String>
                        Ok(HirType::Enum("Result".to_string(), vec![HirType::Bool, HirType::String]))
                    }
                    // Dict methods
                    (HirType::Dict(key_type, value_type), "get") => {
                        if args.len() != 1 {
                            return Err(DiagnosticError::Type(
                                "get() method takes exactly one argument".to_string()
                            ));
                        }
                        let arg_type = self.check_expression(&args[0].value)?;
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
                        let key_arg_type = self.check_expression(&args[0].value)?;
                        if key_arg_type != **key_type {
                            return Err(DiagnosticError::Type(
                                format!("set() method expects key of type {:?}, got {:?}", key_type, key_arg_type)
                            ));
                        }
                        let value_arg_type = self.check_expression(&args[1].value)?;
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
                        let arg_type = self.check_expression(&args[0].value)?;
                        if arg_type != **key_type {
                            return Err(DiagnosticError::Type(
                                format!("remove() method expects key of type {:?}, got {:?}", key_type, arg_type)
                            ));
                        }
                        Ok(HirType::Int64)  // Returns removed value or 0
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
                        Ok(HirType::Int32)
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
                        let arg_type = self.check_expression(&args[0].value)?;
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
                        let arg_type = self.check_expression(&args[0].value)?;
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
                        let arg_type = self.check_expression(&args[0].value)?;
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
                        let key_arg_type = self.check_expression(&args[0].value)?;
                        if key_arg_type != **key_type {
                            return Err(DiagnosticError::Type(
                                format!("get_or() method expects key of type {:?}, got {:?}", key_type, key_arg_type)
                            ));
                        }
                        let default_arg_type = self.check_expression(&args[1].value)?;
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
                        let value_type = self.check_expression(&args[0].value)?;
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
                        let value_type = self.check_expression(&args[0].value)?;
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
                        Ok(HirType::Int32)
                    }
                    (HirType::Set(element_type), "contains") => {
                        if args.len() != 1 {
                            return Err(DiagnosticError::Type(
                                "contains() method takes exactly one argument".to_string()
                            ));
                        }
                        let value_type = self.check_expression(&args[0].value)?;
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
                        let other_type = self.check_expression(&args[0].value)?;
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
                        let other_type = self.check_expression(&args[0].value)?;
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
                        let other_type = self.check_expression(&args[0].value)?;
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
                        let other_type = self.check_expression(&args[0].value)?;
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
                        let other_type = self.check_expression(&args[0].value)?;
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
                        let other_type = self.check_expression(&args[0].value)?;
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
                            // Check visibility
                            if !self.can_access_method(class_name, method_signature.is_public) {
                                return Err(DiagnosticError::Type(
                                    format!("Method '{}' is private and cannot be called from outside class '{}'",
                                           method_name, class_name)
                                ));
                            }

                            // Count required parameters (those without defaults) - exclude implicit self parameter
                            let required_params = method_signature.default_values.iter().take_while(|d| d.is_none()).count();

                            // Check argument count is valid
                            if args.len() < required_params {
                                return Err(DiagnosticError::Type(
                                    format!("Method '{}::{}' expects at least {} arguments, got {}",
                                           class_name, method_name, required_params, args.len())
                                ));
                            }
                            if args.len() > method_signature.params.len() {
                                return Err(DiagnosticError::Type(
                                    format!("Method '{}::{}' expects at most {} arguments, got {}",
                                           class_name, method_name, method_signature.params.len(), args.len())
                                ));
                            }

                            // Check argument types
                            for (i, (arg, (param_name, expected_type))) in args.iter().zip(method_signature.params.iter()).enumerate() {
                                let arg_type = self.check_expression(&arg.value)?;
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
                    // Task methods
                    (HirType::Task(inner_type), "await") => {
                        if !args.is_empty() {
                            return Err(DiagnosticError::Type(
                                "await() method takes no arguments".to_string()
                            ));
                        }
                        // await() returns the inner type T from Task<T>
                        Ok((**inner_type).clone())
                    }
                    // Channel methods
                    (HirType::Channel(element_type), "send") => {
                        if args.len() != 1 {
                            return Err(DiagnosticError::Type(
                                "send() method takes exactly one argument".to_string()
                            ));
                        }
                        // Check that argument type matches channel element type
                        let arg_type = self.check_expression(&args[0].value)?;
                        if arg_type != **element_type {
                            return Err(DiagnosticError::Type(
                                format!("send() expects type {:?}, got {:?}", element_type, arg_type)
                            ));
                        }
                        Ok(HirType::Unit)
                    }
                    (HirType::Channel(element_type), "recv") => {
                        if !args.is_empty() {
                            return Err(DiagnosticError::Type(
                                "recv() method takes no arguments".to_string()
                            ));
                        }
                        // recv() returns Option<T> where T is the channel element type
                        Ok(HirType::Enum("Option".to_string(), vec![(**element_type).clone()]))
                    }
                    (HirType::Channel(_), "close") => {
                        if !args.is_empty() {
                            return Err(DiagnosticError::Type(
                                "close() method takes no arguments".to_string()
                            ));
                        }
                        Ok(HirType::Unit)
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

                // Check visibility for cross-module enum access
                // Enums with :: in their name are qualified (cross-module)
                if enum_name.contains("::") {
                    let enum_module = self.get_module_from_qualified_name(enum_name);
                    if !self.can_access_symbol(&enum_module, enum_info.is_public) {
                        return Err(DiagnosticError::Type(
                            format!("Enum '{}' is private and cannot be accessed from module '{}'",
                                   enum_name, self.module_table.current_module)
                        ));
                    }
                }

                // Check if variant exists
                let variant_fields = enum_info.variants.get(variant)
                    .ok_or_else(|| DiagnosticError::Type(
                        format!("Enum '{}' has no variant '{}'", enum_name, variant)
                    ))?.clone();

                // For generic enums (Option, Result), infer type parameters from arguments
                let mut inferred_type_params = vec![];

                if enum_name == "Option" && variant == "Some" && args.len() == 1 {
                    // Option::Some(value) - infer T from value type
                    let arg_type = self.check_expression(&args[0].value)?;
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
                    inferred_type_params.push(HirType::Int32);

                    if args.len() != 0 {
                        return Err(DiagnosticError::Type(
                            format!("Option::None expects 0 arguments, got {}", args.len())
                        ));
                    }
                } else if enum_name == "Result" && variant == "Ok" && args.len() == 1 {
                    // Result::Ok(value) - infer T from value type
                    let arg_type = self.check_expression(&args[0].value)?;
                    inferred_type_params.push(arg_type.clone());
                    inferred_type_params.push(HirType::Int32); // E defaults to I32

                    if args.len() != 1 {
                        return Err(DiagnosticError::Type(
                            format!("Result::Ok expects 1 argument, got {}", args.len())
                        ));
                    }
                } else if enum_name == "Result" && variant == "Err" && args.len() == 1 {
                    // Result::Err(error) - infer E from error type
                    let arg_type = self.check_expression(&args[0].value)?;
                    inferred_type_params.push(HirType::Int32); // T defaults to I32
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
                        let arg_type = self.check_expression(&arg.value)?;
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
                            // Check visibility
                            if !self.can_access_field(class_name, field_info.is_public) {
                                return Err(DiagnosticError::Type(
                                    format!("Field '{}' is private and cannot be accessed from outside class '{}'",
                                           member, class_name)
                                ));
                            }
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

                // Check visibility for cross-module class access
                // Classes with :: in their name are qualified (cross-module)
                if class_name.contains("::") {
                    let class_module = self.get_module_from_qualified_name(class_name);
                    if !self.can_access_symbol(&class_module, class_info.is_public) {
                        return Err(DiagnosticError::Type(
                            format!("Class '{}' is private and cannot be accessed from module '{}'",
                                   class_name, self.module_table.current_module)
                        ));
                    }
                }

                // Check if init method exists
                if !class_info.methods.contains_key("init") {
                    return Err(DiagnosticError::Type(
                        format!("Class '{}' has no init method", class_name)
                    ));
                }

                let init_signature = &class_info.methods["init"];

                // Count required parameters (those without defaults)
                let required_params = init_signature.default_values.iter().take_while(|d| d.is_none()).count();

                // Check argument count is valid
                if args.len() < required_params {
                    return Err(DiagnosticError::Type(
                        format!("Constructor for '{}' expects at least {} arguments, got {}",
                               class_name, required_params, args.len())
                    ));
                }
                if args.len() > init_signature.params.len() {
                    return Err(DiagnosticError::Type(
                        format!("Constructor for '{}' expects at most {} arguments, got {}",
                               class_name, init_signature.params.len(), args.len())
                    ));
                }

                // Check that all required fields (without defaults) are provided in named arguments
                let mut provided_fields = std::collections::HashSet::new();
                for arg in args {
                    provided_fields.insert(&arg.name);
                }

                // Check each parameter to see if it's required (has no default)
                for ((param_name, _param_type), default_val) in init_signature.params.iter().zip(init_signature.default_values.iter()) {
                    // If no default value and not provided, error
                    if default_val.is_none() && !provided_fields.contains(param_name) {
                        return Err(DiagnosticError::Type(
                            format!("Constructor for '{}' missing required field '{}'", class_name, param_name)
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
                            inferred_type_args.push(HirType::Int32);
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

                        // Check if argument type is compatible with field type (allows upcasting)
                        if !self.is_assignable(&expected_type, &arg_type) {
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

                // Validate named arguments match parameter names and types
                for arg in args {
                    let param = parent_method_signature.params.iter()
                        .find(|(param_name, _)| param_name == &arg.name)
                        .ok_or_else(|| DiagnosticError::Type(
                            format!("Super method '{}' has no parameter named '{}'", method, arg.name)
                        ))?;

                    let arg_type = self.check_expression(&arg.value)?;
                    if arg_type != param.1 {
                        return Err(DiagnosticError::Type(
                            format!("Super method '{}' parameter '{}' expects type {:?}, got {:?}", method, arg.name, param.1, arg_type)
                        ));
                    }
                }

                Ok(parent_method_signature.return_type)
            }
            Expression::Range { start, end, .. } => {
                let start_type = self.check_expression(start)?;
                let end_type = self.check_expression(end)?;

                // Both start and end must be integers (i32 or i64)
                if !matches!(start_type, HirType::Int32 | HirType::Int64) {
                    return Err(DiagnosticError::Type(
                        format!("Range start must be an integer type, got {:?}", start_type)
                    ));
                }

                if !matches!(end_type, HirType::Int32 | HirType::Int64) {
                    return Err(DiagnosticError::Type(
                        format!("Range end must be an integer type, got {:?}", end_type)
                    ));
                }

                // For simplicity, require both to be the same type
                if start_type != end_type {
                    return Err(DiagnosticError::Type(
                        format!("Range start and end must have the same type: {:?} vs {:?}", start_type, end_type)
                    ));
                }

                // A range expression is not directly usable except in for loops
                // We return the element type (the integer type)
                Ok(start_type)
            }
            Expression::If { condition, then_branch, else_branch, .. } => {
                // Check condition is bool
                let condition_type = self.check_expression(condition)?;
                if condition_type != HirType::Bool {
                    return Err(DiagnosticError::Type(
                        format!("If condition must be bool, got {:?}", condition_type)
                    ));
                }

                // Check then branch
                let then_type = self.check_expression(then_branch)?;

                // If there's an else branch, check it and ensure both branches have the same type
                if let Some(else_expr) = else_branch {
                    let else_type = self.check_expression(else_expr)?;
                    if then_type != else_type {
                        return Err(DiagnosticError::Type(
                            format!("If-expression branches must have the same type: then={:?}, else={:?}", then_type, else_type)
                        ));
                    }
                    Ok(then_type)
                } else {
                    // If there's no else branch, the expression returns Unit
                    Ok(HirType::Unit)
                }
            }
            Expression::Cast { value, target_type, .. } => {
                // Check the value expression
                let value_type = self.check_expression(value)?;

                // Convert AST type to HIR type
                let target_hir_type = self.ast_type_to_hir_type(target_type)?;

                // Validate that both source and target are numeric types
                if !self.is_numeric_type(&value_type) {
                    return Err(DiagnosticError::Type(
                        format!("Cannot cast non-numeric type {:?} to {:?}", value_type, target_hir_type)
                    ));
                }

                if !self.is_numeric_type(&target_hir_type) {
                    return Err(DiagnosticError::Type(
                        format!("Cannot cast to non-numeric type {:?}", target_hir_type)
                    ));
                }

                Ok(target_hir_type)
            }
            Expression::Spawn { body, span } => {
                // Validate that spawn is inside a concurrent block
                if !self.in_concurrent_block {
                    return Err(DiagnosticError::Rich(
                        Diagnostic::syntax_error(
                            &self.filename,
                            *span,
                            "spawn can only be used inside a concurrent block"
                        )
                        .with_label("spawn must be inside concurrent { ... }")
                        .with_help("Wrap this in a concurrent block:\n  concurrent {\n    spawn { ... }\n  }")
                    ));
                }

                // Type check the spawn body and infer its return type
                // Special handling for block expressions to infer type from return statements
                let body_type = match body.as_ref() {
                    Expression::Block(block) => {
                        // Infer return type from the block's return statements
                        self.infer_block_return_type(block)?
                    }
                    _ => {
                        // For other expressions, just check normally
                        self.check_expression(body)?
                    }
                };

                // Return Task<T> where T is the body's type
                Ok(HirType::Task(Box::new(body_type)))
            }
        }
    }

    /// Infer the return type of a block by finding return statements
    /// Used for spawn blocks which should return Task<T> based on their return type
    fn infer_block_return_type(&mut self, block: &Block) -> Result<HirType, DiagnosticError> {
        // Check block in a new scope
        self.push_scope();

        let mut return_type = HirType::Unit; // Default to Unit if no return found

        // Look through statements to find return statements
        for statement in &block.statements {
            if let Some(ret_type) = self.find_return_type_in_statement(statement)? {
                return_type = ret_type;
                break; // Use first return found
            }
        }

        self.pop_scope();
        Ok(return_type)
    }

    /// Recursively search for return statements and get their type
    fn find_return_type_in_statement(&mut self, statement: &Statement) -> Result<Option<HirType>, DiagnosticError> {
        match statement {
            Statement::Return { value, .. } => {
                if let Some(expr) = value {
                    let ret_type = self.check_expression(expr)?;
                    Ok(Some(ret_type))
                } else {
                    Ok(Some(HirType::Unit))
                }
            }
            Statement::If { condition, then_branch, else_branch, .. } => {
                // Check condition first
                self.check_expression(condition)?;

                // Look for returns in then branch
                if let Some(ret_type) = self.find_return_type_in_block(then_branch)? {
                    return Ok(Some(ret_type));
                }

                // Look for returns in else branch if it exists
                if let Some(else_blk) = else_branch {
                    if let Some(ret_type) = self.find_return_type_in_block(else_blk)? {
                        return Ok(Some(ret_type));
                    }
                }

                Ok(None)
            }
            Statement::While { condition, body, .. } => {
                self.check_expression(condition)?;
                self.find_return_type_in_block(body)
            }
            Statement::For { .. } => {
                // Type-check but don't look for returns in for loops for simplicity
                self.check_statement(statement)?;
                Ok(None)
            }
            _ => {
                // Other statements - just type check normally
                self.check_statement(statement)?;
                Ok(None)
            }
        }
    }

    /// Find return type in a block
    fn find_return_type_in_block(&mut self, block: &Block) -> Result<Option<HirType>, DiagnosticError> {
        for statement in &block.statements {
            if let Some(ret_type) = self.find_return_type_in_statement(statement)? {
                return Ok(Some(ret_type));
            }
        }
        Ok(None)
    }

    fn check_literal(&mut self, literal: &Literal) -> Result<HirType, DiagnosticError> {
        match literal {
            Literal::Bool(_, _) => Ok(HirType::Bool),
            Literal::Integer(_, int_type, _) => {
                match int_type {
                    IntType::I8 => Ok(HirType::Int8),
                    IntType::I16 => Ok(HirType::Int16),
                    IntType::I32 => Ok(HirType::Int32),
                    IntType::I64 => Ok(HirType::Int64),
                }
            }
            Literal::Float(_, float_type, _) => {
                match float_type {
                    FloatType::F8 => Ok(HirType::Float8),
                    FloatType::F16 => Ok(HirType::Float16),
                    FloatType::F32 => Ok(HirType::Float32),
                    FloatType::F64 => Ok(HirType::Float64),
                }
            }
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
            BinaryOp::Add | BinaryOp::Subtract | BinaryOp::Multiply | BinaryOp::Divide => {
                match (left, right) {
                    (HirType::Int8, HirType::Int8) => Ok(HirType::Int8),
                    (HirType::Int16, HirType::Int16) => Ok(HirType::Int16),
                    (HirType::Int32, HirType::Int32) => Ok(HirType::Int32),
                    (HirType::Int64, HirType::Int64) => Ok(HirType::Int64),
                    (HirType::Float8, HirType::Float8) => Ok(HirType::Float8),
                    (HirType::Float16, HirType::Float16) => Ok(HirType::Float16),
                    (HirType::Float32, HirType::Float32) => Ok(HirType::Float32),
                    (HirType::Float64, HirType::Float64) => Ok(HirType::Float64),
                    (HirType::String, HirType::String) if matches!(op, BinaryOp::Add) => Ok(HirType::String),
                    _ => Err(DiagnosticError::Type(
                        format!("Cannot apply {:?} to types {:?} and {:?}", op, left, right)
                    ))
                }
            }
            BinaryOp::Modulo => {
                // Modulo only works with integers, not floats
                match (left, right) {
                    (HirType::Int8, HirType::Int8) => Ok(HirType::Int8),
                    (HirType::Int16, HirType::Int16) => Ok(HirType::Int16),
                    (HirType::Int32, HirType::Int32) => Ok(HirType::Int32),
                    (HirType::Int64, HirType::Int64) => Ok(HirType::Int64),
                    _ => Err(DiagnosticError::Type(
                        format!("Modulo operator requires integer operands, got {:?} and {:?}", left, right)
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
                    (HirType::Int8, HirType::Int8) | (HirType::Int16, HirType::Int16) |
                    (HirType::Int32, HirType::Int32) | (HirType::Int64, HirType::Int64) |
                    (HirType::Float8, HirType::Float8) | (HirType::Float16, HirType::Float16) |
                    (HirType::Float32, HirType::Float32) | (HirType::Float64, HirType::Float64) => Ok(HirType::Bool),
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
                    HirType::Int8 => Ok(HirType::Int8),
                    HirType::Int16 => Ok(HirType::Int16),
                    HirType::Int32 => Ok(HirType::Int32),
                    HirType::Int64 => Ok(HirType::Int64),
                    HirType::Float8 => Ok(HirType::Float8),
                    HirType::Float16 => Ok(HirType::Float16),
                    HirType::Float32 => Ok(HirType::Float32),
                    HirType::Float64 => Ok(HirType::Float64),
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
            Type::Int8 => Ok(HirType::Int8),
            Type::Int16 => Ok(HirType::Int16),
            Type::Int32 => Ok(HirType::Int32),
            Type::Int64 => Ok(HirType::Int64),
            Type::Float8 => Ok(HirType::Float8),
            Type::Float16 => Ok(HirType::Float16),
            Type::Float32 => Ok(HirType::Float32),
            Type::Float64 => Ok(HirType::Float64),
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
                // Check for built-in Task type first
                if name == "Task" {
                    if type_params.len() != 1 {
                        return Err(DiagnosticError::Type(
                            "Task requires exactly one type parameter".to_string()
                        ));
                    }
                    let inner_type = self.ast_type_to_hir_type(&type_params[0])?;
                    return Ok(HirType::Task(Box::new(inner_type)));
                }

                // Check for built-in Channel type
                if name == "Channel" {
                    if type_params.len() != 1 {
                        return Err(DiagnosticError::Type(
                            "Channel requires exactly one type parameter".to_string()
                        ));
                    }
                    let inner_type = self.ast_type_to_hir_type(&type_params[0])?;
                    return Ok(HirType::Channel(Box::new(inner_type)));
                }

                // Check if this is a newtype first (distinct from type aliases)
                if self.newtypes.contains_key(name) {
                    // Newtypes shouldn't have type parameters
                    if !type_params.is_empty() {
                        return Err(DiagnosticError::Type(
                            format!("Newtype '{}' cannot have type arguments", name)
                        ));
                    }
                    Ok(HirType::Newtype(name.clone()))
                }
                // Check if this is a type alias
                else if self.type_aliases.contains_key(name) {
                    // Type aliases shouldn't have type parameters
                    if !type_params.is_empty() {
                        return Err(DiagnosticError::Type(
                            format!("Type alias '{}' cannot have type arguments", name)
                        ));
                    }
                    Ok(self.type_aliases[name].clone())
                }
                // Check if this is a type parameter (T, U, etc.)
                else if self.type_parameters.contains(name) {
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

                // Add bindings to current scope and verify explicit types match field types
                for ((binding_name, binding_type), field_type) in bindings.iter().zip(actual_field_types.iter()) {
                    // Validate binding name follows snake_case
                    if !is_snake_case(binding_name) {
                        return Err(DiagnosticError::Type(
                            format!("Pattern binding '{}' must be snake_case", binding_name)
                        ));
                    }

                    if self.scopes.last().unwrap().contains_key(binding_name) {
                        return Err(DiagnosticError::Type(
                            format!("Variable '{}' is already bound in this pattern", binding_name)
                        ));
                    }

                    // Convert explicit binding type to HIR type and verify it matches field type
                    let explicit_binding_type = self.ast_type_to_hir_type(binding_type)?;
                    if explicit_binding_type != *field_type {
                        return Err(DiagnosticError::Type(
                            format!("Pattern binding '{}' has type {:?}, but variant field has type {:?}",
                                binding_name, explicit_binding_type, field_type)
                        ));
                    }

                    self.scopes.last_mut().unwrap().insert(binding_name.clone(), explicit_binding_type);
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

    fn check_test_block(&mut self, test_block: &TestBlock) -> Result<(), DiagnosticError> {
        // Validate test block name follows snake_case convention
        if !is_snake_case(&test_block.name) {
            return Err(DiagnosticError::Rich(
                Diagnostic::type_error(
                    &self.filename,
                    test_block.span,
                    format!("Test block name '{}' must be snake_case", test_block.name)
                )
                .with_label("test block name must be snake_case")
                .with_help(format!("Try renaming to: {}", to_snake_case(&test_block.name)))
            ));
        }

        // Validate test block name is unique
        if !self.test_block_names.insert(test_block.name.clone()) {
            return Err(DiagnosticError::Rich(
                Diagnostic::type_error(
                    &self.filename,
                    test_block.span,
                    format!("Duplicate test block name '{}'", test_block.name)
                )
                .with_label("test block name must be unique within the module")
                .with_help("Each test block must have a unique identifier")
            ));
        }

        // Check if there's a before_each hook
        let has_before_each = test_block.functions.iter().any(|f| f.name == "before_each");

        // Check each function in the test block
        for function in &test_block.functions {
            // Validate test function naming convention
            if function.name.starts_with("test_") {
                // This is a test function - validate it
                if has_before_each {
                    // When before_each exists, test functions can have exactly one parameter named "ctx"
                    if function.params.len() > 1 {
                        return Err(DiagnosticError::Type(
                            format!("Test function '{}' can have at most one parameter (ctx) when using before_each", function.name)
                        ));
                    }
                    if function.params.len() == 1 && function.params[0].name != "ctx" {
                        return Err(DiagnosticError::Type(
                            format!("Test function '{}' parameter must be named 'ctx' when using before_each", function.name)
                        ));
                    }
                } else {
                    // Without before_each, test functions must have no parameters
                    if !function.params.is_empty() {
                        return Err(DiagnosticError::Type(
                            format!("Test function '{}' must not have parameters (use before_each hook to provide context)", function.name)
                        ));
                    }
                }

                // Test functions should return Unit (no return value)
                if function.return_type.is_some() {
                    return Err(DiagnosticError::Type(
                        format!("Test function '{}' must not have a return type", function.name)
                    ));
                }
            }

            // Type check the function
            self.check_function(function)?;
        }

        Ok(())
    }

    fn check_bench_block(&mut self, bench_block: &BenchBlock) -> Result<(), DiagnosticError> {
        // Validate bench block name follows snake_case convention
        if !is_snake_case(&bench_block.name) {
            return Err(DiagnosticError::Rich(
                Diagnostic::type_error(
                    &self.filename,
                    bench_block.span,
                    format!("Bench block name '{}' must be snake_case", bench_block.name)
                )
                .with_label("bench block name must be snake_case")
                .with_help(format!("Try renaming to: {}", to_snake_case(&bench_block.name)))
            ));
        }

        // Validate bench block name is unique
        if !self.bench_block_names.insert(bench_block.name.clone()) {
            return Err(DiagnosticError::Rich(
                Diagnostic::type_error(
                    &self.filename,
                    bench_block.span,
                    format!("Duplicate bench block name '{}'", bench_block.name)
                )
                .with_label("bench block name must be unique within the module")
                .with_help("Each bench block must have a unique identifier")
            ));
        }

        // Check each function in the bench block
        for function in &bench_block.functions {
            // Validate bench function naming convention
            if function.name.starts_with("bench_") {
                // This is a bench function - validate it
                if !function.params.is_empty() {
                    return Err(DiagnosticError::Type(
                        format!("Bench function '{}' must not have parameters", function.name)
                    ));
                }

                // Bench functions should return Unit (no return value)
                if function.return_type.is_some() {
                    return Err(DiagnosticError::Type(
                        format!("Bench function '{}' must not have a return type", function.name)
                    ));
                }
            }

            // Type check the function
            self.check_function(function)?;
        }

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
/// Maps type parameter names (like "T", "U") to concrete types (like HirType::Int32)
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
            HirType::Task(inner_type) => {
                HirType::Task(Box::new(inner_type.substitute_types(substitution)))
            }
            HirType::Channel(inner_type) => {
                HirType::Channel(Box::new(inner_type.substitute_types(substitution)))
            }
            // Primitive types and newtypes don't need substitution
            HirType::Bool | HirType::Int8 | HirType::Int16 | HirType::Int32 | HirType::Int64 | HirType::Float8 | HirType::Float16 | HirType::Float32 | HirType::Float64 | HirType::String | HirType::Unit | HirType::Newtype(_) => {
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
            is_public: self.is_public,
        }
    }
}

impl TypeSubstitutable for FunctionSignature {
    fn substitute_types(&self, substitution: &TypeSubstitution) -> Self {
        FunctionSignature {
            type_params: self.type_params.clone(), // Type params don't need substitution
            params: self.params.iter().map(|(name, ty)| (name.clone(), ty.substitute_types(substitution))).collect(),
            default_values: self.default_values.clone(), // Default values are expressions, not types
            return_type: self.return_type.substitute_types(substitution),
            is_mutable: self.is_mutable,
            is_public: self.is_public,
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
    specialized_functions: HashMap<String, FunctionSignature>,

    /// Counter for generating unique specialized names
    specialization_counter: usize,
}

impl Monomorphizer {
    pub fn new() -> Self {
        Self {
            instantiations: HashMap::new(),
            specialized_classes: HashMap::new(),
            specialized_enums: HashMap::new(),
            specialized_functions: HashMap::new(),
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
            is_public: class_info.is_public, // Preserve visibility from original
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
            is_public: enum_info.is_public, // Preserve visibility from original
        };

        // Store the specialized enum
        self.specialized_enums.insert(specialized_name.clone(), specialized_enum);
        self.instantiations.insert(key, specialized_name.clone());

        Ok(specialized_name)
    }

    /// Get or create a specialized version of a generic function
    pub fn specialize_function(&mut self, func_sig: &FunctionSignature, func_name: &str, type_args: &[HirType]) -> Result<String, DiagnosticError> {
        // If not generic, return original name
        if func_sig.type_params.is_empty() {
            return Ok(func_name.to_string());
        }

        // Check if we already specialized this combination
        let key = (func_name.to_string(), type_args.to_vec());
        if let Some(specialized_name) = self.instantiations.get(&key) {
            return Ok(specialized_name.clone());
        }

        // Validate type argument count
        if func_sig.type_params.len() != type_args.len() {
            return Err(DiagnosticError::Type(
                format!("Function '{}' expects {} type arguments, got {}",
                    func_name, func_sig.type_params.len(), type_args.len())
            ));
        }

        // Create type substitution map
        let mut substitution = TypeSubstitution::new();
        for (param_name, concrete_type) in func_sig.type_params.iter().zip(type_args.iter()) {
            substitution.insert(param_name.clone(), concrete_type.clone());
        }

        // Generate specialized name
        let specialized_name = format!("{}$specialized${}", func_name, self.specialization_counter);
        self.specialization_counter += 1;

        // Create specialized function signature
        let specialized_params: Vec<(String, HirType)> = func_sig.params.iter()
            .map(|(name, ty)| (name.clone(), ty.substitute_types(&substitution)))
            .collect();

        let specialized_return = func_sig.return_type.substitute_types(&substitution);

        let specialized_func = FunctionSignature {
            type_params: vec![], // Specialized functions are not generic
            params: specialized_params,
            default_values: func_sig.default_values.clone(), // Keep default values from original
            return_type: specialized_return,
            is_mutable: func_sig.is_mutable,
            is_public: func_sig.is_public,
        };

        // Store the specialized function
        self.specialized_functions.insert(specialized_name.clone(), specialized_func);
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

    /// Get all specialized functions generated so far
    pub fn get_specialized_functions(&self) -> &HashMap<String, FunctionSignature> {
        &self.specialized_functions
    }
}

impl TypeChecker {
    /// Get the monomorphized types after type checking
    pub fn get_monomorphized_types(self) -> (HashMap<String, ClassInfo>, HashMap<String, EnumInfo>, HashMap<String, FunctionSignature>) {
        (self.monomorphizer.specialized_classes, self.monomorphizer.specialized_enums, self.monomorphizer.specialized_functions)
    }

    /// Get a reference to the monomorphizer for debugging
    pub fn get_monomorphizer(&self) -> &Monomorphizer {
        &self.monomorphizer
    }

    /// Fill in default arguments for all function, method, and constructor calls in the program
    pub fn fill_default_arguments(&mut self, program: &mut Program) {
        // Transform all functions
        for function in &mut program.functions {
            self.fill_defaults_in_function(function);
        }

        // Transform test blocks
        for test_block in &mut program.test_blocks {
            for function in &mut test_block.functions {
                self.fill_defaults_in_function(function);
            }
        }

        // Transform bench blocks
        for bench_block in &mut program.bench_blocks {
            for function in &mut bench_block.functions {
                self.fill_defaults_in_function(function);
            }
        }

        // Transform class methods
        for class in &mut program.classes {
            for method in &mut class.methods {
                self.fill_defaults_in_function(method);
            }
        }

        // Transform enum methods
        for enum_decl in &mut program.enums {
            for method in &mut enum_decl.methods {
                self.fill_defaults_in_function(method);
            }
        }
    }

    fn fill_defaults_in_function(&mut self, function: &mut Function) {
        // Build a map of variable types from the function body
        let mut var_types: HashMap<String, String> = HashMap::new();
        self.collect_variable_types(&function.body, &mut var_types);

        self.fill_defaults_in_block(&mut function.body, &var_types);
    }

    fn collect_variable_types(&self, block: &Block, var_types: &mut HashMap<String, String>) {
        for statement in &block.statements {
            match statement {
                Statement::Let { name, ty, .. } | Statement::Var { name, ty, .. } => {
                    if let Type::Named(class_name, _) = ty {
                        var_types.insert(name.clone(), class_name.clone());
                    }
                }
                Statement::For { variable, variable_type, body, .. } => {
                    if let Type::Named(class_name, _) = variable_type {
                        var_types.insert(variable.clone(), class_name.clone());
                    }
                    self.collect_variable_types(body, var_types);
                }
                Statement::If { then_branch, else_branch, .. } => {
                    self.collect_variable_types(then_branch, var_types);
                    if let Some(else_block) = else_branch {
                        self.collect_variable_types(else_block, var_types);
                    }
                }
                Statement::While { body, .. } => {
                    self.collect_variable_types(body, var_types);
                }
                _ => {}
            }
        }
    }

    fn fill_defaults_in_block(&mut self, block: &mut Block, var_types: &HashMap<String, String>) {
        for statement in &mut block.statements {
            self.fill_defaults_in_statement(statement, var_types);
        }
    }

    fn fill_defaults_in_statement(&mut self, statement: &mut Statement, var_types: &HashMap<String, String>) {
        match statement {
            Statement::Let { value, .. } | Statement::Var { value, .. } => {
                self.fill_defaults_in_expression(value, var_types);
            }
            Statement::Expression(expr) => {
                self.fill_defaults_in_expression(expr, var_types);
            }
            Statement::Return { value: Some(expr), .. } => {
                self.fill_defaults_in_expression(expr, var_types);
            }
            Statement::If { condition, then_branch, else_branch, .. } => {
                self.fill_defaults_in_expression(condition, var_types);
                self.fill_defaults_in_block(then_branch, var_types);
                if let Some(else_block) = else_branch {
                    self.fill_defaults_in_block(else_block, var_types);
                }
            }
            Statement::While { condition, body, .. } => {
                self.fill_defaults_in_expression(condition, var_types);
                self.fill_defaults_in_block(body, var_types);
            }
            Statement::For { iterable, body, .. } => {
                self.fill_defaults_in_expression(iterable, var_types);
                self.fill_defaults_in_block(body, var_types);
            }
            Statement::Print { value, .. } => {
                self.fill_defaults_in_expression(value, var_types);
            }
            _ => {}
        }
    }

    fn fill_defaults_in_expression(&mut self, expr: &mut Expression, var_types: &HashMap<String, String>) {
        match expr {
            Expression::Call { function, args, span } => {
                // First, recursively process all argument expressions
                for arg in args.iter_mut() {
                    self.fill_defaults_in_expression(&mut arg.value, var_types);
                }

                // Look up function signature
                let resolved_name = self.module_table.resolve(function).unwrap_or_else(|| function.clone());
                if let Some(sig) = self.functions.get(&resolved_name).or_else(|| self.functions.get(function)) {
                    // Build a map of provided arguments
                    let mut provided: HashMap<String, usize> = HashMap::new();
                    for (i, arg) in args.iter().enumerate() {
                        provided.insert(arg.name.clone(), i);
                    }

                    // Fill in missing arguments with defaults
                    for (i, ((param_name, _param_type), default_val)) in sig.params.iter().zip(sig.default_values.iter()).enumerate() {
                        if !provided.contains_key(param_name) {
                            if let Some(default_expr) = default_val {
                                args.push(NamedArg {
                                    name: param_name.clone(),
                                    value: default_expr.clone(),
                                    span: *span,
                                });
                            }
                        }
                    }
                }
            }
            Expression::MethodCall { object, method, args, span } => {
                // Process object and arguments
                self.fill_defaults_in_expression(object, var_types);
                for arg in args.iter_mut() {
                    self.fill_defaults_in_expression(&mut arg.value, var_types);
                }

                // Fill in defaults for method calls
                // Try to determine object type by looking at the object expression
                let class_name_opt = match object.as_ref() {
                    Expression::Identifier { name, .. } => {
                        // Look up variable type from var_types map
                        var_types.get(name).cloned()
                    }
                    Expression::ConstructorCall { class_name, .. } => {
                        Some(class_name.clone())
                    }
                    Expression::MethodCall { .. } => {
                        // For chained method calls, we'd need to infer the return type
                        // Skip for now - will be validated during type checking
                        None
                    }
                    _ => None,
                };

                if let Some(class_name) = class_name_opt {
                    if let Some(class_info) = self.classes.get(&class_name) {
                        if let Some(method_sig) = class_info.methods.get(method) {
                            // Build a map of provided arguments
                            let mut provided: HashMap<String, usize> = HashMap::new();
                            for (i, arg) in args.iter().enumerate() {
                                provided.insert(arg.name.clone(), i);
                            }

                            // Fill in missing arguments with defaults
                            for ((param_name, _param_type), default_val) in method_sig.params.iter().zip(method_sig.default_values.iter()) {
                                if !provided.contains_key(param_name) {
                                    if let Some(default_expr) = default_val {
                                        args.push(NamedArg {
                                            name: param_name.clone(),
                                            value: default_expr.clone(),
                                            span: *span,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Expression::ConstructorCall { class_name, args, span } => {
                // Process arguments
                for arg in args.iter_mut() {
                    self.fill_defaults_in_expression(&mut arg.value, var_types);
                }

                // Look up class and its init method
                if let Some(class_info) = self.classes.get(class_name) {
                    if let Some(init_sig) = class_info.methods.get("init") {
                        // Build a map of provided arguments
                        let mut provided: HashMap<String, usize> = HashMap::new();
                        for (i, arg) in args.iter().enumerate() {
                            provided.insert(arg.name.clone(), i);
                        }

                        // Fill in missing arguments with defaults
                        for ((_param_name, _param_type), default_val) in init_sig.params.iter().zip(init_sig.default_values.iter()) {
                            if let Some(default_expr) = default_val {
                                let param_name = _param_name;
                                if !provided.contains_key(param_name) {
                                    args.push(NamedArg {
                                        name: param_name.clone(),
                                        value: default_expr.clone(),
                                        span: *span,
                                    });
                                }
                            }
                        }
                    }
                }
            }
            // Recursively process other expression types
            Expression::Binary { left, right, .. } => {
                self.fill_defaults_in_expression(left, var_types);
                self.fill_defaults_in_expression(right, var_types);
            }
            Expression::Unary { operand, .. } => {
                self.fill_defaults_in_expression(operand, var_types);
            }
            Expression::Assignment { target, value, .. } => {
                self.fill_defaults_in_expression(target, var_types);
                self.fill_defaults_in_expression(value, var_types);
            }
            Expression::Index { object, index, .. } => {
                self.fill_defaults_in_expression(object, var_types);
                self.fill_defaults_in_expression(index, var_types);
            }
            Expression::Block(block) => {
                self.fill_defaults_in_block(block, var_types);
            }
            Expression::Match { value, arms, .. } => {
                self.fill_defaults_in_expression(value, var_types);
                for arm in arms {
                    self.fill_defaults_in_expression(&mut arm.body, var_types);
                }
            }
            Expression::Try { expression, .. } => {
                self.fill_defaults_in_expression(expression, var_types);
            }
            Expression::MemberAccess { object, .. } => {
                self.fill_defaults_in_expression(object, var_types);
            }
            Expression::Range { start, end, .. } => {
                self.fill_defaults_in_expression(start, var_types);
                self.fill_defaults_in_expression(end, var_types);
            }
            Expression::If { condition, then_branch, else_branch, .. } => {
                self.fill_defaults_in_expression(condition, var_types);
                self.fill_defaults_in_expression(then_branch, var_types);
                if let Some(else_expr) = else_branch {
                    self.fill_defaults_in_expression(else_expr, var_types);
                }
            }
            Expression::Cast { value, .. } => {
                self.fill_defaults_in_expression(value, var_types);
            }
            Expression::Literal(Literal::InterpolatedString(parts, _)) => {
                for part in parts {
                    if let InterpolationPart::Expression(expr) = part {
                        self.fill_defaults_in_expression(expr, var_types);
                    }
                }
            }
            Expression::Literal(Literal::Array(elements, _)) => {
                for elem in elements {
                    self.fill_defaults_in_expression(elem, var_types);
                }
            }
            Expression::Literal(Literal::Dict(pairs, _)) => {
                for (key, value) in pairs {
                    self.fill_defaults_in_expression(key, var_types);
                    self.fill_defaults_in_expression(value, var_types);
                }
            }
            Expression::Literal(Literal::Set(elements, _)) => {
                for elem in elements {
                    self.fill_defaults_in_expression(elem, var_types);
                }
            }
            Expression::EnumConstructor { args, .. } => {
                for arg in args {
                    self.fill_defaults_in_expression(&mut arg.value, var_types);
                }
            }
            Expression::SuperCall { args, .. } => {
                for arg in args {
                    self.fill_defaults_in_expression(&mut arg.value, var_types);
                }
            }
            _ => {}
        }
    }
}