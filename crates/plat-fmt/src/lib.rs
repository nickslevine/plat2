#[cfg(test)]
mod tests;

use plat_ast::*;
use plat_parser::Parser;
use plat_diags::DiagnosticError;

pub struct Formatter {
    indent: usize,
    buffer: String,
}

impl Formatter {
    pub fn format(input: &str) -> Result<String, DiagnosticError> {
        let parser = Parser::new(input)?;
        let program = parser.parse()?;

        let mut formatter = Self {
            indent: 0,
            buffer: String::new(),
        };

        formatter.format_program(&program);
        Ok(formatter.buffer)
    }

    fn format_program(&mut self, program: &Program) {
        for (i, function) in program.functions.iter().enumerate() {
            if i > 0 {
                self.write_line("");
            }
            self.format_function(function);
            self.write_line("");
        }
    }

    fn format_function(&mut self, function: &Function) {
        self.write("fn ");
        self.write(&function.name);
        self.write("(");

        for (i, param) in function.params.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            self.format_parameter(param);
        }

        self.write(")");

        if let Some(return_type) = &function.return_type {
            self.write(" -> ");
            self.format_type(return_type);
        }

        self.write(" ");
        self.format_function_block(&function.body);
    }

    fn format_parameter(&mut self, param: &Parameter) {
        self.write(&param.name);
        self.write(": ");
        self.format_type(&param.ty);
    }

    fn format_type(&mut self, ty: &Type) {
        match ty {
            Type::Bool => self.write("bool"),
            Type::I32 => self.write("i32"),
            Type::I64 => self.write("i64"),
            Type::String => self.write("string"),
            Type::List(element_type) => {
                self.write("List[");
                self.format_type(element_type);
                self.write("]");
            }
        }
    }

    fn format_function_block(&mut self, block: &Block) {
        self.write_line("{");
        self.indent += 1;

        for statement in &block.statements {
            self.format_statement(statement);
        }

        self.indent -= 1;
        self.write_indent();
        self.write("}");
    }

    fn format_if_block(&mut self, block: &Block) {
        self.write_line("{");
        self.indent += 1;

        for statement in &block.statements {
            self.format_statement(statement);
        }

        self.indent -= 1;
        self.write_indent();
        self.write("}");
    }


    fn format_statement(&mut self, statement: &Statement) {
        self.write_indent();

        match statement {
            Statement::Let { name, ty, value, .. } => {
                self.write("let ");
                self.write(name);
                if let Some(ty) = ty {
                    self.write(": ");
                    self.format_type(ty);
                }
                self.write(" = ");
                self.format_expression(value);
                self.write_line(";");
            }
            Statement::Var { name, ty, value, .. } => {
                self.write("var ");
                self.write(name);
                if let Some(ty) = ty {
                    self.write(": ");
                    self.format_type(ty);
                }
                self.write(" = ");
                self.format_expression(value);
                self.write_line(";");
            }
            Statement::Expression(expr) => {
                self.format_expression(expr);
                self.write_line(";");
            }
            Statement::Return { value, .. } => {
                self.write("return");
                if let Some(value) = value {
                    self.write(" ");
                    self.format_expression(value);
                }
                self.write_line(";");
            }
            Statement::If { condition, then_branch, else_branch, .. } => {
                self.write("if (");
                self.format_expression(condition);
                self.write(") ");
                self.format_if_block(then_branch);

                if let Some(else_branch) = else_branch {
                    self.write(" else ");
                    self.format_if_block(else_branch);
                }
            }
            Statement::While { condition, body, .. } => {
                self.write("while (");
                self.format_expression(condition);
                self.write(") ");
                self.format_if_block(body);
            }
            Statement::Print { value, .. } => {
                self.write("print(");
                self.format_expression(value);
                self.write_line(");");
            }
        }
    }

    fn format_expression(&mut self, expression: &Expression) {
        match expression {
            Expression::Literal(literal) => self.format_literal(literal),
            Expression::Identifier { name, .. } => self.write(name),
            Expression::Binary { left, op, right, .. } => {
                self.format_expression(left);
                self.write(" ");
                self.format_binary_op(op);
                self.write(" ");
                self.format_expression(right);
            }
            Expression::Unary { op, operand, .. } => {
                self.format_unary_op(op);
                self.format_expression(operand);
            }
            Expression::Call { function, args, .. } => {
                self.write(function);
                self.write("(");
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.format_expression(arg);
                }
                self.write(")");
            }
            Expression::Assignment { name, value, .. } => {
                self.write(name);
                self.write(" = ");
                self.format_expression(value);
            }
            Expression::Index { object, index, .. } => {
                self.format_expression(object);
                self.write("[");
                self.format_expression(index);
                self.write("]");
            }
            Expression::MethodCall { object, method, args, .. } => {
                self.format_expression(object);
                self.write(".");
                self.write(method);
                self.write("(");
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.format_expression(arg);
                }
                self.write(")");
            }
            Expression::Block(block) => {
                self.format_function_block(block);
            }
        }
    }

    fn format_literal(&mut self, literal: &Literal) {
        match literal {
            Literal::Bool(value, _) => self.write(&value.to_string()),
            Literal::Integer(value, _) => self.write(&value.to_string()),
            Literal::String(value, _) => {
                self.write("\"");
                // Escape special characters
                for c in value.chars() {
                    match c {
                        '\n' => self.write("\\n"),
                        '\t' => self.write("\\t"),
                        '\r' => self.write("\\r"),
                        '\\' => self.write("\\\\"),
                        '"' => self.write("\\\""),
                        c => self.write(&c.to_string()),
                    }
                }
                self.write("\"");
            }
            Literal::InterpolatedString(parts, _) => {
                self.write("\"");
                for part in parts {
                    match part {
                        InterpolationPart::Text(text) => {
                            // Escape special characters in text parts
                            for c in text.chars() {
                                match c {
                                    '\n' => self.write("\\n"),
                                    '\t' => self.write("\\t"),
                                    '\r' => self.write("\\r"),
                                    '\\' => self.write("\\\\"),
                                    '"' => self.write("\\\""),
                                    c => self.write(&c.to_string()),
                                }
                            }
                        }
                        InterpolationPart::Expression(expr) => {
                            self.write("${");
                            self.format_expression(expr);
                            self.write("}");
                        }
                    }
                }
                self.write("\"");
            }
            Literal::Array(elements, _) => {
                self.write("[");
                for (i, element) in elements.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.format_expression(element);
                }
                self.write("]");
            }
        }
    }

    fn format_binary_op(&mut self, op: &BinaryOp) {
        let op_str = match op {
            BinaryOp::Add => "+",
            BinaryOp::Subtract => "-",
            BinaryOp::Multiply => "*",
            BinaryOp::Divide => "/",
            BinaryOp::Modulo => "%",
            BinaryOp::Equal => "==",
            BinaryOp::NotEqual => "!=",
            BinaryOp::Less => "<",
            BinaryOp::LessEqual => "<=",
            BinaryOp::Greater => ">",
            BinaryOp::GreaterEqual => ">=",
            BinaryOp::And => "and",
            BinaryOp::Or => "or",
        };
        self.write(op_str);
    }

    fn format_unary_op(&mut self, op: &UnaryOp) {
        let op_str = match op {
            UnaryOp::Not => "not ",
            UnaryOp::Negate => "-",
        };
        self.write(op_str);
    }

    fn write(&mut self, s: &str) {
        self.buffer.push_str(s);
    }

    fn write_line(&mut self, s: &str) {
        self.buffer.push_str(s);
        self.buffer.push('\n');
    }

    fn write_indent(&mut self) {
        for _ in 0..(self.indent * 2) {
            self.buffer.push(' ');
        }
    }
}