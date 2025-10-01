#[cfg(test)]
mod tests {
    use crate::TypeChecker;
    use plat_parser::Parser;

    fn type_check(input: &str) -> Result<(), plat_diags::DiagnosticError> {
        let parser = Parser::new(input)?;
        let program = parser.parse()?;
        let type_checker = TypeChecker::new();
        let result = type_checker.check_program(&program);
        if let Err(e) = &result {
            eprintln!("Type check error: {}", e);
        }
        result
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

        let result = type_check(input);
        if let Err(ref e) = result {
            panic!("Type check failed with error: {}", e);
        }
        assert!(result.is_ok());
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

    #[test]
    fn test_for_loop_type_checking() {
        let input = r#"
            fn main() {
                let numbers = [1, 2, 3, 4, 5];
                for (num in numbers) {
                    print(num);
                }
            }
        "#;

        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_for_loop_non_array_iterable() {
        let input = r#"
            fn main() {
                let x = 42;
                for (item in x) {
                    print(item);
                }
            }
        "#;

        let result = type_check(input);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("can only iterate over List or Range types"));
    }

    #[test]
    fn test_for_loop_variable_scoping() {
        let input = r#"
            fn main() {
                let numbers = [1, 2, 3];
                for (num in numbers) {
                    let doubled = num * 2;
                    print(doubled);
                }
                // num should not be visible here
            }
        "#;

        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_for_loop_variable_shadowing() {
        let input = r#"
            fn main() {
                let num = 42;
                let numbers = [1, 2, 3];
                for (num in numbers) {
                    print(num); // This shadows the outer 'num'
                }
                print(num); // This refers to the original 'num'
            }
        "#;

        // For loops create a new scope, so the loop variable doesn't conflict with outer scope
        // This is actually valid behavior - the loop variable shadows the outer one temporarily
        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_nested_control_flow_scoping() {
        let input = r#"
            fn main() {
                let arr = [1, 2, 3];
                for (x in arr) {
                    if (x > 1) {
                        let y = x * 2;
                        while (y > 0) {
                            y = y - 1;
                            if (y == 1) {
                                print("found one");
                            }
                        }
                    }
                }
            }
        "#;

        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_loop_variable_access_in_body() {
        let input = r#"
            fn main() {
                let items = [10, 20, 30];
                for (item in items) {
                    let result = item + 5;
                    print(result);
                }
            }
        "#;

        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_for_loop_with_complex_expressions() {
        let input = r#"
            fn main() {
                let arrays = [[1, 2], [3, 4]];
                for (subarray in arrays) {
                    let length = subarray.len();
                    print(length);
                }
            }
        "#;

        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_enum_declaration() {
        let input = r#"
            enum Message {
                Quit,
                Move(i32, i32),
                Write(string)
            }

            fn main() {
            }
        "#;

        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_enum_constructor() {
        let input = r#"
            enum Message {
                Quit,
                Move(i32, i32),
                Write(string)
            }

            fn main() {
                let msg1 = Message::Quit;
                let msg2 = Message::Move(10, 20);
                let msg3 = Message::Write("Hello");
            }
        "#;

        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_enum_constructor_wrong_args() {
        let input = r#"
            enum Message {
                Quit,
                Move(i32, i32),
                Write(string)
            }

            fn main() {
                let msg = Message::Move(10);
            }
        "#;

        let result = type_check(input);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("expects 2 arguments"));
    }

    #[test]
    fn test_enum_constructor_wrong_arg_types() {
        let input = r#"
            enum Message {
                Quit,
                Move(i32, i32),
                Write(string)
            }

            fn main() {
                let msg = Message::Move("hello", 20);
            }
        "#;

        let result = type_check(input);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("has type"));
    }

    #[test]
    fn test_enum_unknown_variant() {
        let input = r#"
            enum Message {
                Quit,
                Move(i32, i32)
            }

            fn main() {
                let msg = Message::Unknown;
            }
        "#;

        let result = type_check(input);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("has no variant"));
    }

    #[test]
    fn test_match_expression() {
        let input = r#"
            enum Message {
                Quit,
                Move(i32, i32),
                Write(string)
            }

            fn main() {
                let msg = Message::Move(10, 20);
                let result = match msg {
                    Message::Quit -> 0,
                    Message::Move(x, y) -> x + y,
                    Message::Write(s) -> 100
                };
            }
        "#;

        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_match_expression_non_exhaustive() {
        let input = r#"
            enum Message {
                Quit,
                Move(i32, i32),
                Write(string)
            }

            fn main() {
                let msg = Message::Move(10, 20);
                let result = match msg {
                    Message::Quit -> 0,
                    Message::Move(x, y) -> x + y
                };
            }
        "#;

        let result = type_check(input);
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("not exhaustive"));
        assert!(error_msg.contains("Write"));
    }

    #[test]
    fn test_match_expression_inconsistent_types() {
        let input = r#"
            enum Message {
                Quit,
                Move(i32, i32)
            }

            fn main() {
                let msg = Message::Move(10, 20);
                let result = match msg {
                    Message::Quit -> 0,
                    Message::Move(x, y) -> "hello"
                };
            }
        "#;

        let result = type_check(input);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("returns type"));
    }

    #[test]
    fn test_match_with_pattern_bindings() {
        let input = r#"
            enum Message {
                Move(i32, i32),
                Write(string)
            }

            fn main() {
                let msg = Message::Write("hello");
                let result = match msg {
                    Message::Move(x, y) -> x + y,
                    Message::Write(text) -> text
                };
            }
        "#;

        let result = type_check(input);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("returns type"));
    }

    #[test]
    fn test_enum_duplicate_definition() {
        let input = r#"
            enum Message {
                Quit
            }

            enum Message {
                Move(i32, i32)
            }

            fn main() {
            }
        "#;

        let result = type_check(input);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("defined multiple times"));
    }

    // TODO: Generic enum support - requires more complex type inference
    // #[test]
    // fn test_generic_enum() {
    //     let input = r#"
    //         enum Option<T> {
    //             Some(T),
    //             None
    //         }

    //         fn main() {
    //             let some_int = Option::Some(42);
    //             let none_int = Option::None;
    //         }
    //     "#;

    //     assert!(type_check(input).is_ok());
    // }

    #[test]
    fn test_enum_with_methods() {
        let input = r#"
            enum Message {
                Quit,
                Move(i32, i32),

                fn is_quit() -> bool {
                    return true;
                }
            }

            fn main() {
            }
        "#;

        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_polymorphic_assignment_let() {
        let input = r#"
            class Animal {
                let species: string;

                init(species: string) -> Animal {
                    self.species = species;
                    return self;
                }

                virtual fn make_sound() -> string {
                    return "Generic animal sound";
                }
            }

            class Dog : Animal {
                let species: string;
                let breed: string;

                init(species: string, breed: string) -> Dog {
                    self.species = species;
                    self.breed = breed;
                    return self;
                }

                override fn make_sound() -> string {
                    return "Woof!";
                }
            }

            fn main() -> i32 {
                let animal: Animal = Dog(species = "Canine", breed = "Golden");
                print("Animal created");
                return 0;
            }
        "#;

        let result = type_check(input);
        if let Err(e) = &result {
            eprintln!("Error: {}", e);
        }
        assert!(result.is_ok());
    }

    #[test]
    fn test_polymorphic_assignment_var() {
        let input = r#"
            class Animal {
                let name: string;

                init(name: string) -> Animal {
                    self.name = name;
                    return self;
                }
            }

            class Cat : Animal {
                let name: string;

                init(name: string) -> Cat {
                    self.name = name;
                    return self;
                }
            }

            fn main() -> i32 {
                var animal: Animal = Cat(name = "Whiskers");
                print("Cat created as Animal");
                return 0;
            }
        "#;

        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_polymorphic_assignment_transitive() {
        let input = r#"
            class Animal {
                let name: string;

                init(name: string) -> Animal {
                    self.name = name;
                    return self;
                }
            }

            class Mammal : Animal {
                let name: string;

                init(name: string) -> Mammal {
                    self.name = name;
                    return self;
                }
            }

            class Dog : Mammal {
                let name: string;

                init(name: string) -> Dog {
                    self.name = name;
                    return self;
                }
            }

            fn main() -> i32 {
                let animal: Animal = Dog(name = "Rex");
                let mammal: Mammal = Dog(name = "Spot");
                print("Transitive inheritance works!");
                return 0;
            }
        "#;

        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_polymorphic_assignment_field() {
        let input = r#"
            class Animal {
                let name: string;

                init(name: string) -> Animal {
                    self.name = name;
                    return self;
                }
            }

            class Dog : Animal {
                let name: string;

                init(name: string) -> Dog {
                    self.name = name;
                    return self;
                }
            }

            class AnimalContainer {
                var animal: Animal;

                init(animal: Animal) -> AnimalContainer {
                    self.animal = animal;
                    return self;
                }

                fn set_animal(animal: Animal) {
                    self.animal = animal;
                }
            }

            fn main() -> i32 {
                let dog = Dog(name = "Buddy");
                let container = AnimalContainer(animal = dog);
                print("Dog stored in Animal field");
                return 0;
            }
        "#;

        let result = type_check(input);
        if let Err(e) = &result {
            eprintln!("Field test error: {}", e);
        }
        assert!(result.is_ok());
    }

    #[test]
    fn test_polymorphic_assignment_invalid_upcast() {
        let input = r#"
            class Animal {
                let name: string;

                init(name: string) -> Animal {
                    self.name = name;
                    return self;
                }
            }

            class Dog : Animal {
                let name: string;

                init(name: string) -> Dog {
                    self.name = name;
                    return self;
                }
            }

            fn main() -> i32 {
                let dog: Dog = Animal(name = "Generic");
                return 0;
            }
        "#;

        let result = type_check(input);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Type mismatch"));
    }

    #[test]
    fn test_polymorphic_assignment_unrelated_classes() {
        let input = r#"
            class Animal {
                let name: string;

                init(name: string) -> Animal {
                    self.name = name;
                    return self;
                }
            }

            class Vehicle {
                let model: string;

                init(model: string) -> Vehicle {
                    self.model = model;
                    return self;
                }
            }

            fn main() -> i32 {
                let animal: Animal = Vehicle(model = "Car");
                return 0;
            }
        "#;

        let result = type_check(input);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Type mismatch"));
    }

    #[test]
    fn test_polymorphic_assignment_variable_reassignment() {
        let input = r#"
            class Animal {
                let name: string;

                init(name: string) -> Animal {
                    self.name = name;
                    return self;
                }
            }

            class Dog : Animal {
                let name: string;

                init(name: string) -> Dog {
                    self.name = name;
                    return self;
                }
            }

            class Cat : Animal {
                let name: string;

                init(name: string) -> Cat {
                    self.name = name;
                    return self;
                }
            }

            fn main() -> i32 {
                var animal: Animal = Dog(name = "Buddy");
                animal = Cat(name = "Whiskers");
                print("Can reassign different derived types to base type variable");
                return 0;
            }
        "#;

        assert!(type_check(input).is_ok());
    }
}