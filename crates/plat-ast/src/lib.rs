use plat_lexer::Span;

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub functions: Vec<Function>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub name: String,
    pub params: Vec<Parameter>,
    pub return_type: Option<Type>,
    pub body: Block,
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
    I32,
    I64,
    String,
    List(Box<Type>),
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
        ty: Option<Type>,
        value: Expression,
        span: Span,
    },
    Var {
        name: String,
        ty: Option<Type>,
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
        args: Vec<Expression>,
        span: Span,
    },
    Assignment {
        name: String,
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
        args: Vec<Expression>,
        span: Span,
    },
    Block(Block),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Bool(bool, Span),
    Integer(i64, Span),
    String(String, Span),
    InterpolatedString(Vec<InterpolationPart>, Span),
    Array(Vec<Expression>, Span),
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