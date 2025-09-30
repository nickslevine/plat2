#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Keywords
    Fn,
    Let,
    Var,
    If,
    Else,
    While,
    For,
    In,
    Return,
    True,
    False,
    Print,
    List,
    Dict,
    Set,
    Enum,
    Match,
    Mut,
    Class,
    Init,
    Self_,
    Virtual,
    Override,
    Super,

    // Identifiers and literals
    Ident(String),
    IntLiteral(i64),
    StringLiteral(String),
    InterpolatedString(Vec<StringPart>),

    // Operators
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    And,
    Or,
    Not,
    Assign,
    Eq,
    NotEq,
    Less,
    LessEq,
    Greater,
    GreaterEq,
    Question,
    DotDot,      // .. (exclusive range)
    DotDotEq,    // ..= (inclusive range)

    // Punctuation
    LeftParen,
    RightParen,
    LeftBrace,
    RightBrace,
    LeftBracket,
    RightBracket,
    Semicolon,
    Comma,
    Arrow,
    Colon,
    Dot,
    DoubleColon,

    // Special
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StringPart {
    Text(String),
    Interpolation(String),
}

impl Token {
    pub fn keyword_from_str(s: &str) -> Option<Token> {
        match s {
            "fn" => Some(Token::Fn),
            "let" => Some(Token::Let),
            "var" => Some(Token::Var),
            "if" => Some(Token::If),
            "else" => Some(Token::Else),
            "while" => Some(Token::While),
            "for" => Some(Token::For),
            "in" => Some(Token::In),
            "return" => Some(Token::Return),
            "true" => Some(Token::True),
            "false" => Some(Token::False),
            "print" => Some(Token::Print),
            "List" => Some(Token::List),
            "Dict" => Some(Token::Dict),
            "Set" => Some(Token::Set),
            "enum" => Some(Token::Enum),
            "match" => Some(Token::Match),
            "mut" => Some(Token::Mut),
            "class" => Some(Token::Class),
            "init" => Some(Token::Init),
            "self" => Some(Token::Self_),
            "virtual" => Some(Token::Virtual),
            "override" => Some(Token::Override),
            "super" => Some(Token::Super),
            "and" => Some(Token::And),
            "or" => Some(Token::Or),
            "not" => Some(Token::Not),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TokenWithSpan {
    pub token: Token,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}