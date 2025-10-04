mod token;
#[cfg(test)]
mod tests;

pub use token::{FloatType, Span, StringPart, Token, TokenWithSpan};

use plat_diags::{Diagnostic, DiagnosticError};

pub struct Lexer {
    input: Vec<char>,
    filename: String,
    current: usize,
    tokens: Vec<TokenWithSpan>,
}

impl Lexer {
    pub fn new(input: &str) -> Self {
        Self {
            input: input.chars().collect(),
            filename: "<unknown>".to_string(),
            current: 0,
            tokens: Vec::new(),
        }
    }

    pub fn with_filename(input: &str, filename: impl Into<String>) -> Self {
        Self {
            input: input.chars().collect(),
            filename: filename.into(),
            current: 0,
            tokens: Vec::new(),
        }
    }

    pub fn tokenize(mut self) -> Result<Vec<TokenWithSpan>, DiagnosticError> {
        while !self.is_at_end() {
            self.skip_whitespace();
            if self.is_at_end() {
                break;
            }

            let start = self.current;

            match self.advance() {
                '+' => self.add_token(Token::Plus, start),
                '-' => {
                    if self.peek() == Some('>') {
                        self.advance();
                        self.add_token(Token::Arrow, start);
                    } else {
                        self.add_token(Token::Minus, start);
                    }
                }
                '*' => self.add_token(Token::Star, start),
                '/' => {
                    if self.peek() == Some('/') {
                        self.skip_line_comment();
                    } else {
                        self.add_token(Token::Slash, start);
                    }
                }
                '%' => self.add_token(Token::Percent, start),
                '(' => self.add_token(Token::LeftParen, start),
                ')' => self.add_token(Token::RightParen, start),
                '{' => self.add_token(Token::LeftBrace, start),
                '}' => self.add_token(Token::RightBrace, start),
                '[' => self.add_token(Token::LeftBracket, start),
                ']' => self.add_token(Token::RightBracket, start),
                ';' => self.add_token(Token::Semicolon, start),
                ',' => self.add_token(Token::Comma, start),
                ':' => {
                    if self.peek() == Some(':') {
                        self.advance();
                        self.add_token(Token::DoubleColon, start);
                    } else {
                        self.add_token(Token::Colon, start);
                    }
                }
                '.' => {
                    if self.peek() == Some('.') {
                        self.advance();
                        if self.peek() == Some('=') {
                            self.advance();
                            self.add_token(Token::DotDotEq, start);
                        } else {
                            self.add_token(Token::DotDot, start);
                        }
                    } else {
                        self.add_token(Token::Dot, start);
                    }
                }
                '=' => {
                    if self.peek() == Some('=') {
                        self.advance();
                        self.add_token(Token::Eq, start);
                    } else {
                        self.add_token(Token::Assign, start);
                    }
                }
                '!' => {
                    if self.peek() == Some('=') {
                        self.advance();
                        self.add_token(Token::NotEq, start);
                    } else {
                        return Err(DiagnosticError::Rich(
                            Diagnostic::syntax_error(
                                &self.filename,
                                Span::new(start, start + 1),
                                "Unexpected character '!'"
                            )
                            .with_label("unexpected character")
                            .with_help("Did you mean '!=' for not equal?")
                        ));
                    }
                }
                '<' => {
                    if self.peek() == Some('=') {
                        self.advance();
                        self.add_token(Token::LessEq, start);
                    } else {
                        self.add_token(Token::Less, start);
                    }
                }
                '>' => {
                    if self.peek() == Some('=') {
                        self.advance();
                        self.add_token(Token::GreaterEq, start);
                    } else {
                        self.add_token(Token::Greater, start);
                    }
                }
                '?' => self.add_token(Token::Question, start),
                '"' => self.scan_string(start)?,
                c if c.is_ascii_digit() => self.scan_number(start)?,
                c if c.is_ascii_alphabetic() || c == '_' => self.scan_identifier(start)?,
                c => {
                    return Err(DiagnosticError::Rich(
                        Diagnostic::syntax_error(
                            &self.filename,
                            Span::new(start, start + c.len_utf8()),
                            format!("Unexpected character '{}'", c)
                        )
                        .with_label("unexpected character")
                        .with_help("Remove this character or check your syntax")
                    ));
                }
            }
        }

        self.add_token(Token::Eof, self.current);
        Ok(self.tokens)
    }

    fn advance(&mut self) -> char {
        let c = self.input[self.current];
        self.current += 1;
        c
    }

    fn peek(&self) -> Option<char> {
        if self.is_at_end() {
            None
        } else {
            Some(self.input[self.current])
        }
    }

    fn peek_next(&self) -> Option<char> {
        if self.current + 1 >= self.input.len() {
            None
        } else {
            Some(self.input[self.current + 1])
        }
    }

    fn is_at_end(&self) -> bool {
        self.current >= self.input.len()
    }

    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn skip_line_comment(&mut self) {
        while self.peek() != Some('\n') && !self.is_at_end() {
            self.advance();
        }
    }

    fn scan_string(&mut self, start: usize) -> Result<(), DiagnosticError> {
        let mut parts = Vec::new();
        let mut current_text = String::new();

        while self.peek() != Some('"') && !self.is_at_end() {
            if self.peek() == Some('$') && self.peek_next() == Some('{') {
                // Save any text before interpolation
                if !current_text.is_empty() {
                    parts.push(StringPart::Text(current_text.clone()));
                    current_text.clear();
                }

                // Skip ${
                self.advance();
                self.advance();

                // Scan interpolation expression
                let mut expr = String::new();
                let mut depth = 1;

                while !self.is_at_end() && depth > 0 {
                    let c = self.advance();
                    if c == '{' {
                        depth += 1;
                        expr.push(c);
                    } else if c == '}' {
                        depth -= 1;
                        if depth > 0 {
                            expr.push(c);
                        }
                    } else {
                        expr.push(c);
                    }
                }

                if depth != 0 {
                    return Err(DiagnosticError::Rich(
                        Diagnostic::syntax_error(
                            &self.filename,
                            Span::new(start, self.current),
                            "Unclosed string interpolation"
                        )
                        .with_label("unterminated interpolation expression")
                        .with_help("Add a closing '}' to complete the interpolation")
                    ));
                }

                parts.push(StringPart::Interpolation(expr));
            } else if self.peek() == Some('\\') {
                self.advance();
                if let Some(escaped) = self.peek() {
                    self.advance();
                    current_text.push(match escaped {
                        'n' => '\n',
                        't' => '\t',
                        'r' => '\r',
                        '\\' => '\\',
                        '"' => '"',
                        c => c,
                    });
                }
            } else {
                current_text.push(self.advance());
            }
        }

        if self.is_at_end() {
            return Err(DiagnosticError::Rich(
                Diagnostic::syntax_error(
                    &self.filename,
                    Span::new(start, self.current),
                    "Unterminated string literal"
                )
                .with_label("string started here but never closed")
                .with_help("Add a closing \" to complete the string")
            ));
        }

        // Skip closing "
        self.advance();

        // Add any remaining text
        if !current_text.is_empty() {
            parts.push(StringPart::Text(current_text));
        }

        let token = if parts.is_empty() {
            Token::StringLiteral(String::new())
        } else if parts.len() == 1 {
            match &parts[0] {
                StringPart::Text(s) => Token::StringLiteral(s.clone()),
                _ => Token::InterpolatedString(parts),
            }
        } else {
            Token::InterpolatedString(parts)
        };

        self.add_token(token, start);
        Ok(())
    }

    fn scan_number(&mut self, start: usize) -> Result<(), DiagnosticError> {
        // Scan integer part
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() || c == '_' {
                self.advance();
            } else {
                break;
            }
        }

        // Check for decimal point (float)
        let is_float = if self.peek() == Some('.') {
            // Make sure it's not a range operator (..)
            if let Some(next) = self.peek_next() {
                if next.is_ascii_digit() {
                    self.advance(); // consume '.'
                    // Scan fractional part
                    while let Some(c) = self.peek() {
                        if c.is_ascii_digit() || c == '_' {
                            self.advance();
                        } else {
                            break;
                        }
                    }
                    true
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        };

        // Check for scientific notation (e.g., 1.5e10, 2.3e-5)
        let has_exponent = if self.peek() == Some('e') || self.peek() == Some('E') {
            self.advance(); // consume 'e' or 'E'

            // Optional sign
            if self.peek() == Some('+') || self.peek() == Some('-') {
                self.advance();
            }

            // Must have at least one digit after 'e'
            if !self.peek().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                return Err(DiagnosticError::Rich(
                    Diagnostic::syntax_error(
                        &self.filename,
                        Span::new(start, self.current),
                        "Invalid scientific notation"
                    )
                    .with_label("expected digit after 'e' or 'E'")
                    .with_help("Scientific notation requires digits after the exponent marker (e.g., 1.5e10)")
                ));
            }

            while let Some(c) = self.peek() {
                if c.is_ascii_digit() || c == '_' {
                    self.advance();
                } else {
                    break;
                }
            }
            true
        } else {
            false
        };

        let num_str: String = self.input[start..self.current]
            .iter()
            .filter(|&c| *c != '_')
            .collect();

        // Check for suffix (f32, f64, i32, i64)
        let suffix = if self.peek() == Some('f') || self.peek() == Some('i') {
            let suffix_start = self.current;
            self.advance();

            // Read the suffix digits
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() {
                    self.advance();
                } else {
                    break;
                }
            }

            let suffix_str: String = self.input[suffix_start..self.current].iter().collect();
            Some(suffix_str)
        } else {
            None
        };

        // Determine if this is a float or int based on presence of decimal point or exponent
        let is_float_literal = is_float || has_exponent || matches!(suffix.as_deref(), Some("f32") | Some("f64"));

        if is_float_literal {
            // Parse as float
            let float_value = num_str.parse::<f64>()
                .map_err(|_| DiagnosticError::Rich(
                    Diagnostic::syntax_error(
                        &self.filename,
                        Span::new(start, self.current),
                        "Invalid float literal"
                    )
                    .with_label("cannot parse as floating point number")
                    .with_help("Check the number format (e.g., 3.14, 1.5e10)")
                ))?;

            let float_type = match suffix.as_deref() {
                Some("f32") => token::FloatType::F32,
                Some("f64") | None => token::FloatType::F64, // Default to f64
                Some(s) => {
                    return Err(DiagnosticError::Rich(
                        Diagnostic::syntax_error(
                            &self.filename,
                            Span::new(start, self.current),
                            format!("Invalid float suffix '{}'", s)
                        )
                        .with_label("invalid suffix")
                        .with_help("Valid suffixes are 'f32' and 'f64'")
                    ));
                }
            };

            self.add_token(Token::FloatLiteral(float_value, float_type), start);
        } else {
            // Parse as integer
            let value = num_str.parse::<i64>()
                .map_err(|_| DiagnosticError::Rich(
                    Diagnostic::syntax_error(
                        &self.filename,
                        Span::new(start, self.current),
                        "Invalid integer literal"
                    )
                    .with_label("cannot parse as integer")
                    .with_help("Check the number format and ensure it fits in the integer range")
                ))?;

            // Validate suffix if present
            if let Some(suffix) = suffix {
                if suffix != "i32" && suffix != "i64" {
                    return Err(DiagnosticError::Rich(
                        Diagnostic::syntax_error(
                            &self.filename,
                            Span::new(start, self.current),
                            format!("Invalid integer suffix '{}'", suffix)
                        )
                        .with_label("invalid suffix")
                        .with_help("Valid suffixes are 'i32' and 'i64'")
                    ));
                }
            }

            self.add_token(Token::IntLiteral(value), start);
        }

        Ok(())
    }

    fn scan_identifier(&mut self, start: usize) -> Result<(), DiagnosticError> {
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == '_' {
                self.advance();
            } else {
                break;
            }
        }

        let text: String = self.input[start..self.current].iter().collect();

        let token = Token::keyword_from_str(&text)
            .unwrap_or_else(|| Token::Ident(text));

        self.add_token(token, start);
        Ok(())
    }

    fn add_token(&mut self, token: Token, start: usize) {
        let span = Span::new(start, self.current);
        self.tokens.push(TokenWithSpan { token, span });
    }
}