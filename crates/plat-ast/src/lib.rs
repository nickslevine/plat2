use plat_lexer::Span;

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub module_decl: Option<ModuleDecl>,
    pub use_decls: Vec<UseDecl>,
    pub type_aliases: Vec<TypeAlias>,
    pub newtypes: Vec<NewtypeDecl>,
    pub test_blocks: Vec<TestBlock>,
    pub bench_blocks: Vec<BenchBlock>,
    pub functions: Vec<Function>,
    pub enums: Vec<EnumDecl>,
    pub classes: Vec<ClassDecl>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModuleDecl {
    pub path: Vec<String>, // e.g., ["database", "connection"] for "mod database::connection;"
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UseDecl {
    pub path: Vec<String>, // e.g., ["database"] for "use database;"
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypeAlias {
    pub name: String,
    pub ty: Type,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewtypeDecl {
    pub name: String,
    pub underlying_type: Type,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TestBlock {
    pub name: String, // Test block description
    pub functions: Vec<Function>, // All functions within the test block
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BenchBlock {
    pub name: String, // Benchmark block description
    pub functions: Vec<Function>, // All functions within the benchmark block
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub name: String,
    pub type_params: Vec<String>, // Generic type parameters, e.g., <T, U>
    pub params: Vec<Parameter>,
    pub return_type: Option<Type>,
    pub body: Block,
    pub is_mutable: bool,
    pub is_virtual: bool,    // true if method is virtual (can be overridden)
    pub is_override: bool,   // true if method overrides a parent method
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Parameter {
    pub name: String,
    pub ty: Type,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
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
    List(Box<Type>),
    Dict(Box<Type>, Box<Type>), // Key type, Value type
    Set(Box<Type>), // Element type
    Named(String, Vec<Type>), // e.g., Option<T>, Message
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub statements: Vec<Statement>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    Let {
        name: String,
        ty: Type,
        value: Expression,
        span: Span,
    },
    Var {
        name: String,
        ty: Type,
        value: Expression,
        span: Span,
    },
    Expression(Expression),
    Return {
        value: Option<Expression>,
        span: Span,
    },
    If {
        condition: Expression,
        then_branch: Block,
        else_branch: Option<Block>,
        span: Span,
    },
    While {
        condition: Expression,
        body: Block,
        span: Span,
    },
    For {
        variable: String,
        variable_type: Type,
        iterable: Expression,
        body: Block,
        span: Span,
    },
    Print {
        value: Expression,
        span: Span,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expression {
    Literal(Literal),
    Identifier {
        name: String,
        span: Span,
    },
    Binary {
        left: Box<Expression>,
        op: BinaryOp,
        right: Box<Expression>,
        span: Span,
    },
    Unary {
        op: UnaryOp,
        operand: Box<Expression>,
        span: Span,
    },
    Call {
        function: String,
        args: Vec<NamedArg>,
        span: Span,
    },
    Assignment {
        target: Box<Expression>, // Can be Identifier or MemberAccess
        value: Box<Expression>,
        span: Span,
    },
    Index {
        object: Box<Expression>,
        index: Box<Expression>,
        span: Span,
    },
    MethodCall {
        object: Box<Expression>,
        method: String,
        args: Vec<NamedArg>,
        span: Span,
    },
    Block(Block),
    EnumConstructor {
        enum_name: String,
        variant: String,
        args: Vec<NamedArg>,
        span: Span,
    },
    Match {
        value: Box<Expression>,
        arms: Vec<MatchArm>,
        span: Span,
    },
    Try {
        expression: Box<Expression>,
        span: Span,
    },
    Self_ {
        span: Span,
    },
    MemberAccess {
        object: Box<Expression>,
        member: String,
        span: Span,
    },
    ConstructorCall {
        class_name: String,
        args: Vec<NamedArg>,
        span: Span,
    },
    SuperCall {
        method: String,
        args: Vec<NamedArg>,
        span: Span,
    },
    Range {
        start: Box<Expression>,
        end: Box<Expression>,
        inclusive: bool, // true for ..=, false for ..
        span: Span,
    },
    If {
        condition: Box<Expression>,
        then_branch: Box<Expression>,
        else_branch: Option<Box<Expression>>,
        span: Span,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Bool(bool, Span),
    Integer(i64, Span),
    Float(f64, FloatType, Span), // value, type (f32/f64), span
    String(String, Span),
    InterpolatedString(Vec<InterpolationPart>, Span),
    Array(Vec<Expression>, Span),
    Dict(Vec<(Expression, Expression)>, Span), // Key-value pairs
    Set(Vec<Expression>, Span), // Set elements
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FloatType {
    F32,
    F64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InterpolationPart {
    Text(String),
    Expression(Box<Expression>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOp {
    Not,
    Negate,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumDecl {
    pub name: String,
    pub type_params: Vec<String>,
    pub variants: Vec<EnumVariant>,
    pub methods: Vec<Function>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumVariant {
    pub name: String,
    pub fields: Vec<Type>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Expression,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    EnumVariant {
        enum_name: Option<String>,
        variant: String,
        bindings: Vec<(String, Type)>,
        span: Span,
    },
    Identifier {
        name: String,
        span: Span,
    },
    Literal(Literal),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClassDecl {
    pub name: String,
    pub type_params: Vec<String>,
    pub parent_class: Option<String>, // None for no inheritance, Some(name) for inheritance
    pub fields: Vec<FieldDecl>,
    pub methods: Vec<Function>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldDecl {
    pub name: String,
    pub ty: Type,
    pub is_mutable: bool, // true for var, false for let
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NamedArg {
    pub name: String,
    pub value: Expression,
    pub span: Span,
}