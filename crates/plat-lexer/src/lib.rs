mod token;
#[cfg(test)]
mod tests;

pub use token::{Span, StringPart, Token, TokenWithSpan};

use plat_diags::DiagnosticError;

pub struct Lexer {
    input: Vec<char>,
    current: usize,
    tokens: Vec<TokenWithSpan>,
}

impl Lexer {
    pub fn new(input: &str) -> Self {
        Self {
            input: input.chars().collect(),
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
                '.' => self.add_token(Token::Dot, start),
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
                        return Err(DiagnosticError::Syntax(
                            format!("Unexpected character '!' at position {}", start)
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
                '"' => self.scan_string(start)?,
                c if c.is_ascii_digit() => self.scan_number(start)?,
                c if c.is_ascii_alphabetic() || c == '_' => self.scan_identifier(start)?,
                c => {
                    return Err(DiagnosticError::Syntax(
                        format!("Unexpected character '{}' at position {}", c, start)
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
                    return Err(DiagnosticError::Syntax(
                        format!("Unclosed string interpolation starting at position {}", start)
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
            return Err(DiagnosticError::Syntax(
                format!("Unterminated string starting at position {}", start)
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
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                self.advance();
            } else {
                break;
            }
        }

        let num_str: String = self.input[start..self.current].iter().collect();

        // Check for i32 or i64 suffix
        let (num_part, suffix) = if self.peek() == Some('i') {
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
            (num_str, Some(suffix_str))
        } else {
            (num_str, None)
        };

        let value = num_part.parse::<i64>()
            .map_err(|_| DiagnosticError::Syntax(
                format!("Invalid number literal at position {}", start)
            ))?;

        // Validate suffix if present
        if let Some(suffix) = suffix {
            if suffix != "i32" && suffix != "i64" {
                return Err(DiagnosticError::Syntax(
                    format!("Invalid number suffix '{}' at position {}", suffix, start)
                ));
            }
        }

        self.add_token(Token::IntLiteral(value), start);
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