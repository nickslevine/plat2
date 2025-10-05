#[cfg(test)]
mod tests;

use plat_ast::*;
use plat_diags::{Diagnostic, DiagnosticError};
use plat_lexer::{Lexer, Span, StringPart, Token, TokenWithSpan};

pub struct Parser {
    tokens: Vec<TokenWithSpan>,
    current: usize,
    filename: String,
}

impl Parser {
    pub fn new(input: &str) -> Result<Self, DiagnosticError> {
        let lexer = Lexer::new(input);
        let tokens = lexer.tokenize()?;
        Ok(Self {
            tokens,
            current: 0,
            filename: "<unknown>".to_string(),
        })
    }

    pub fn with_filename(input: &str, filename: impl Into<String>) -> Result<Self, DiagnosticError> {
        let filename = filename.into();
        let lexer = Lexer::with_filename(input, filename.clone());
        let tokens = lexer.tokenize()?;
        Ok(Self {
            tokens,
            current: 0,
            filename,
        })
    }

    pub fn parse(mut self) -> Result<Program, DiagnosticError> {
        // Parse optional module declaration
        let module_decl = if self.check(&Token::Mod) {
            Some(self.parse_module_decl()?)
        } else {
            None
        };

        // Parse use declarations
        let mut use_decls = Vec::new();
        while self.check(&Token::Use) {
            use_decls.push(self.parse_use_decl()?);
        }

        // Parse type aliases
        let mut type_aliases = Vec::new();
        while self.check(&Token::Pub) || self.check(&Token::Type) {
            let is_public = self.match_token(&Token::Pub);
            if !self.check(&Token::Type) {
                break; // pub followed by something other than type
            }
            type_aliases.push(self.parse_type_alias(is_public)?);
        }

        // Parse newtypes
        let mut newtypes = Vec::new();
        while self.check(&Token::Pub) || self.check(&Token::Newtype) {
            let is_public = self.match_token(&Token::Pub);
            if !self.check(&Token::Newtype) {
                break; // pub followed by something other than newtype
            }
            newtypes.push(self.parse_newtype(is_public)?);
        }

        let mut test_blocks = Vec::new();
        let mut bench_blocks = Vec::new();
        let mut functions = Vec::new();
        let mut enums = Vec::new();
        let mut classes = Vec::new();

        while !self.is_at_end() {
            // Check for optional 'pub' keyword
            let is_public = self.match_token(&Token::Pub);

            if self.check(&Token::Enum) {
                enums.push(self.parse_enum(is_public)?);
            } else if self.check(&Token::Class) {
                classes.push(self.parse_class(is_public)?);
            } else if self.check(&Token::Type) {
                type_aliases.push(self.parse_type_alias(is_public)?);
            } else if self.check(&Token::Newtype) {
                newtypes.push(self.parse_newtype(is_public)?);
            } else if self.check(&Token::Test) {
                if is_public {
                    return Err(DiagnosticError::Rich(
                        Diagnostic::syntax_error(
                            &self.filename,
                            self.previous_span(),
                            "Test blocks cannot be marked as public"
                        )
                        .with_label("'pub' keyword not allowed here")
                        .with_help("Remove the 'pub' keyword - test blocks are not exported")
                    ));
                }
                test_blocks.push(self.parse_test_block()?);
            } else if self.check(&Token::Bench) {
                if is_public {
                    return Err(DiagnosticError::Rich(
                        Diagnostic::syntax_error(
                            &self.filename,
                            self.previous_span(),
                            "Benchmark blocks cannot be marked as public"
                        )
                        .with_label("'pub' keyword not allowed here")
                        .with_help("Remove the 'pub' keyword - benchmark blocks are not exported")
                    ));
                }
                bench_blocks.push(self.parse_bench_block()?);
            } else {
                functions.push(self.parse_function(is_public)?);
            }
        }

        Ok(Program { module_decl, use_decls, type_aliases, newtypes, test_blocks, bench_blocks, functions, enums, classes })
    }

    fn parse_module_decl(&mut self) -> Result<ModuleDecl, DiagnosticError> {
        let start = self.current_span().start;
        self.consume(Token::Mod, "Expected 'mod'")?;

        let mut path = Vec::new();
        path.push(self.consume_identifier("Expected module name")?);

        // Parse nested module path (database::connection)
        while self.match_token(&Token::DoubleColon) {
            path.push(self.consume_identifier("Expected module name after '::'")?);
        }

        self.consume(Token::Semicolon, "Expected ';' after module declaration")?;
        let end = self.previous_span().end;

        Ok(ModuleDecl {
            path,
            span: Span::new(start, end),
        })
    }

    fn parse_use_decl(&mut self) -> Result<UseDecl, DiagnosticError> {
        let start = self.current_span().start;
        self.consume(Token::Use, "Expected 'use'")?;

        let mut path = Vec::new();
        path.push(self.consume_identifier("Expected module name")?);

        // Parse nested module path (database::connection)
        while self.match_token(&Token::DoubleColon) {
            path.push(self.consume_identifier("Expected module name after '::'")?);
        }

        self.consume(Token::Semicolon, "Expected ';' after use declaration")?;
        let end = self.previous_span().end;

        Ok(UseDecl {
            path,
            span: Span::new(start, end),
        })
    }

    fn parse_type_alias(&mut self, is_public: bool) -> Result<TypeAlias, DiagnosticError> {
        let start = self.current_span().start;
        self.consume(Token::Type, "Expected 'type'")?;

        let name = self.consume_identifier("Expected type alias name")?;

        self.consume(Token::Assign, "Expected '=' after type alias name")?;

        let ty = self.parse_type()?;

        self.consume(Token::Semicolon, "Expected ';' after type alias")?;
        let end = self.previous_span().end;

        Ok(TypeAlias {
            name,
            ty,
            is_public,
            span: Span::new(start, end),
        })
    }

    fn parse_newtype(&mut self, is_public: bool) -> Result<NewtypeDecl, DiagnosticError> {
        let start = self.current_span().start;
        self.consume(Token::Newtype, "Expected 'newtype'")?;

        let name = self.consume_identifier("Expected newtype name")?;

        self.consume(Token::Assign, "Expected '=' after newtype name")?;

        let underlying_type = self.parse_type()?;

        self.consume(Token::Semicolon, "Expected ';' after newtype declaration")?;
        let end = self.previous_span().end;

        Ok(NewtypeDecl {
            name,
            underlying_type,
            is_public,
            span: Span::new(start, end),
        })
    }

    fn parse_test_block(&mut self) -> Result<TestBlock, DiagnosticError> {
        let start = self.current_span().start;
        self.consume(Token::Test, "Expected 'test'")?;

        // Parse test block name (must be an identifier)
        let name = if let Token::Ident(s) = &self.peek().token {
            let name = s.clone();
            self.advance();
            name
        } else {
            return Err(DiagnosticError::Rich(
                Diagnostic::syntax_error(
                    &self.filename,
                    self.current_span(),
                    "Expected identifier for test block name"
                )
                .with_label("expected a snake_case identifier here")
                .with_help("Test blocks require a snake_case identifier: test my_test_block { ... }")
            ));
        };

        self.consume(Token::LeftBrace, "Expected '{' after test block name")?;

        // Parse all functions within the test block
        let mut functions = Vec::new();
        while !self.check(&Token::RightBrace) && !self.is_at_end() {
            functions.push(self.parse_function(false)?); // Test functions are not public
        }

        self.consume(Token::RightBrace, "Expected '}' after test block")?;
        let end = self.previous_span().end;

        Ok(TestBlock {
            name,
            functions,
            span: Span::new(start, end),
        })
    }

    fn parse_bench_block(&mut self) -> Result<BenchBlock, DiagnosticError> {
        let start = self.current_span().start;
        self.consume(Token::Bench, "Expected 'bench'")?;

        // Parse bench block name (must be an identifier)
        let name = if let Token::Ident(s) = &self.peek().token {
            let name = s.clone();
            self.advance();
            name
        } else {
            return Err(DiagnosticError::Rich(
                Diagnostic::syntax_error(
                    &self.filename,
                    self.current_span(),
                    "Expected identifier for bench block name"
                )
                .with_label("expected a snake_case identifier here")
                .with_help("Benchmark blocks require a snake_case identifier: bench my_bench_block { ... }")
            ));
        };

        self.consume(Token::LeftBrace, "Expected '{' after bench block name")?;

        // Parse all functions within the bench block
        let mut functions = Vec::new();
        while !self.check(&Token::RightBrace) && !self.is_at_end() {
            functions.push(self.parse_function(false)?); // Bench functions are not public
        }

        self.consume(Token::RightBrace, "Expected '}' after bench block")?;
        let end = self.previous_span().end;

        Ok(BenchBlock {
            name,
            functions,
            span: Span::new(start, end),
        })
    }

    fn parse_function(&mut self, is_public: bool) -> Result<Function, DiagnosticError> {
        let start = self.current_span().start;

        // Parse optional modifiers: virtual, override, mut
        let is_virtual = self.match_token(&Token::Virtual);
        let is_override = self.match_token(&Token::Override);
        let is_mutable = self.match_token(&Token::Mut);

        // Handle 'init' as a special function name, or regular 'fn'
        let name = if self.match_token(&Token::Init) {
            "init".to_string()
        } else {
            self.consume(Token::Fn, "Expected 'fn' or 'init'")?;
            self.consume_identifier("Expected function name")?
        };

        // Parse optional generic type parameters
        let mut type_params = Vec::new();
        if self.match_token(&Token::Less) {
            loop {
                type_params.push(self.consume_identifier("Expected type parameter name")?);
                if !self.match_token(&Token::Comma) {
                    break;
                }
            }
            self.consume(Token::Greater, "Expected '>' after type parameters")?;
        }

        self.consume(Token::LeftParen, "Expected '('")?;

        let params = self.parse_parameters()?;

        self.consume(Token::RightParen, "Expected ')'")?;

        let return_type = if self.match_token(&Token::Arrow) {
            Some(self.parse_type()?)
        } else {
            None
        };

        let body = self.parse_block()?;
        let end = body.span.end;

        Ok(Function {
            name,
            type_params,
            params,
            return_type,
            body,
            is_mutable,
            is_virtual,
            is_override,
            is_public,
            span: Span::new(start, end),
        })
    }

    fn parse_parameters(&mut self) -> Result<Vec<Parameter>, DiagnosticError> {
        let mut params = Vec::new();

        if !self.check(&Token::RightParen) {
            loop {
                let start = self.current_span().start;
                let name = self.consume_identifier("Expected parameter name")?;
                self.consume(Token::Colon, "Expected ':' after parameter name")?;
                let ty = self.parse_type()?;

                // Parse optional default value
                let default_value = if self.match_token(&Token::Assign) {
                    Some(self.parse_expression()?)
                } else {
                    None
                };

                let end = self.previous_span().end;

                params.push(Parameter {
                    name,
                    ty,
                    default_value,
                    span: Span::new(start, end),
                });

                if !self.match_token(&Token::Comma) {
                    break;
                }
            }
        }

        Ok(params)
    }

    fn parse_type(&mut self) -> Result<Type, DiagnosticError> {
        if self.match_token(&Token::List) {
            self.consume(Token::LeftBracket, "Expected '[' after 'List'")?;
            let element_type = self.parse_type()?;
            self.consume(Token::RightBracket, "Expected ']' after element type")?;
            return Ok(Type::List(Box::new(element_type)));
        }

        if self.match_token(&Token::Dict) {
            self.consume(Token::LeftBracket, "Expected '[' after 'Dict'")?;
            let key_type = self.parse_type()?;
            self.consume(Token::Comma, "Expected ',' after key type")?;
            let value_type = self.parse_type()?;
            self.consume(Token::RightBracket, "Expected ']' after value type")?;
            return Ok(Type::Dict(Box::new(key_type), Box::new(value_type)));
        }

        if self.match_token(&Token::Set) {
            self.consume(Token::LeftBracket, "Expected '[' after 'Set'")?;
            let element_type = self.parse_type()?;
            self.consume(Token::RightBracket, "Expected ']' after element type")?;
            return Ok(Type::Set(Box::new(element_type)));
        }

        let type_name = self.consume_identifier("Expected type name")?;

        // Check for generic type parameters
        if self.match_token(&Token::Less) {
            let mut type_params = Vec::new();
            loop {
                type_params.push(self.parse_type()?);
                if !self.match_token(&Token::Comma) {
                    break;
                }
            }
            self.consume(Token::Greater, "Expected '>' after type parameters")?;
            return Ok(Type::Named(type_name, type_params));
        }

        match type_name.as_str() {
            "Bool" => Ok(Type::Bool),
            "Int8" => Ok(Type::Int8),
            "Int16" => Ok(Type::Int16),
            "Int32" => Ok(Type::Int32),
            "Int64" => Ok(Type::Int64),
            "Int" => Ok(Type::Int64), // Alias for Int64
            "Float8" => Ok(Type::Float8),
            "Float16" => Ok(Type::Float16),
            "Float32" => Ok(Type::Float32),
            "Float64" => Ok(Type::Float64),
            "Float" => Ok(Type::Float64), // Alias for Float64
            "String" => Ok(Type::String),
            _ => Ok(Type::Named(type_name, vec![])),
        }
    }

    fn parse_block(&mut self) -> Result<Block, DiagnosticError> {
        let start = self.current_span().start;
        self.consume(Token::LeftBrace, "Expected '{'")?;

        let mut statements = Vec::new();

        while !self.check(&Token::RightBrace) && !self.is_at_end() {
            statements.push(self.parse_statement()?);
        }

        self.consume(Token::RightBrace, "Expected '}'")?;
        let end = self.previous_span().end;

        Ok(Block {
            statements,
            span: Span::new(start, end),
        })
    }

    fn parse_statement(&mut self) -> Result<Statement, DiagnosticError> {
        if self.match_token(&Token::Let) {
            self.parse_let_statement()
        } else if self.match_token(&Token::Var) {
            self.parse_var_statement()
        } else if self.match_token(&Token::Return) {
            self.parse_return_statement()
        } else if self.match_token(&Token::If) {
            self.parse_if_statement()
        } else if self.match_token(&Token::While) {
            self.parse_while_statement()
        } else if self.match_token(&Token::For) {
            self.parse_for_statement()
        } else if self.match_token(&Token::Print) {
            self.parse_print_statement()
        } else if self.match_token(&Token::Concurrent) {
            self.parse_concurrent_statement()
        } else {
            let expr = self.parse_expression()?;
            self.consume(Token::Semicolon, "Expected ';' after expression")?;
            Ok(Statement::Expression(expr))
        }
    }

    fn parse_let_statement(&mut self) -> Result<Statement, DiagnosticError> {
        let start = self.previous_span().start;
        let name = self.consume_identifier("Expected variable name")?;

        self.consume(Token::Colon, "Expected ':' after variable name (type annotation required)")?;
        let ty = self.parse_type()?;

        self.consume(Token::Assign, "Expected '=' in let statement")?;
        let value = self.parse_expression()?;
        self.consume(Token::Semicolon, "Expected ';' after let statement")?;
        let end = self.previous_span().end;

        Ok(Statement::Let {
            name,
            ty,
            value,
            span: Span::new(start, end),
        })
    }

    fn parse_var_statement(&mut self) -> Result<Statement, DiagnosticError> {
        let start = self.previous_span().start;
        let name = self.consume_identifier("Expected variable name")?;

        self.consume(Token::Colon, "Expected ':' after variable name (type annotation required)")?;
        let ty = self.parse_type()?;

        self.consume(Token::Assign, "Expected '=' in var statement")?;
        let value = self.parse_expression()?;
        self.consume(Token::Semicolon, "Expected ';' after var statement")?;
        let end = self.previous_span().end;

        Ok(Statement::Var {
            name,
            ty,
            value,
            span: Span::new(start, end),
        })
    }

    fn parse_return_statement(&mut self) -> Result<Statement, DiagnosticError> {
        let start = self.previous_span().start;

        let value = if self.check(&Token::Semicolon) {
            None
        } else {
            Some(self.parse_expression()?)
        };

        self.consume(Token::Semicolon, "Expected ';' after return statement")?;
        let end = self.previous_span().end;

        Ok(Statement::Return {
            value,
            span: Span::new(start, end),
        })
    }

    fn parse_if_statement(&mut self) -> Result<Statement, DiagnosticError> {
        let start = self.previous_span().start;

        self.consume(Token::LeftParen, "Expected '(' after 'if'")?;
        let condition = self.parse_expression()?;
        self.consume(Token::RightParen, "Expected ')' after condition")?;

        let then_branch = self.parse_block()?;

        let else_branch = if self.match_token(&Token::Else) {
            Some(self.parse_block()?)
        } else {
            None
        };

        let end = else_branch.as_ref()
            .map(|b| b.span.end)
            .unwrap_or(then_branch.span.end);

        Ok(Statement::If {
            condition,
            then_branch,
            else_branch,
            span: Span::new(start, end),
        })
    }

    fn parse_while_statement(&mut self) -> Result<Statement, DiagnosticError> {
        let start = self.previous_span().start;

        self.consume(Token::LeftParen, "Expected '(' after 'while'")?;
        let condition = self.parse_expression()?;
        self.consume(Token::RightParen, "Expected ')' after condition")?;

        let body = self.parse_block()?;
        let end = body.span.end;

        Ok(Statement::While {
            condition,
            body,
            span: Span::new(start, end),
        })
    }

    fn parse_for_statement(&mut self) -> Result<Statement, DiagnosticError> {
        let start = self.previous_span().start;

        self.consume(Token::LeftParen, "Expected '(' after 'for'")?;
        let variable = self.consume_identifier("Expected variable name in for loop")?;
        self.consume(Token::Colon, "Expected ':' after for loop variable (type annotation required)")?;
        let variable_type = self.parse_type()?;
        self.consume(Token::In, "Expected 'in' after for loop variable type")?;
        let iterable = self.parse_expression()?;
        self.consume(Token::RightParen, "Expected ')' after for loop expression")?;

        let body = self.parse_block()?;
        let end = body.span.end;

        Ok(Statement::For {
            variable,
            variable_type,
            iterable,
            body,
            span: Span::new(start, end),
        })
    }

    fn parse_print_statement(&mut self) -> Result<Statement, DiagnosticError> {
        let start = self.previous_span().start;

        self.consume(Token::LeftParen, "Expected '(' after 'print'")?;

        // Expect named argument: value = expression
        let param_name = self.consume_identifier("Expected parameter name 'value'")?;
        if param_name != "value" {
            return Err(DiagnosticError::Rich(
                Diagnostic::syntax_error(
                    &self.filename,
                    self.previous_span(),
                    format!("Expected parameter name 'value', found '{}'", param_name)
                )
                .with_label("incorrect parameter name")
                .with_help("print() requires a named argument: print(value = ...)")
            ));
        }
        self.consume(Token::Assign, "Expected '=' after parameter name")?;

        let value = self.parse_expression()?;
        self.consume(Token::RightParen, "Expected ')' after print argument")?;
        self.consume(Token::Semicolon, "Expected ';' after print statement")?;
        let end = self.previous_span().end;

        Ok(Statement::Print {
            value,
            span: Span::new(start, end),
        })
    }

    fn parse_concurrent_statement(&mut self) -> Result<Statement, DiagnosticError> {
        let start = self.previous_span().start;
        let body = self.parse_block()?;
        let end = body.span.end;

        Ok(Statement::Concurrent {
            body,
            span: Span::new(start, end),
        })
    }

    fn parse_expression(&mut self) -> Result<Expression, DiagnosticError> {
        self.parse_assignment()
    }

    fn parse_assignment(&mut self) -> Result<Expression, DiagnosticError> {
        let expr = self.parse_logical_or()?;

        if self.match_token(&Token::Assign) {
            // Allow assignment to identifier or member access expressions
            match &expr {
                Expression::Identifier { .. } | Expression::MemberAccess { .. } => {
                    let value = Box::new(self.parse_assignment()?);
                    let end = self.previous_span().end;
                    let start = match &expr {
                        Expression::Identifier { span, .. } => span.start,
                        Expression::MemberAccess { span, .. } => span.start,
                        _ => unreachable!(),
                    };
                    return Ok(Expression::Assignment {
                        target: Box::new(expr),
                        value,
                        span: Span::new(start, end),
                    });
                }
                _ => {
                    let span = self.expr_span(&expr);
                    return Err(DiagnosticError::Rich(
                        Diagnostic::syntax_error(
                            &self.filename,
                            span,
                            "Invalid assignment target"
                        )
                        .with_label("cannot assign to this expression")
                        .with_help("Only identifiers (x = ...) and member access (obj.field = ...) can be assigned to")
                    ));
                }
            }
        }

        Ok(expr)
    }

    fn parse_logical_or(&mut self) -> Result<Expression, DiagnosticError> {
        let mut expr = self.parse_logical_and()?;

        while self.match_token(&Token::Or) {
            let op = BinaryOp::Or;
            let right = Box::new(self.parse_logical_and()?);
            let span = Span::new(
                match &expr {
                    Expression::Binary { span, .. } => span.start,
                    Expression::Unary { span, .. } => span.start,
                    Expression::Literal(lit) => match lit {
                        Literal::Bool(_, s) | Literal::Integer(_, s) | Literal::Float(_, _, s) |
                        Literal::String(_, s) | Literal::InterpolatedString(_, s) |
                        Literal::Array(_, s) | Literal::Dict(_, s) | Literal::Set(_, s) => s.start,
                    },
                    Expression::Identifier { span, .. } => span.start,
                    Expression::Call { span, .. } => span.start,
                    Expression::Assignment { span, .. } => span.start,
                    Expression::Index { span, .. } => span.start,
                    Expression::MethodCall { span, .. } => span.start,
                    Expression::Block(b) => b.span.start,
                    Expression::EnumConstructor { span, .. } => span.start,
                    Expression::Match { span, .. } => span.start,
                    Expression::Try { span, .. } => span.start,
                    Expression::Self_ { span, .. } => span.start,
                    Expression::MemberAccess { span, .. } => span.start,
                    Expression::ConstructorCall { span, .. } => span.start,
                    Expression::SuperCall { span, .. } => span.start,
                    Expression::Range { span, .. } => span.start,
                    Expression::If { span, .. } => span.start,
                    Expression::Cast { span, .. } => span.start,
                    Expression::Spawn { span, .. } => span.start,
                },
                self.previous_span().end,
            );
            expr = Expression::Binary {
                left: Box::new(expr),
                op,
                right,
                span,
            };
        }

        Ok(expr)
    }

    fn parse_logical_and(&mut self) -> Result<Expression, DiagnosticError> {
        let mut expr = self.parse_equality()?;

        while self.match_token(&Token::And) {
            let op = BinaryOp::And;
            let right = Box::new(self.parse_equality()?);
            let span = self.get_expression_span(&expr, self.previous_span().end);
            expr = Expression::Binary {
                left: Box::new(expr),
                op,
                right,
                span,
            };
        }

        Ok(expr)
    }

    fn parse_equality(&mut self) -> Result<Expression, DiagnosticError> {
        let mut expr = self.parse_comparison()?;

        while let Some(op) = self.match_tokens(&[Token::Eq, Token::NotEq]) {
            let op = match op {
                Token::Eq => BinaryOp::Equal,
                Token::NotEq => BinaryOp::NotEqual,
                _ => unreachable!(),
            };
            let right = Box::new(self.parse_comparison()?);
            let span = self.get_expression_span(&expr, self.previous_span().end);
            expr = Expression::Binary {
                left: Box::new(expr),
                op,
                right,
                span,
            };
        }

        Ok(expr)
    }

    fn parse_comparison(&mut self) -> Result<Expression, DiagnosticError> {
        let mut expr = self.parse_range()?;

        while let Some(op) = self.match_tokens(&[
            Token::Greater, Token::GreaterEq, Token::Less, Token::LessEq
        ]) {
            let op = match op {
                Token::Greater => BinaryOp::Greater,
                Token::GreaterEq => BinaryOp::GreaterEqual,
                Token::Less => BinaryOp::Less,
                Token::LessEq => BinaryOp::LessEqual,
                _ => unreachable!(),
            };
            let right = Box::new(self.parse_range()?);
            let span = self.get_expression_span(&expr, self.previous_span().end);
            expr = Expression::Binary {
                left: Box::new(expr),
                op,
                right,
                span,
            };
        }

        Ok(expr)
    }

    fn parse_range(&mut self) -> Result<Expression, DiagnosticError> {
        let start_expr = self.parse_term()?;

        // Check for range operators
        if let Some(token) = self.match_tokens(&[Token::DotDot, Token::DotDotEq]) {
            let inclusive = token == Token::DotDotEq;
            let end_expr = self.parse_term()?;
            let span = self.get_expression_span(&start_expr, self.previous_span().end);

            return Ok(Expression::Range {
                start: Box::new(start_expr),
                end: Box::new(end_expr),
                inclusive,
                span,
            });
        }

        Ok(start_expr)
    }

    fn parse_term(&mut self) -> Result<Expression, DiagnosticError> {
        let mut expr = self.parse_factor()?;

        while let Some(op) = self.match_tokens(&[Token::Plus, Token::Minus]) {
            let op = match op {
                Token::Plus => BinaryOp::Add,
                Token::Minus => BinaryOp::Subtract,
                _ => unreachable!(),
            };
            let right = Box::new(self.parse_factor()?);
            let span = self.get_expression_span(&expr, self.previous_span().end);
            expr = Expression::Binary {
                left: Box::new(expr),
                op,
                right,
                span,
            };
        }

        Ok(expr)
    }

    fn parse_factor(&mut self) -> Result<Expression, DiagnosticError> {
        let mut expr = self.parse_unary()?;

        while let Some(op) = self.match_tokens(&[Token::Star, Token::Slash, Token::Percent]) {
            let op = match op {
                Token::Star => BinaryOp::Multiply,
                Token::Slash => BinaryOp::Divide,
                Token::Percent => BinaryOp::Modulo,
                _ => unreachable!(),
            };
            let right = Box::new(self.parse_unary()?);
            let span = self.get_expression_span(&expr, self.previous_span().end);
            expr = Expression::Binary {
                left: Box::new(expr),
                op,
                right,
                span,
            };
        }

        Ok(expr)
    }

    fn parse_unary(&mut self) -> Result<Expression, DiagnosticError> {
        if let Some(op) = self.match_tokens(&[Token::Not, Token::Minus]) {
            let start = self.previous_span().start;
            let op = match op {
                Token::Not => UnaryOp::Not,
                Token::Minus => UnaryOp::Negate,
                _ => unreachable!(),
            };
            let operand = Box::new(self.parse_unary()?);
            let end = self.previous_span().end;
            return Ok(Expression::Unary {
                op,
                operand,
                span: Span::new(start, end),
            });
        }

        self.parse_call()
    }

    fn parse_call(&mut self) -> Result<Expression, DiagnosticError> {
        let mut expr = self.parse_primary()?;

        loop {
            if self.match_token(&Token::LeftParen) {
                if let Expression::Identifier { name, span } = expr {
                    // All calls now use named arguments
                    let args = self.parse_named_arguments()?;
                    self.consume(Token::RightParen, "Expected ')' after arguments")?;
                    let end = self.previous_span().end;

                    // Regular function call (constructors now use Type.init() syntax)
                    expr = Expression::Call {
                        function: name,
                        args,
                        span: Span::new(span.start, end),
                    };
                } else {
                    let span = self.expr_span(&expr);
                    return Err(DiagnosticError::Rich(
                        Diagnostic::syntax_error(
                            &self.filename,
                            span,
                            "Cannot call this expression"
                        )
                        .with_label("this is not a function")
                        .with_help("Only functions and methods can be called with ()")
                    ));
                }
            } else if self.match_token(&Token::LeftBracket) {
                let index = self.parse_expression()?;
                self.consume(Token::RightBracket, "Expected ']' after index")?;
                let end = self.previous_span().end;
                let start = self.get_expression_span(&expr, end).start;
                expr = Expression::Index {
                    object: Box::new(expr),
                    index: Box::new(index),
                    span: Span::new(start, end),
                };
            } else if self.match_token(&Token::Dot) {
                // Handle init keyword specially since it's a reserved token
                let member = if self.check(&Token::Init) {
                    self.advance();
                    "init".to_string()
                } else {
                    self.consume_identifier("Expected member name after '.'")?
                };

                if self.match_token(&Token::LeftParen) {
                    // Check if this is a constructor call (Type.init())
                    if member == "init" {
                        if let Expression::Identifier { ref name, span } = expr {
                            if name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                                // Constructor call: Type.init(args)
                                let class_name = name.clone();
                                let args = self.parse_named_arguments()?;
                                self.consume(Token::RightParen, "Expected ')' after constructor arguments")?;
                                let end = self.previous_span().end;
                                expr = Expression::ConstructorCall {
                                    class_name,
                                    args,
                                    span: Span::new(span.start, end),
                                };
                                continue;
                            }
                        }
                    }

                    // Regular method call - now with named arguments
                    let args = self.parse_named_arguments()?;
                    self.consume(Token::RightParen, "Expected ')' after method arguments")?;
                    let end = self.previous_span().end;
                    let start = self.get_expression_span(&expr, end).start;
                    expr = Expression::MethodCall {
                        object: Box::new(expr),
                        method: member,
                        args,
                        span: Span::new(start, end),
                    };
                } else {
                    // Member access
                    let end = self.previous_span().end;
                    let start = self.get_expression_span(&expr, end).start;
                    expr = Expression::MemberAccess {
                        object: Box::new(expr),
                        member,
                        span: Span::new(start, end),
                    };
                }
            } else if self.match_token(&Token::Question) {
                let end = self.previous_span().end;
                let start = self.get_expression_span(&expr, end).start;
                expr = Expression::Try {
                    expression: Box::new(expr),
                    span: Span::new(start, end),
                };
            } else {
                break;
            }
        }

        Ok(expr)
    }


    fn parse_primary(&mut self) -> Result<Expression, DiagnosticError> {
        if self.match_token(&Token::Match) {
            return self.parse_match_expression();
        }

        if self.match_token(&Token::If) {
            return self.parse_if_expression();
        }

        if self.match_token(&Token::Cast) {
            return self.parse_cast_expression();
        }

        if self.match_token(&Token::Spawn) {
            return self.parse_spawn_expression();
        }

        if self.match_token(&Token::True) {
            let span = self.previous_span();
            return Ok(Expression::Literal(Literal::Bool(true, span)));
        }

        if self.match_token(&Token::False) {
            let span = self.previous_span();
            return Ok(Expression::Literal(Literal::Bool(false, span)));
        }

        if let Some(Token::IntLiteral(n)) = self.match_if(|t| matches!(t, Token::IntLiteral(_))) {
            let span = self.previous_span();
            return Ok(Expression::Literal(Literal::Integer(n, span)));
        }

        if let Some(Token::FloatLiteral(value, float_type)) = self.match_if(|t| matches!(t, Token::FloatLiteral(_, _))) {
            let span = self.previous_span();
            let ast_float_type = match float_type {
                plat_lexer::FloatType::F32 => FloatType::F32,
                plat_lexer::FloatType::F64 => FloatType::F64,
            };
            return Ok(Expression::Literal(Literal::Float(value, ast_float_type, span)));
        }

        if let Some(Token::StringLiteral(s)) = self.match_if(|t| matches!(t, Token::StringLiteral(_))) {
            let span = self.previous_span();
            return Ok(Expression::Literal(Literal::String(s, span)));
        }

        if let Some(Token::InterpolatedString(parts)) = self.match_if(|t| matches!(t, Token::InterpolatedString(_))) {
            let span = self.previous_span();
            let interpolation_parts = self.parse_interpolated_string(parts)?;
            return Ok(Expression::Literal(Literal::InterpolatedString(interpolation_parts, span)));
        }

        if self.match_token(&Token::Self_) {
            let span = self.previous_span();
            return Ok(Expression::Self_ { span });
        }

        if self.match_token(&Token::Super) {
            let start = self.previous_span().start;
            self.consume(Token::Dot, "Expected '.' after 'super'")?;
            // Allow 'init' keyword as a method name in super calls
            let method = if self.match_token(&Token::Init) {
                "init".to_string()
            } else {
                self.consume_identifier("Expected method name after 'super.'")?
            };
            self.consume(Token::LeftParen, "Expected '(' after super method name")?;
            let args = self.parse_named_arguments()?;
            self.consume(Token::RightParen, "Expected ')' after super method arguments")?;
            let end = self.previous_span().end;
            return Ok(Expression::SuperCall {
                method,
                args,
                span: Span::new(start, end),
            });
        }

        if let Some(Token::Ident(name)) = self.match_if(|t| matches!(t, Token::Ident(_))) {
            let span = self.previous_span();
            // Check for qualified name (module::item or EnumName::Variant)
            if self.match_token(&Token::DoubleColon) {
                let second_part = self.consume_identifier("Expected identifier after '::'")?;

                // Use a simple heuristic: if the first part starts with uppercase, treat as enum
                // Otherwise, treat as qualified function/identifier (module::function)
                let is_likely_enum = name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false);

                // Check if this is an enum constructor with arguments
                if is_likely_enum && self.match_token(&Token::LeftParen) {
                    let args = self.parse_named_arguments()?;
                    self.consume(Token::RightParen, "Expected ')' after enum constructor arguments")?;
                    let end = self.previous_span().end;
                    return Ok(Expression::EnumConstructor {
                        enum_name: name,
                        variant: second_part,
                        args,
                        span: Span::new(span.start, end),
                    });
                } else if is_likely_enum && !self.check(&Token::LeftParen) {
                    // Enum variant without arguments
                    let end = self.previous_span().end;
                    return Ok(Expression::EnumConstructor {
                        enum_name: name,
                        variant: second_part,
                        args: vec![],
                        span: Span::new(span.start, end),
                    });
                }

                // Otherwise, it's a qualified identifier (module::function)
                let qualified_name = format!("{}::{}", name, second_part);
                let end = self.previous_span().end;
                return Ok(Expression::Identifier {
                    name: qualified_name,
                    span: Span::new(span.start, end),
                });
            }
            return Ok(Expression::Identifier { name, span });
        }

        if self.match_token(&Token::Set) {
            return self.parse_set_literal();
        }

        if self.match_token(&Token::LeftParen) {
            let expr = self.parse_expression()?;
            self.consume(Token::RightParen, "Expected ')' after expression")?;
            return Ok(expr);
        }

        if self.match_token(&Token::LeftBrace) {
            // Lookahead to determine if this is a dict literal or a block
            if self.is_dict_literal() {
                self.current -= 1; // Back up to re-parse the dict
                return self.parse_dict_literal();
            } else {
                self.current -= 1; // Back up to re-parse the block
                let block = self.parse_block()?;
                return Ok(Expression::Block(block));
            }
        }

        if self.match_token(&Token::LeftBracket) {
            let start = self.previous_span().start;
            let mut elements = Vec::new();

            if !self.check(&Token::RightBracket) {
                loop {
                    elements.push(self.parse_expression()?);
                    if !self.match_token(&Token::Comma) {
                        break;
                    }
                }
            }

            self.consume(Token::RightBracket, "Expected ']' after array elements")?;
            let end = self.previous_span().end;

            return Ok(Expression::Literal(Literal::Array(elements, Span::new(start, end))));
        }

        Err(DiagnosticError::Syntax("Expected expression".to_string()))
    }

    fn parse_interpolated_string(&mut self, parts: Vec<StringPart>) -> Result<Vec<InterpolationPart>, DiagnosticError> {
        let mut result = Vec::new();

        for part in parts {
            match part {
                StringPart::Text(text) => {
                    result.push(InterpolationPart::Text(text));
                }
                StringPart::Interpolation(expr_str) => {
                    // Parse the interpolation expression
                    let mut parser = Parser::new(&expr_str)?;
                    let expr = parser.parse_expression()?;
                    result.push(InterpolationPart::Expression(Box::new(expr)));
                }
            }
        }

        Ok(result)
    }

    fn get_expression_span(&self, expr: &Expression, end: usize) -> Span {
        let start = match expr {
            Expression::Binary { span, .. } => span.start,
            Expression::Unary { span, .. } => span.start,
            Expression::Literal(lit) => match lit {
                Literal::Bool(_, s) | Literal::Integer(_, s) | Literal::Float(_, _, s) |
                Literal::String(_, s) | Literal::InterpolatedString(_, s) |
                Literal::Array(_, s) | Literal::Dict(_, s) | Literal::Set(_, s) => s.start,
            },
            Expression::Identifier { span, .. } => span.start,
            Expression::Call { span, .. } => span.start,
            Expression::Assignment { span, .. } => span.start,
            Expression::Index { span, .. } => span.start,
            Expression::MethodCall { span, .. } => span.start,
            Expression::Block(b) => b.span.start,
            Expression::EnumConstructor { span, .. } => span.start,
            Expression::Match { span, .. } => span.start,
            Expression::Try { span, .. } => span.start,
            Expression::Self_ { span, .. } => span.start,
            Expression::MemberAccess { span, .. } => span.start,
            Expression::ConstructorCall { span, .. } => span.start,
            Expression::SuperCall { span, .. } => span.start,
            Expression::Range { span, .. } => span.start,
            Expression::If { span, .. } => span.start,
            Expression::Cast { span, .. } => span.start,
            Expression::Spawn { span, .. } => span.start,
        };
        Span::new(start, end)
    }

    fn parse_enum(&mut self, is_public: bool) -> Result<EnumDecl, DiagnosticError> {
        let start = self.current_span().start;
        self.consume(Token::Enum, "Expected 'enum'")?;

        let name = self.consume_identifier("Expected enum name")?;

        // Parse optional generic parameters
        let mut type_params = Vec::new();
        if self.match_token(&Token::Less) {
            loop {
                type_params.push(self.consume_identifier("Expected type parameter name")?);
                if !self.match_token(&Token::Comma) {
                    break;
                }
            }
            self.consume(Token::Greater, "Expected '>' after type parameters")?;
        }

        self.consume(Token::LeftBrace, "Expected '{' after enum name")?;

        let mut variants = Vec::new();
        let mut methods = Vec::new();

        while !self.check(&Token::RightBrace) && !self.is_at_end() {
            // Check if it's a method (fn or mut fn)
            if self.check(&Token::Fn) || (self.check(&Token::Mut) && self.peek_next() == Some(&Token::Fn)) {
                methods.push(self.parse_function(false)?); // Enum methods are not public by default
            } else {
                // It's a variant
                let variant_start = self.current_span().start;
                let variant_name = self.consume_identifier("Expected variant name")?;

                let mut fields = Vec::new();
                if self.match_token(&Token::LeftParen) {
                    if !self.check(&Token::RightParen) {
                        loop {
                            fields.push(self.parse_type()?);
                            if !self.match_token(&Token::Comma) {
                                break;
                            }
                        }
                    }
                    self.consume(Token::RightParen, "Expected ')' after variant fields")?;
                }

                let variant_end = self.previous_span().end;
                variants.push(EnumVariant {
                    name: variant_name,
                    fields,
                    span: Span::new(variant_start, variant_end),
                });

                // Consume optional comma
                self.match_token(&Token::Comma);
            }
        }

        self.consume(Token::RightBrace, "Expected '}' after enum body")?;
        let end = self.previous_span().end;

        Ok(EnumDecl {
            name,
            type_params,
            variants,
            methods,
            is_public,
            span: Span::new(start, end),
        })
    }

    fn parse_match_expression(&mut self) -> Result<Expression, DiagnosticError> {
        let start = self.previous_span().start;

        let value = Box::new(self.parse_assignment()?);

        self.consume(Token::LeftBrace, "Expected '{' after match value")?;

        let mut arms = Vec::new();

        while !self.check(&Token::RightBrace) && !self.is_at_end() {
            let arm_start = self.current_span().start;
            let pattern = self.parse_pattern()?;

            self.consume(Token::Arrow, "Expected '=>' after pattern")?;

            let body = self.parse_expression()?;

            let arm_end = self.previous_span().end;
            arms.push(MatchArm {
                pattern,
                body,
                span: Span::new(arm_start, arm_end),
            });

            // Consume optional comma
            self.match_token(&Token::Comma);
        }

        self.consume(Token::RightBrace, "Expected '}' after match arms")?;
        let end = self.previous_span().end;

        Ok(Expression::Match {
            value,
            arms,
            span: Span::new(start, end),
        })
    }

    fn parse_if_expression(&mut self) -> Result<Expression, DiagnosticError> {
        let start = self.previous_span().start;

        self.consume(Token::LeftParen, "Expected '(' after 'if'")?;
        let condition = Box::new(self.parse_expression()?);
        self.consume(Token::RightParen, "Expected ')' after condition")?;

        // For if-expressions, we require braces for the then branch
        self.consume(Token::LeftBrace, "Expected '{' for if-expression then branch")?;
        let then_branch = Box::new(self.parse_block_expression()?);

        let else_branch = if self.match_token(&Token::Else) {
            self.consume(Token::LeftBrace, "Expected '{' for if-expression else branch")?;
            Some(Box::new(self.parse_block_expression()?))
        } else {
            None
        };

        let end = else_branch.as_ref()
            .map(|b| self.get_expression_span(b, 0).end)
            .unwrap_or_else(|| self.get_expression_span(&then_branch, 0).end);

        Ok(Expression::If {
            condition,
            then_branch,
            else_branch,
            span: Span::new(start, end),
        })
    }

    fn parse_cast_expression(&mut self) -> Result<Expression, DiagnosticError> {
        let start = self.previous_span().start;

        self.consume(Token::LeftParen, "Expected '(' after 'cast'")?;

        // Parse first argument: value = <expr>
        let value_param = self.consume_identifier("Expected 'value' parameter")?;
        if value_param != "value" {
            return Err(DiagnosticError::Syntax(
                format!("Expected 'value' as first parameter, found '{}'", value_param)
            ));
        }
        self.consume(Token::Assign, "Expected '=' after 'value'")?;
        let value = Box::new(self.parse_expression()?);

        self.consume(Token::Comma, "Expected ',' after value expression")?;

        // Parse second argument: target = <type>
        let target_param = self.consume_identifier("Expected 'target' parameter")?;
        if target_param != "target" {
            return Err(DiagnosticError::Syntax(
                format!("Expected 'target' as second parameter, found '{}'", target_param)
            ));
        }
        self.consume(Token::Assign, "Expected '=' after 'target'")?;
        let target_type = self.parse_type()?;

        self.consume(Token::RightParen, "Expected ')' after cast arguments")?;
        let end = self.previous_span().end;

        Ok(Expression::Cast {
            value,
            target_type,
            span: Span::new(start, end),
        })
    }

    fn parse_spawn_expression(&mut self) -> Result<Expression, DiagnosticError> {
        let start = self.previous_span().start;
        let block = self.parse_block()?;
        let end = block.span.end;

        Ok(Expression::Spawn {
            body: Box::new(Expression::Block(block)),
            span: Span::new(start, end),
        })
    }

    fn parse_block_expression(&mut self) -> Result<Expression, DiagnosticError> {
        // Parse statements until we find the last expression or closing brace
        let start = self.previous_span().start;
        let mut statements = Vec::new();
        let mut final_expr = None;

        while !self.check(&Token::RightBrace) && !self.is_at_end() {
            // Check if this looks like the final expression (no semicolon)
            let checkpoint = self.current;

            // Try to parse as expression first
            if let Ok(expr) = self.parse_expression() {
                // If followed by semicolon, it's a statement
                if self.match_token(&Token::Semicolon) {
                    statements.push(Statement::Expression(expr));
                } else if self.check(&Token::RightBrace) {
                    // This is the final expression
                    final_expr = Some(expr);
                    break;
                } else {
                    // Not followed by semicolon or closing brace, might be error
                    // But first check if it's a control flow statement that doesn't need semicolon
                    self.current = checkpoint;
                    statements.push(self.parse_statement()?);
                }
            } else {
                // Failed to parse as expression, try as statement
                self.current = checkpoint;
                statements.push(self.parse_statement()?);
            }
        }

        self.consume(Token::RightBrace, "Expected '}' after block")?;
        let end = self.previous_span().end;

        // If there's a final expression, use it; otherwise, create a block
        if let Some(expr) = final_expr {
            if statements.is_empty() {
                Ok(expr)
            } else {
                // Need to wrap statements + final expr in a block
                statements.push(Statement::Expression(expr));
                Ok(Expression::Block(Block {
                    statements,
                    span: Span::new(start, end),
                }))
            }
        } else {
            Ok(Expression::Block(Block {
                statements,
                span: Span::new(start, end),
            }))
        }
    }

    fn parse_pattern(&mut self) -> Result<Pattern, DiagnosticError> {
        let start = self.current_span().start;

        // Check for literal patterns
        if self.check(&Token::True) || self.check(&Token::False) {
            let is_true = self.match_token(&Token::True);
            if !is_true {
                self.consume(Token::False, "Expected 'false'")?;
            }
            let span = self.previous_span();
            return Ok(Pattern::Literal(Literal::Bool(is_true, span)));
        }

        if let Some(Token::IntLiteral(n)) = self.match_if(|t| matches!(t, Token::IntLiteral(_))) {
            let span = self.previous_span();
            return Ok(Pattern::Literal(Literal::Integer(n, span)));
        }

        if let Some(Token::StringLiteral(s)) = self.match_if(|t| matches!(t, Token::StringLiteral(_))) {
            let span = self.previous_span();
            return Ok(Pattern::Literal(Literal::String(s, span)));
        }

        // Check for identifier/enum variant pattern
        if let Some(Token::Ident(name)) = self.match_if(|t| matches!(t, Token::Ident(_))) {
            // Check if it's an enum variant pattern
            if self.match_token(&Token::DoubleColon) {
                let variant = self.consume_identifier("Expected variant name after ':'")?;
                let mut bindings = Vec::new();

                if self.match_token(&Token::LeftParen) {
                    if !self.check(&Token::RightParen) {
                        loop {
                            let binding_name = self.consume_identifier("Expected binding name")?;
                            self.consume(Token::Colon, "Expected ':' after binding name (type annotation required)")?;
                            let binding_type = self.parse_type()?;
                            bindings.push((binding_name, binding_type));
                            if !self.match_token(&Token::Comma) {
                                break;
                            }
                        }
                    }
                    self.consume(Token::RightParen, "Expected ')' after pattern bindings")?;
                }

                let end = self.previous_span().end;
                return Ok(Pattern::EnumVariant {
                    enum_name: Some(name),
                    variant,
                    bindings,
                    span: Span::new(start, end),
                });
            }

            // Otherwise, could be a simple identifier pattern or a variant without enum prefix
            // Check if next token is '(' which means it's a variant with fields
            if self.match_token(&Token::LeftParen) {
                let mut bindings = Vec::new();

                if !self.check(&Token::RightParen) {
                    loop {
                        let binding_name = self.consume_identifier("Expected binding name")?;
                        self.consume(Token::Colon, "Expected ':' after binding name (type annotation required)")?;
                        let binding_type = self.parse_type()?;
                        bindings.push((binding_name, binding_type));
                        if !self.match_token(&Token::Comma) {
                            break;
                        }
                    }
                }
                self.consume(Token::RightParen, "Expected ')' after pattern bindings")?;

                let end = self.previous_span().end;
                return Ok(Pattern::EnumVariant {
                    enum_name: None,
                    variant: name,
                    bindings,
                    span: Span::new(start, end),
                });
            }

            // Just an identifier pattern (binding)
            let end = self.previous_span().end;
            return Ok(Pattern::Identifier {
                name,
                span: Span::new(start, end),
            });
        }

        Err(DiagnosticError::Syntax("Expected pattern".to_string()))
    }

    fn peek_next(&self) -> Option<&Token> {
        if self.current + 1 < self.tokens.len() {
            Some(&self.tokens[self.current + 1].token)
        } else {
            None
        }
    }

    // Helper methods

    fn match_token(&mut self, token: &Token) -> bool {
        if self.check(token) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn match_tokens(&mut self, tokens: &[Token]) -> Option<Token> {
        for token in tokens {
            if self.check(token) {
                let matched = self.peek().token.clone();
                self.advance();
                return Some(matched);
            }
        }
        None
    }

    fn match_if<F>(&mut self, predicate: F) -> Option<Token>
    where
        F: Fn(&Token) -> bool,
    {
        if !self.is_at_end() && predicate(&self.peek().token) {
            let token = self.peek().token.clone();
            self.advance();
            Some(token)
        } else {
            None
        }
    }

    fn check(&self, token: &Token) -> bool {
        if self.is_at_end() {
            false
        } else {
            std::mem::discriminant(&self.peek().token) == std::mem::discriminant(token)
        }
    }

    fn advance(&mut self) -> &TokenWithSpan {
        if !self.is_at_end() {
            self.current += 1;
        }
        self.previous()
    }

    fn is_at_end(&self) -> bool {
        matches!(self.peek().token, Token::Eof)
    }

    fn peek(&self) -> &TokenWithSpan {
        &self.tokens[self.current]
    }

    fn previous(&self) -> &TokenWithSpan {
        &self.tokens[self.current - 1]
    }

    fn current_span(&self) -> Span {
        self.peek().span
    }

    fn previous_span(&self) -> Span {
        self.previous().span
    }

    /// Extract span from any expression
    fn expr_span(&self, expr: &Expression) -> Span {
        match expr {
            Expression::Literal(lit) => match lit {
                Literal::Bool(_, span) => *span,
                Literal::Integer(_, span) => *span,
                Literal::Float(_, _, span) => *span,
                Literal::String(_, span) => *span,
                Literal::InterpolatedString(_, span) => *span,
                Literal::Array(_, span) => *span,
                Literal::Dict(_, span) => *span,
                Literal::Set(_, span) => *span,
            },
            Expression::Identifier { span, .. } => *span,
            Expression::Binary { span, .. } => *span,
            Expression::Unary { span, .. } => *span,
            Expression::Call { span, .. } => *span,
            Expression::Assignment { span, .. } => *span,
            Expression::Index { span, .. } => *span,
            Expression::MethodCall { span, .. } => *span,
            Expression::Block(block) => block.span,
            Expression::EnumConstructor { span, .. } => *span,
            Expression::Match { span, .. } => *span,
            Expression::Try { span, .. } => *span,
            Expression::MemberAccess { span, .. } => *span,
            Expression::If { span, .. } => *span,
            Expression::Cast { span, .. } => *span,
            Expression::Self_ { span, .. } => *span,
            Expression::ConstructorCall { span, .. } => *span,
            Expression::SuperCall { span, .. } => *span,
            Expression::Range { span, .. } => *span,
            Expression::Spawn { span, .. } => *span,
        }
    }

    fn consume(&mut self, token: Token, message: &str) -> Result<(), DiagnosticError> {
        if self.check(&token) {
            self.advance();
            Ok(())
        } else {
            let span = self.current_span();
            let current_token = &self.peek().token;

            // Create rich diagnostic with helpful message
            let help_message = match token {
                Token::Semicolon => "Add ';' after this statement",
                Token::RightParen => "Add ')' to close the parenthesis",
                Token::RightBrace => "Add '}' to close the block",
                Token::Colon => "Add ':' after the identifier",
                _ => "Check the syntax",
            };

            let error_code = match token {
                Token::Semicolon => "E001",
                Token::RightParen => "E002",
                Token::RightBrace => "E003",
                Token::Colon => "E004",
                _ => "E000",
            };

            Err(DiagnosticError::Rich(
                Diagnostic::syntax_error(&self.filename, span, message)
                    .with_code(error_code)
                    .with_label(format!("found {:?} here", current_token))
                    .with_help(help_message)
            ))
        }
    }

    fn consume_identifier(&mut self, message: &str) -> Result<String, DiagnosticError> {
        if let Token::Ident(name) = &self.peek().token {
            let name = name.clone();
            self.advance();
            Ok(name)
        } else {
            Err(DiagnosticError::Syntax(format!(
                "{} at position {}",
                message,
                self.current_span().start
            )))
        }
    }

    fn is_dict_literal(&mut self) -> bool {
        // Look ahead to see if this looks like a dict literal
        // Pattern: { "key": value, ... } or { key: value, ... }
        // Save current position for restoration
        let saved_current = self.current;

        // Skip the opening brace (we've already consumed it)
        // Look for pattern: (string|ident) : expr
        if self.is_at_end() || self.check(&Token::RightBrace) {
            // Empty braces could be either, assume it's a block
            self.current = saved_current;
            return false;
        }

        // Check if we have a key-like token followed by ':'
        let looks_like_dict = match &self.peek().token {
            Token::StringLiteral(_) => {
                self.advance();
                self.check(&Token::Colon)
            }
            Token::Ident(_) => {
                self.advance();
                self.check(&Token::Colon)
            }
            _ => false,
        };

        // Restore position
        self.current = saved_current;
        looks_like_dict
    }

    fn parse_dict_literal(&mut self) -> Result<Expression, DiagnosticError> {
        let start = self.current_span().start;
        self.consume(Token::LeftBrace, "Expected '{'")?;

        let mut pairs = Vec::new();

        if !self.check(&Token::RightBrace) {
            loop {
                // Parse key (can be string literal or identifier)
                let key = match &self.peek().token {
                    Token::StringLiteral(_) => self.parse_primary()?,
                    Token::Ident(_) => {
                        let name = self.consume_identifier("Expected key")?;
                        let span = self.previous_span();
                        Expression::Literal(Literal::String(name, span))
                    }
                    _ => return Err(DiagnosticError::Syntax("Expected string or identifier as dict key".to_string())),
                };

                self.consume(Token::Colon, "Expected ':' after dict key")?;
                let value = self.parse_expression()?;

                pairs.push((key, value));

                if !self.match_token(&Token::Comma) {
                    break;
                }
            }
        }

        self.consume(Token::RightBrace, "Expected '}' after dict elements")?;
        let end = self.previous_span().end;

        Ok(Expression::Literal(Literal::Dict(pairs, Span::new(start, end))))
    }

    fn parse_set_literal(&mut self) -> Result<Expression, DiagnosticError> {
        let start = self.previous_span().start;
        self.consume(Token::LeftBrace, "Expected '{' after 'Set'")?;

        let mut elements = Vec::new();

        if !self.check(&Token::RightBrace) {
            loop {
                elements.push(self.parse_expression()?);
                if !self.match_token(&Token::Comma) {
                    break;
                }
            }
        }

        self.consume(Token::RightBrace, "Expected '}' after set elements")?;
        let end = self.previous_span().end;

        Ok(Expression::Literal(Literal::Set(elements, Span::new(start, end))))
    }

    fn parse_class(&mut self, is_public: bool) -> Result<ClassDecl, DiagnosticError> {
        let start = self.current_span().start;
        self.consume(Token::Class, "Expected 'class'")?;

        let name = self.consume_identifier("Expected class name")?;

        // Parse optional generic parameters
        let mut type_params = Vec::new();
        if self.match_token(&Token::Less) {
            loop {
                type_params.push(self.consume_identifier("Expected type parameter name")?);
                if !self.match_token(&Token::Comma) {
                    break;
                }
            }
            self.consume(Token::Greater, "Expected '>' after type parameters")?;
        }

        // Parse optional inheritance
        let parent_class = if self.match_token(&Token::Colon) {
            Some(self.consume_identifier("Expected parent class name after ':'")?)
        } else {
            None
        };

        self.consume(Token::LeftBrace, "Expected '{' after class declaration")?;

        let mut fields = Vec::new();
        let mut methods = Vec::new();

        while !self.check(&Token::RightBrace) && !self.is_at_end() {
            // Check for optional 'pub' keyword for class members
            let member_is_public = self.match_token(&Token::Pub);

            // Check if it's a method (fn, init, virtual fn, override fn, mut fn, etc.)
            if self.check(&Token::Fn) || self.check(&Token::Init)
                || self.check(&Token::Virtual) || self.check(&Token::Override)
                || (self.check(&Token::Mut) && self.peek_next() == Some(&Token::Fn)) {
                methods.push(self.parse_function(member_is_public)?);
            } else if self.check(&Token::Let) || self.check(&Token::Var) {
                // It's a field declaration
                let field_start = self.current_span().start;
                let is_mutable = self.match_token(&Token::Var);
                if !is_mutable {
                    self.consume(Token::Let, "Expected 'let' or 'var'")?;
                }

                let field_name = self.consume_identifier("Expected field name")?;
                self.consume(Token::Colon, "Expected ':' after field name")?;
                let field_type = self.parse_type()?;
                self.consume(Token::Semicolon, "Expected ';' after field declaration")?;

                let field_end = self.previous_span().end;
                fields.push(FieldDecl {
                    name: field_name,
                    ty: field_type,
                    is_mutable,
                    is_public: member_is_public,
                    span: Span::new(field_start, field_end),
                });
            } else {
                return Err(DiagnosticError::Syntax(
                    "Expected field declaration ('let'/'var') or method declaration ('fn') in class body".to_string()
                ));
            }
        }

        self.consume(Token::RightBrace, "Expected '}' after class body")?;
        let end = self.previous_span().end;

        Ok(ClassDecl {
            name,
            type_params,
            parent_class,
            fields,
            methods,
            is_public,
            span: Span::new(start, end),
        })
    }

    fn parse_named_arguments(&mut self) -> Result<Vec<NamedArg>, DiagnosticError> {
        let mut args = Vec::new();

        if !self.check(&Token::RightParen) {
            loop {
                let start = self.current_span().start;
                let name = self.consume_identifier("Expected parameter name")?;
                self.consume(Token::Assign, "Expected '=' after parameter name")?;
                let value = self.parse_expression()?;
                let end = self.previous_span().end;

                args.push(NamedArg {
                    name,
                    value,
                    span: Span::new(start, end),
                });

                if !self.match_token(&Token::Comma) {
                    break;
                }
            }
        }

        Ok(args)
    }
}