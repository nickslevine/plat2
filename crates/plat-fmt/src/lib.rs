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
        let mut items_written = 0;

        // Format enums first
        for enum_decl in &program.enums {
            if items_written > 0 {
                self.write_line("");
            }
            self.format_enum(enum_decl);
            self.write_line("");
            items_written += 1;
        }

        // Then format classes
        for class_decl in &program.classes {
            if items_written > 0 {
                self.write_line("");
            }
            self.format_class(class_decl);
            self.write_line("");
            items_written += 1;
        }

        // Finally format functions
        for function in &program.functions {
            if items_written > 0 {
                self.write_line("");
            }
            self.format_function(function);
            self.write_line("");
            items_written += 1;
        }
    }

    fn format_enum(&mut self, enum_decl: &EnumDecl) {
        self.write("enum ");
        self.write(&enum_decl.name);

        if !enum_decl.type_params.is_empty() {
            self.write("<");
            for (i, param) in enum_decl.type_params.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                self.write(param);
            }
            self.write(">");
        }

        self.write_line(" {");
        self.indent += 1;

        // Format variants
        for variant in &enum_decl.variants {
            self.write_indent();
            self.write(&variant.name);
            if !variant.fields.is_empty() {
                self.write("(");
                for (i, field) in variant.fields.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.format_type(field);
                }
                self.write(")");
            }
            self.write_line(",");
        }

        // Add blank line before methods if both variants and methods exist
        if !enum_decl.variants.is_empty() && !enum_decl.methods.is_empty() {
            self.write_line("");
        }

        // Format methods
        for method in &enum_decl.methods {
            self.write_indent();
            if method.is_virtual {
                self.write("virtual ");
            }
            if method.is_override {
                self.write("override ");
            }
            if method.is_mutable {
                self.write("mut ");
            }
            self.write("fn ");
            self.write(&method.name);
            self.write("(");

            for (i, param) in method.params.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                self.format_parameter(param);
            }

            self.write(")");

            if let Some(return_type) = &method.return_type {
                self.write(" -> ");
                self.format_type(return_type);
            }

            self.write(" ");
            self.format_function_block(&method.body);
            self.write_line("");
        }

        self.indent -= 1;
        self.write("}");
    }

    fn format_class(&mut self, class_decl: &ClassDecl) {
        self.write("class ");
        self.write(&class_decl.name);

        if !class_decl.type_params.is_empty() {
            self.write("<");
            for (i, param) in class_decl.type_params.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                self.write(param);
            }
            self.write(">");
        }

        // Format inheritance
        if let Some(parent_class) = &class_decl.parent_class {
            self.write(" : ");
            self.write(parent_class);
        }

        self.write_line(" {");
        self.indent += 1;

        // Format fields
        for field in &class_decl.fields {
            self.write_indent();
            if field.is_mutable {
                self.write("var ");
            } else {
                self.write("let ");
            }
            self.write(&field.name);
            self.write(": ");
            self.format_type(&field.ty);
            self.write_line(";");
        }

        // Add blank line before methods if both fields and methods exist
        if !class_decl.fields.is_empty() && !class_decl.methods.is_empty() {
            self.write_line("");
        }

        // Format methods
        for method in &class_decl.methods {
            self.write_indent();
            if method.name == "init" {
                self.write("init");
            } else {
                if method.is_virtual {
                    self.write("virtual ");
                }
                if method.is_override {
                    self.write("override ");
                }
                if method.is_mutable {
                    self.write("mut ");
                }
                self.write("fn ");
                self.write(&method.name);
            }
            self.write("(");

            for (i, param) in method.params.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                self.format_parameter(param);
            }

            self.write(")");

            if let Some(return_type) = &method.return_type {
                self.write(" -> ");
                self.format_type(return_type);
            }

            self.write(" ");
            self.format_function_block(&method.body);
            self.write_line("");
        }

        self.indent -= 1;
        self.write("}");
    }

    fn format_function(&mut self, function: &Function) {
        if function.is_virtual {
            self.write("virtual ");
        }
        if function.is_override {
            self.write("override ");
        }
        if function.is_mutable {
            self.write("mut ");
        }
        self.write("fn ");
        self.write(&function.name);

        // Format generic type parameters
        if !function.type_params.is_empty() {
            self.write("<");
            for (i, type_param) in function.type_params.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                self.write(type_param);
            }
            self.write(">");
        }

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
            Type::F32 => self.write("f32"),
            Type::F64 => self.write("f64"),
            Type::String => self.write("string"),
            Type::List(element_type) => {
                self.write("List[");
                self.format_type(element_type);
                self.write("]");
            }
            Type::Dict(key_type, value_type) => {
                self.write("Dict[");
                self.format_type(key_type);
                self.write(", ");
                self.format_type(value_type);
                self.write("]");
            }
            Type::Set(element_type) => {
                self.write("Set[");
                self.format_type(element_type);
                self.write("]");
            }
            Type::Named(name, type_params) => {
                self.write(name);
                if !type_params.is_empty() {
                    self.write("<");
                    for (i, param) in type_params.iter().enumerate() {
                        if i > 0 {
                            self.write(", ");
                        }
                        self.format_type(param);
                    }
                    self.write(">");
                }
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
            Statement::For { variable, iterable, body, .. } => {
                self.write("for (");
                self.write(variable);
                self.write(" in ");
                self.format_expression(iterable);
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
            Expression::Assignment { target, value, .. } => {
                self.format_expression(target);
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
            Expression::EnumConstructor { enum_name, variant, args, .. } => {
                self.write(enum_name);
                self.write("::");
                self.write(variant);
                if !args.is_empty() {
                    self.write("(");
                    for (i, arg) in args.iter().enumerate() {
                        if i > 0 {
                            self.write(", ");
                        }
                        self.format_expression(arg);
                    }
                    self.write(")");
                }
            }
            Expression::Match { value, arms, .. } => {
                self.write("match ");
                self.format_expression(value);
                self.write_line(" {");
                self.indent += 1;
                for arm in arms {
                    self.write_indent();
                    self.format_pattern(&arm.pattern);
                    self.write(" -> ");
                    self.format_expression(&arm.body);
                    self.write_line(",");
                }
                self.indent -= 1;
                self.write_indent();
                self.write("}");
            }
            Expression::Try { expression, .. } => {
                self.format_expression(expression);
                self.write("?");
            }
            Expression::Self_ { .. } => {
                self.write("self");
            }
            Expression::MemberAccess { object, member, .. } => {
                self.format_expression(object);
                self.write(".");
                self.write(member);
            }
            Expression::ConstructorCall { class_name, args, .. } => {
                self.write(class_name);
                self.write("(");
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.write(&arg.name);
                    self.write(" = ");
                    self.format_expression(&arg.value);
                }
                self.write(")");
            }
            Expression::SuperCall { method, args, .. } => {
                self.write("super.");
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
            Expression::Range { start, end, inclusive, .. } => {
                self.format_expression(start);
                if *inclusive {
                    self.write("..=");
                } else {
                    self.write("..");
                }
                self.format_expression(end);
            }
        }
    }

    fn format_literal(&mut self, literal: &Literal) {
        match literal {
            Literal::Bool(value, _) => self.write(&value.to_string()),
            Literal::Integer(value, _) => self.write(&value.to_string()),
            Literal::Float(value, float_type, _) => {
                self.write(&value.to_string());
                // Add type suffix if f32
                if matches!(float_type, FloatType::F32) {
                    self.write("f32");
                }
            }
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
            Literal::Dict(pairs, _) => {
                self.write("{");
                for (i, (key, value)) in pairs.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.format_expression(key);
                    self.write(": ");
                    self.format_expression(value);
                }
                self.write("}");
            }
            Literal::Set(elements, _) => {
                self.write("Set{");
                for (i, element) in elements.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.format_expression(element);
                }
                self.write("}");
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

    fn format_pattern(&mut self, pattern: &Pattern) {
        match pattern {
            Pattern::EnumVariant { enum_name, variant, bindings, .. } => {
                if let Some(enum_name) = enum_name {
                    self.write(enum_name);
                    self.write("::");
                }
                self.write(variant);
                if !bindings.is_empty() {
                    self.write("(");
                    for (i, binding) in bindings.iter().enumerate() {
                        if i > 0 {
                            self.write(", ");
                        }
                        self.write(binding);
                    }
                    self.write(")");
                }
            }
            Pattern::Identifier { name, .. } => {
                self.write(name);
            }
            Pattern::Literal(literal) => {
                self.format_literal(literal);
            }
        }
    }
}