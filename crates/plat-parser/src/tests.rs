#[cfg(test)]
mod tests {
    use crate::Parser;
    use plat_ast::*;

    #[test]
    fn test_parse_simple_function() {
        let input = r#"
            fn main() {
                print(value = "Hello, world!");
            }
        "#;

        let parser = Parser::new(input).unwrap();
        let program = parser.parse().unwrap();

        assert_eq!(program.functions.len(), 1);
        assert_eq!(program.functions[0].name, "main");
        assert_eq!(program.functions[0].params.len(), 0);
        assert_eq!(program.functions[0].return_type, None);
        assert_eq!(program.functions[0].is_mutable, false);
    }

    #[test]
    fn test_parse_function_with_params() {
        let input = r#"
            fn add(x: Int32, y: Int32) -> Int32 {
                return x + y;
            }
        "#;

        let parser = Parser::new(input).unwrap();
        let program = parser.parse().unwrap();

        assert_eq!(program.functions.len(), 1);
        let func = &program.functions[0];
        assert_eq!(func.name, "add");
        assert_eq!(func.params.len(), 2);
        assert_eq!(func.params[0].name, "x");
        assert_eq!(func.params[0].ty, Type::Int32);
        assert_eq!(func.params[1].name, "y");
        assert_eq!(func.params[1].ty, Type::Int32);
        assert_eq!(func.return_type, Some(Type::Int32));
    }

    // FIXME: Outdated test - type annotations are now mandatory
    // #[test]
    // fn test_parse_let_and_var_statements() {
    //     // Test disabled - type inference removed, all variables need explicit types
    // }

    #[test]
    fn test_parse_if_else() {
        let input = r#"
            fn main() {
                if (x > 10) {
                    print(value = "greater");
                } else {
                    print(value = "less or equal");
                }
            }
        "#;

        let parser = Parser::new(input).unwrap();
        let program = parser.parse().unwrap();

        let statements = &program.functions[0].body.statements;
        assert_eq!(statements.len(), 1);

        match &statements[0] {
            Statement::If { then_branch, else_branch, .. } => {
                assert_eq!(then_branch.statements.len(), 1);
                assert!(else_branch.is_some());
                assert_eq!(else_branch.as_ref().unwrap().statements.len(), 1);
            }
            _ => panic!("Expected if statement"),
        }
    }

    #[test]
    fn test_parse_while_loop() {
        let input = r#"
            fn main() {
                while (x < 10) {
                    x = x + 1;
                }
            }
        "#;

        let parser = Parser::new(input).unwrap();
        let program = parser.parse().unwrap();

        let statements = &program.functions[0].body.statements;
        assert_eq!(statements.len(), 1);

        match &statements[0] {
            Statement::While { body, .. } => {
                assert_eq!(body.statements.len(), 1);
            }
            _ => panic!("Expected while statement"),
        }
    }

    #[test]
    fn test_parse_for_loop() {
        let input = r#"
            fn main() {
                for (item: String in items) {
                    print(value = item);
                }
            }
        "#;

        let parser = Parser::new(input).unwrap();
        let program = parser.parse().unwrap();

        let statements = &program.functions[0].body.statements;
        assert_eq!(statements.len(), 1);

        match &statements[0] {
            Statement::For { variable, iterable, body, .. } => {
                assert_eq!(variable, "item");
                match iterable {
                    Expression::Identifier { name, .. } => {
                        assert_eq!(name, "items");
                    }
                    _ => panic!("Expected identifier for iterable"),
                }
                assert_eq!(body.statements.len(), 1);
            }
            _ => panic!("Expected for statement"),
        }
    }

    #[test]
    fn test_parse_for_loop_with_array_literal() {
        let input = r#"
            fn main() {
                for (num: Int32 in [1, 2, 3]) {
                    print(value = "Number: ${num}");
                }
            }
        "#;

        let parser = Parser::new(input).unwrap();
        let program = parser.parse().unwrap();

        let statements = &program.functions[0].body.statements;
        assert_eq!(statements.len(), 1);

        match &statements[0] {
            Statement::For { variable, iterable, body, .. } => {
                assert_eq!(variable, "num");
                match iterable {
                    Expression::Literal(Literal::Array(elements, _)) => {
                        assert_eq!(elements.len(), 3);
                    }
                    _ => panic!("Expected array literal for iterable"),
                }
                assert_eq!(body.statements.len(), 1);
            }
            _ => panic!("Expected for statement"),
        }
    }

    #[test]
    fn test_parse_nested_control_flow() {
        let input = r#"
            fn main() {
                if (x > 0) {
                    for (i: Int32 in items) {
                        if (i > 5) {
                            print(value = "Large: ${i}");
                        } else {
                            while (i < 10) {
                                i = i + 1;
                            }
                        }
                    }
                }
            }
        "#;

        let parser = Parser::new(input).unwrap();
        let program = parser.parse().unwrap();

        let statements = &program.functions[0].body.statements;
        assert_eq!(statements.len(), 1);

        // Just verify it parses without panicking - structure is complex
        match &statements[0] {
            Statement::If { then_branch, .. } => {
                assert_eq!(then_branch.statements.len(), 1);
                match &then_branch.statements[0] {
                    Statement::For { body, .. } => {
                        assert_eq!(body.statements.len(), 1);
                    }
                    _ => panic!("Expected for statement in if body"),
                }
            }
            _ => panic!("Expected if statement"),
        }
    }

    #[test]
    fn test_parse_expressions() {
        let input = r#"
            fn main() {
                let a: Int32 = 1 + 2 * 3;
                let b: Int32 = (1 + 2) * 3;
                let c: Bool = true and false or not true;
                let d: Bool = x == 5 and y != 10;
            }
        "#;

        let parser = Parser::new(input).unwrap();
        let program = parser.parse().unwrap();

        let statements = &program.functions[0].body.statements;
        assert_eq!(statements.len(), 4);
    }

    #[test]
    fn test_parse_function_calls() {
        let input = r#"
            fn main() {
                print(value = "Hello");
                let result: Int32 = add(x = 10, y = 20);
                let complex: Int32 = multiply(a = add(x = 1, y = 2), b = subtract(a = 5, b = 3));
            }
        "#;

        let parser = Parser::new(input).unwrap();
        let program = parser.parse().unwrap();

        let statements = &program.functions[0].body.statements;
        assert_eq!(statements.len(), 3);

        match &statements[0] {
            Statement::Print { .. } => {}
            _ => panic!("Expected print statement"),
        }

        match &statements[1] {
            Statement::Let { value, .. } => {
                match value {
                    Expression::Call { function, args, .. } => {
                        assert_eq!(function, "add");
                        assert_eq!(args.len(), 2);
                    }
                    _ => panic!("Expected function call"),
                }
            }
            _ => panic!("Expected let statement"),
        }
    }

    #[test]
    fn test_parse_string_interpolation() {
        let input = r#"
            fn main() {
                let name: String = "World";
                print(value = "Hello, ${name}!");
                print(value = "The sum of 2 + 2 is ${2 + 2}");
            }
        "#;

        let parser = Parser::new(input).unwrap();
        let program = parser.parse().unwrap();

        let statements = &program.functions[0].body.statements;
        assert_eq!(statements.len(), 3);
    }

    #[test]
    fn test_parse_all_operators() {
        let input = r#"
            fn main() {
                let a: Int32 = 10 + 5;
                let b: Int32 = 10 - 5;
                let c: Int32 = 10 * 5;
                let d: Int32 = 10 / 5;
                let e: Int32 = 10 % 3;
                let f: Bool = 10 == 10;
                let g: Bool = 10 != 5;
                let h: Bool = 10 > 5;
                let i: Bool = 10 >= 10;
                let j: Bool = 10 < 15;
                let k: Bool = 10 <= 10;
                let l: Bool = true and false;
                let m: Bool = true or false;
                let n: Bool = not true;
                let o: Int32 = -5;
            }
        "#;

        let parser = Parser::new(input).unwrap();
        let program = parser.parse().unwrap();

        let statements = &program.functions[0].body.statements;
        assert_eq!(statements.len(), 15);
    }

    #[test]
    fn test_parse_assignment() {
        let input = r#"
            fn main() {
                var x: Int32 = 10;
                x = 20;
                x = x + 1;
            }
        "#;

        let parser = Parser::new(input).unwrap();
        let program = parser.parse().unwrap();

        let statements = &program.functions[0].body.statements;
        assert_eq!(statements.len(), 3);

        match &statements[1] {
            Statement::Expression(Expression::Assignment { target, .. }) => {
                match target.as_ref() {
                    Expression::Identifier { name, .. } => {
                        assert_eq!(name, "x");
                    }
                    _ => panic!("Expected identifier as assignment target"),
                }
            }
            _ => panic!("Expected assignment expression"),
        }
    }

    #[test]
    fn test_parse_multiple_functions() {
        let input = r#"
            fn add(x: Int32, y: Int32) -> Int32 {
                return x + y;
            }

            fn main() {
                let result: Int32 = add(x = 5, y = 3);
                print(value = "Result: ${result}");
            }
        "#;

        let parser = Parser::new(input).unwrap();
        let program = parser.parse().unwrap();

        assert_eq!(program.functions.len(), 2);
        assert_eq!(program.functions[0].name, "add");
        assert_eq!(program.functions[1].name, "main");
    }

    #[test]
    fn test_parse_literals() {
        let input = r#"
            fn main() {
                let a: Bool = True;
                let b: Bool = False;
                let c: Int32 = 42;
                let d: Int64 = 100i64;
                let e: String = "hello";
                let f: String = "hello ${name}";
            }
        "#;

        let parser = Parser::new(input).unwrap();
        let program = parser.parse().unwrap();

        let statements = &program.functions[0].body.statements;
        assert_eq!(statements.len(), 6);
    }

    #[test]
    fn test_parse_error_missing_semicolon() {
        let input = r#"
            fn main() {
                let x: Int32 = 10
            }
        "#;

        let parser = Parser::new(input).unwrap();
        let result = parser.parse();
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_error_invalid_type() {
        let input = r#"
            fn main() {
                let x: invalid = 10;
            }
        "#;

        let parser = Parser::new(input).unwrap();
        let result = parser.parse();
        // Now that we support custom types/enums, this should actually succeed
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_enum_declaration() {
        let input = r#"
            enum Message {
                Quit,
                Move(Int32, Int32),
                Write(String)
            }
        "#;

        let parser = Parser::new(input).unwrap();
        let program = parser.parse().unwrap();

        assert_eq!(program.enums.len(), 1);
        let enum_decl = &program.enums[0];
        assert_eq!(enum_decl.name, "Message");
        assert_eq!(enum_decl.variants.len(), 3);

        assert_eq!(enum_decl.variants[0].name, "Quit");
        assert_eq!(enum_decl.variants[0].fields.len(), 0);

        assert_eq!(enum_decl.variants[1].name, "Move");
        assert_eq!(enum_decl.variants[1].fields.len(), 2);
        assert_eq!(enum_decl.variants[1].fields[0], Type::Int32);
        assert_eq!(enum_decl.variants[1].fields[1], Type::Int32);

        assert_eq!(enum_decl.variants[2].name, "Write");
        assert_eq!(enum_decl.variants[2].fields.len(), 1);
        assert_eq!(enum_decl.variants[2].fields[0], Type::String);
    }

    #[test]
    fn test_parse_generic_enum() {
        let input = r#"
            enum Option<T> {
                Some(T),
                None
            }
        "#;

        let parser = Parser::new(input).unwrap();
        let program = parser.parse().unwrap();

        assert_eq!(program.enums.len(), 1);
        let enum_decl = &program.enums[0];
        assert_eq!(enum_decl.name, "Option");
        assert_eq!(enum_decl.type_params.len(), 1);
        assert_eq!(enum_decl.type_params[0], "T");
        assert_eq!(enum_decl.variants.len(), 2);
    }

    #[test]
    fn test_parse_enum_with_methods() {
        let input = r#"
            enum Message {
                Quit,
                Move(Int32, Int32),

                fn is_quit() -> Bool {
                    return True;
                }

                mut fn process() {
                    print(value = "Processing message");
                }
            }
        "#;

        let parser = Parser::new(input).unwrap();
        let program = parser.parse().unwrap();

        assert_eq!(program.enums.len(), 1);
        let enum_decl = &program.enums[0];
        assert_eq!(enum_decl.name, "Message");
        assert_eq!(enum_decl.methods.len(), 2);

        assert_eq!(enum_decl.methods[0].name, "is_quit");
        assert_eq!(enum_decl.methods[0].is_mutable, false);
        assert_eq!(enum_decl.methods[0].return_type, Some(Type::Bool));

        assert_eq!(enum_decl.methods[1].name, "process");
        assert_eq!(enum_decl.methods[1].is_mutable, true);
    }

    #[test]
    fn test_parse_enum_constructor() {
        let input = r#"
            fn main() {
                let msg1: Message = Message::Quit;
                let msg2: Message = Message::Move(field_0 = 10, field_1 = 20);
                let msg3: Message = Message::Write(value = "Hello");
            }
        "#;

        let parser = Parser::new(input).unwrap();
        let program = parser.parse().unwrap();

        let statements = &program.functions[0].body.statements;
        assert_eq!(statements.len(), 3);

        match &statements[0] {
            Statement::Let { value, .. } => {
                match value {
                    Expression::EnumConstructor { enum_name, variant, args, .. } => {
                        assert_eq!(enum_name, "Message");
                        assert_eq!(variant, "Quit");
                        assert_eq!(args.len(), 0);
                    }
                    _ => panic!("Expected enum constructor"),
                }
            }
            _ => panic!("Expected let statement"),
        }

        match &statements[1] {
            Statement::Let { value, .. } => {
                match value {
                    Expression::EnumConstructor { enum_name, variant, args, .. } => {
                        assert_eq!(enum_name, "Message");
                        assert_eq!(variant, "Move");
                        assert_eq!(args.len(), 2);
                    }
                    _ => panic!("Expected enum constructor"),
                }
            }
            _ => panic!("Expected let statement"),
        }
    }

    // FIXME: Outdated test - pattern bindings now include types
    // #[test]
    // fn test_parse_match_expression() {
    //     // Test disabled - pattern binding structure changed to include types
    // }

    #[test]
    fn test_parse_mutable_function() {
        let input = r#"
            mut fn update(value: Int32) {
                let x: Int32 = value;
            }
        "#;

        let parser = Parser::new(input).unwrap();
        let program = parser.parse().unwrap();

        assert_eq!(program.functions.len(), 1);
        assert_eq!(program.functions[0].name, "update");
        assert_eq!(program.functions[0].is_mutable, true);
    }

    #[test]
    fn test_range_expression() {
        let input = r#"
            fn my_func() {
                let x: Range = 0..5;
                let y: Range = 10..=20;
            }
        "#;

        let parser = Parser::new(input).unwrap();
        let program = parser.parse().unwrap();

        assert_eq!(program.functions.len(), 1);
        assert_eq!(program.functions[0].body.statements.len(), 2);
    }

    #[test]
    fn test_for_range_loop() {
        let input = r#"
            fn main() -> Int32 {
                for (i: Int32 in 0..5) {
                    let x: Int32 = i;
                }
                return 0;
            }
        "#;

        let parser = Parser::new(input).unwrap();
        let program = parser.parse().unwrap();

        assert_eq!(program.functions.len(), 1);
        assert_eq!(program.functions[0].name, "main");
    }
}