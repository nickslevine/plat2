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
    Return,
    True,
    False,
    Print,

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

    // Punctuation
    LeftParen,
    RightParen,
    LeftBrace,
    RightBrace,
    Semicolon,
    Comma,
    Arrow,
    Colon,

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
            "return" => Some(Token::Return),
            "true" => Some(Token::True),
            "false" => Some(Token::False),
            "print" => Some(Token::Print),
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