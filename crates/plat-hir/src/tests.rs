#[cfg(test)]
mod tests {
    use crate::TypeChecker;
    use plat_parser::Parser;

    fn type_check(input: &str) -> Result<(), plat_diags::DiagnosticError> {
        let parser = Parser::new(input)?;
        let program = parser.parse()?;
        let type_checker = TypeChecker::new();
        type_checker.check_program(&program)
    }

    #[test]
    fn test_simple_main_function() {
        let input = r#"
            fn main() {
                print("Hello, world!");
            }
        "#;

        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_main_function_required() {
        let input = r#"
            fn hello() {
                print("Hello!");
            }
        "#;

        let result = type_check(input);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("main function"));
    }

    #[test]
    fn test_main_function_wrong_signature() {
        let input = r#"
            fn main(x: i32) {
                print("Hello!");
            }
        "#;

        let result = type_check(input);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("no parameters"));
    }

    #[test]
    fn test_let_variable_inference() {
        let input = r#"
            fn main() {
                let x = 42;
                let y = true;
                let z = "hello";
            }
        "#;

        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_let_variable_explicit_type() {
        let input = r#"
            fn main() {
                let x: i32 = 42;
                let y: bool = true;
                let z: string = "hello";
            }
        "#;

        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_let_variable_type_mismatch() {
        let input = r#"
            fn main() {
                let x: i32 = true;
            }
        "#;

        let result = type_check(input);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Type mismatch"));
    }

    #[test]
    fn test_variable_shadowing_not_allowed() {
        let input = r#"
            fn main() {
                let x = 5;
                let x = 10;
            }
        "#;

        let result = type_check(input);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already defined"));
    }

    #[test]
    fn test_undefined_variable() {
        let input = r#"
            fn main() {
                print(unknown_var);
            }
        "#;

        let result = type_check(input);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Undefined variable"));
    }

    #[test]
    fn test_var_assignment() {
        let input = r#"
            fn main() {
                var x = 5;
                x = 10;
            }
        "#;

        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_var_assignment_type_mismatch() {
        let input = r#"
            fn main() {
                var x = 5;
                x = "hello";
            }
        "#;

        let result = type_check(input);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Assignment type mismatch"));
    }

    #[test]
    fn test_arithmetic_operations() {
        let input = r#"
            fn main() {
                let a = 5 + 3;
                let b = 10 - 2;
                let c = 4 * 7;
                let d = 20 / 5;
                let e = 17 % 3;
            }
        "#;

        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_arithmetic_type_mismatch() {
        let input = r#"
            fn main() {
                let x = 5 + true;
            }
        "#;

        let result = type_check(input);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Cannot apply"));
    }

    #[test]
    fn test_boolean_operations() {
        let input = r#"
            fn main() {
                let a = true and false;
                let b = true or false;
                let c = not true;
            }
        "#;

        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_boolean_type_mismatch() {
        let input = r#"
            fn main() {
                let x = 5 and true;
            }
        "#;

        let result = type_check(input);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("boolean operands"));
    }

    #[test]
    fn test_comparison_operations() {
        let input = r#"
            fn main() {
                let a = 5 < 10;
                let b = 5 <= 5;
                let c = 10 > 5;
                let d = 10 >= 10;
                let e = 5 == 5;
                let f = 5 != 3;
            }
        "#;

        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_comparison_type_mismatch() {
        let input = r#"
            fn main() {
                let x = 5 < "hello";
            }
        "#;

        let result = type_check(input);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Cannot compare"));
    }

    #[test]
    fn test_string_concatenation() {
        let input = r#"
            fn main() {
                let greeting = "Hello, " + "world!";
            }
        "#;

        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_if_statement() {
        let input = r#"
            fn main() {
                if (true) {
                    print("yes");
                } else {
                    print("no");
                }
            }
        "#;

        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_if_condition_not_boolean() {
        let input = r#"
            fn main() {
                if (5) {
                    print("yes");
                }
            }
        "#;

        let result = type_check(input);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be boolean"));
    }

    #[test]
    fn test_while_loop() {
        let input = r#"
            fn main() {
                var x = 0;
                while (x < 10) {
                    x = x + 1;
                }
            }
        "#;

        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_while_condition_not_boolean() {
        let input = r#"
            fn main() {
                while (5) {
                    print("loop");
                }
            }
        "#;

        let result = type_check(input);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be boolean"));
    }

    #[test]
    fn test_function_call() {
        let input = r#"
            fn add(x: i32, y: i32) -> i32 {
                return x + y;
            }

            fn main() {
                let result = add(5, 3);
            }
        "#;

        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_function_call_wrong_args() {
        let input = r#"
            fn add(x: i32, y: i32) -> i32 {
                return x + y;
            }

            fn main() {
                let result = add(5);
            }
        "#;

        let result = type_check(input);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("expects 2 arguments"));
    }

    #[test]
    fn test_function_call_wrong_arg_types() {
        let input = r#"
            fn add(x: i32, y: i32) -> i32 {
                return x + y;
            }

            fn main() {
                let result = add(5, "hello");
            }
        "#;

        let result = type_check(input);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("expects argument of type"));
    }

    #[test]
    fn test_return_type_checking() {
        let input = r#"
            fn get_number() -> i32 {
                return 42;
            }

            fn main() {
                let x = get_number();
            }
        "#;

        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_return_type_mismatch() {
        let input = r#"
            fn get_number() -> i32 {
                return "hello";
            }

            fn main() {
            }
        "#;

        let result = type_check(input);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Return type mismatch"));
    }

    #[test]
    fn test_function_duplicate_definition() {
        let input = r#"
            fn test() {
            }

            fn test() {
            }

            fn main() {
            }
        "#;

        let result = type_check(input);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("defined multiple times"));
    }

    #[test]
    fn test_unknown_function() {
        let input = r#"
            fn main() {
                unknown_function();
            }
        "#;

        let result = type_check(input);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unknown function"));
    }

    #[test]
    fn test_scoping() {
        let input = r#"
            fn main() {
                let x = 5;
                if (true) {
                    let y = 10;
                    let z = x + y; // x is visible from outer scope
                }
                // y is not visible here
            }
        "#;

        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_parameter_scoping() {
        let input = r#"
            fn test(x: i32, y: i32) -> i32 {
                let z = x + y;
                return z;
            }

            fn main() {
                let result = test(5, 10);
            }
        "#;

        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_duplicate_parameters() {
        let input = r#"
            fn test(x: i32, x: i32) -> i32 {
                return x;
            }

            fn main() {
            }
        "#;

        let result = type_check(input);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("defined multiple times"));
    }

    #[test]
    fn test_string_interpolation() {
        let input = r#"
            fn main() {
                let name = "World";
                let greeting = "Hello, ${name}!";
                print(greeting);
            }
        "#;

        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_print_different_types() {
        let input = r#"
            fn main() {
                print(42);
                print(true);
                print("hello");
            }
        "#;

        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_unary_negation() {
        let input = r#"
            fn main() {
                let x = -5;
                let y = -(-10);
            }
        "#;

        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_unary_negation_wrong_type() {
        let input = r#"
            fn main() {
                let x = -true;
            }
        "#;

        let result = type_check(input);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Cannot negate"));
    }

    #[test]
    fn test_not_operator_wrong_type() {
        let input = r#"
            fn main() {
                let x = not 5;
            }
        "#;

        let result = type_check(input);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Cannot apply 'not'"));
    }
}