#[cfg(test)]
mod tests {
    use crate::{Lexer, StringPart, Token};

    fn tokenize(input: &str) -> Vec<Token> {
        let lexer = Lexer::new(input);
        lexer.tokenize()
            .unwrap()
            .into_iter()
            .map(|t| t.token)
            .collect()
    }

    #[test]
    fn test_keywords() {
        let input = "fn let var if else while for return true false print and or not enum match mut";
        let tokens = tokenize(input);

        assert_eq!(tokens, vec![
            Token::Fn,
            Token::Let,
            Token::Var,
            Token::If,
            Token::Else,
            Token::While,
            Token::For,
            Token::Return,
            Token::True,
            Token::False,
            Token::Print,
            Token::And,
            Token::Or,
            Token::Not,
            Token::Enum,
            Token::Match,
            Token::Mut,
            Token::Eof,
        ]);
    }

    #[test]
    fn test_identifiers() {
        let input = "hello world_123 _underscore camelCase";
        let tokens = tokenize(input);

        assert_eq!(tokens, vec![
            Token::Ident("hello".to_string()),
            Token::Ident("world_123".to_string()),
            Token::Ident("_underscore".to_string()),
            Token::Ident("camelCase".to_string()),
            Token::Eof,
        ]);
    }

    #[test]
    fn test_numbers() {
        use crate::IntType;

        let input = "42 100 0 999i32 1234i64";
        let tokens = tokenize(input);

        assert_eq!(tokens, vec![
            Token::IntLiteral(42, IntType::I32),
            Token::IntLiteral(100, IntType::I32),
            Token::IntLiteral(0, IntType::I32),
            Token::IntLiteral(999, IntType::I32),
            Token::IntLiteral(1234, IntType::I64),
            Token::Eof,
        ]);
    }

    #[test]
    fn test_strings() {
        let input = r#""hello" "world" "hello\nworld" "tab\there""#;
        let tokens = tokenize(input);

        assert_eq!(tokens, vec![
            Token::StringLiteral("hello".to_string()),
            Token::StringLiteral("world".to_string()),
            Token::StringLiteral("hello\nworld".to_string()),
            Token::StringLiteral("tab\there".to_string()),
            Token::Eof,
        ]);
    }

    #[test]
    fn test_string_interpolation() {
        let input = r#""Hello ${name}!" "Value: ${x + y}" "Nested ${foo${bar}}""#;
        let tokens = tokenize(input);

        assert_eq!(tokens, vec![
            Token::InterpolatedString(vec![
                StringPart::Text("Hello ".to_string()),
                StringPart::Interpolation("name".to_string()),
                StringPart::Text("!".to_string()),
            ]),
            Token::InterpolatedString(vec![
                StringPart::Text("Value: ".to_string()),
                StringPart::Interpolation("x + y".to_string()),
            ]),
            Token::InterpolatedString(vec![
                StringPart::Text("Nested ".to_string()),
                StringPart::Interpolation("foo${bar}".to_string()),
            ]),
            Token::Eof,
        ]);
    }

    #[test]
    fn test_operators() {
        let input = "+ - * / % = == != < <= > >= and or not";
        let tokens = tokenize(input);

        assert_eq!(tokens, vec![
            Token::Plus,
            Token::Minus,
            Token::Star,
            Token::Slash,
            Token::Percent,
            Token::Assign,
            Token::Eq,
            Token::NotEq,
            Token::Less,
            Token::LessEq,
            Token::Greater,
            Token::GreaterEq,
            Token::And,
            Token::Or,
            Token::Not,
            Token::Eof,
        ]);
    }

    #[test]
    fn test_punctuation() {
        let input = "( ) { } ; , -> : ::";
        let tokens = tokenize(input);

        assert_eq!(tokens, vec![
            Token::LeftParen,
            Token::RightParen,
            Token::LeftBrace,
            Token::RightBrace,
            Token::Semicolon,
            Token::Comma,
            Token::Arrow,
            Token::Colon,
            Token::DoubleColon,
            Token::Eof,
        ]);
    }

    #[test]
    fn test_comments() {
        use crate::IntType;

        let input = "let x = 5; // this is a comment\nlet y = 10;";
        let tokens = tokenize(input);

        assert_eq!(tokens, vec![
            Token::Let,
            Token::Ident("x".to_string()),
            Token::Assign,
            Token::IntLiteral(5, IntType::I32),
            Token::Semicolon,
            Token::Let,
            Token::Ident("y".to_string()),
            Token::Assign,
            Token::IntLiteral(10, IntType::I32),
            Token::Semicolon,
            Token::Eof,
        ]);
    }

    #[test]
    fn test_complex_expression() {
        let input = "fn add(x: i32, y: i32) -> i32 { return x + y; }";
        let tokens = tokenize(input);

        assert_eq!(tokens, vec![
            Token::Fn,
            Token::Ident("add".to_string()),
            Token::LeftParen,
            Token::Ident("x".to_string()),
            Token::Colon,
            Token::Ident("i32".to_string()),
            Token::Comma,
            Token::Ident("y".to_string()),
            Token::Colon,
            Token::Ident("i32".to_string()),
            Token::RightParen,
            Token::Arrow,
            Token::Ident("i32".to_string()),
            Token::LeftBrace,
            Token::Return,
            Token::Ident("x".to_string()),
            Token::Plus,
            Token::Ident("y".to_string()),
            Token::Semicolon,
            Token::RightBrace,
            Token::Eof,
        ]);
    }

    #[test]
    fn test_error_unterminated_string() {
        let input = r#""hello world"#;
        let lexer = Lexer::new(input);
        let result = lexer.tokenize();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unterminated string"));
    }

    #[test]
    fn test_error_invalid_character() {
        let input = "let x = @";
        let lexer = Lexer::new(input);
        let result = lexer.tokenize();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unexpected character"));
    }

    #[test]
    fn test_whitespace_handling() {
        use crate::IntType;

        let input = "  let  \n\t x   =\r\n  5  ";
        let tokens = tokenize(input);

        assert_eq!(tokens, vec![
            Token::Let,
            Token::Ident("x".to_string()),
            Token::Assign,
            Token::IntLiteral(5, IntType::I32),
            Token::Eof,
        ]);
    }

    #[test]
    fn test_empty_input() {
        let input = "";
        let tokens = tokenize(input);
        assert_eq!(tokens, vec![Token::Eof]);

        let input = "   \n\t  ";
        let tokens = tokenize(input);
        assert_eq!(tokens, vec![Token::Eof]);
    }

    #[test]
    fn test_range_operators() {
        use crate::IntType;

        let input = "0..5";
        let tokens = tokenize(input);
        assert_eq!(tokens, vec![
            Token::IntLiteral(0, IntType::I32),
            Token::DotDot,
            Token::IntLiteral(5, IntType::I32),
            Token::Eof,
        ]);

        let input = "0..=10";
        let tokens = tokenize(input);
        assert_eq!(tokens, vec![
            Token::IntLiteral(0, IntType::I32),
            Token::DotDotEq,
            Token::IntLiteral(10, IntType::I32),
            Token::Eof,
        ]);

        // Ensure regular dot still works
        let input = "x.len()";
        let tokens = tokenize(input);
        assert_eq!(tokens, vec![
            Token::Ident("x".to_string()),
            Token::Dot,
            Token::Ident("len".to_string()),
            Token::LeftParen,
            Token::RightParen,
            Token::Eof,
        ]);
    }

    #[test]
    fn test_numbers_with_underscores() {
        use crate::IntType;

        let input = "1_000_000 2_000i32 3_500_000i64";
        let tokens = tokenize(input);
        assert_eq!(tokens, vec![
            Token::IntLiteral(1_000_000, IntType::I32),
            Token::IntLiteral(2_000, IntType::I32),
            Token::IntLiteral(3_500_000, IntType::I64),
            Token::Eof,
        ]);
    }

    #[test]
    fn test_floats_with_underscores() {
        use crate::FloatType;

        let input = "2_000_000.00 1_234.567_89 9_999.99f32";
        let tokens = tokenize(input);
        assert_eq!(tokens, vec![
            Token::FloatLiteral(2_000_000.00, FloatType::F64),
            Token::FloatLiteral(1_234.56789, FloatType::F64),
            Token::FloatLiteral(9_999.99, FloatType::F32),
            Token::Eof,
        ]);
    }
}