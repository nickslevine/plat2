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
                print(value = "Hello, world!");
            }
        "#;

        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_main_function_required() {
        let input = r#"
            fn hello() {
                print(value = "Hello!");
            }
        "#;

        let result = type_check(input);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("main function"));
    }

    #[test]
    fn test_main_function_wrong_signature() {
        let input = r#"
            fn main(x: Int32) {
                print(value = "Hello!");
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
                let x: Int32 = 42;
                let y: Bool = true;
                let z: String = "hello";
            }
        "#;

        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_let_variable_explicit_type() {
        let input = r#"
            fn main() {
                let x: Int32 = 42;
                let y: Bool = true;
                let z: String = "hello";
            }
        "#;

        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_let_variable_type_mismatch() {
        let input = r#"
            fn main() {
                let x: Int32 = true;
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
                let x: Int32 = 5;
                let x: Int32 = 10;
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
                print(value = unknown_var);
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
                var x: Int32 = 5;
                x = 10;
            }
        "#;

        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_var_assignment_type_mismatch() {
        let input = r#"
            fn main() {
                var x: Int32 = 5;
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
                let a: Int32 = 5 + 3;
                let b: Int32 = 10 - 2;
                let c: Int32 = 4 * 7;
                let d: Int32 = 20 / 5;
                let e: Int32 = 17 % 3;
            }
        "#;

        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_arithmetic_type_mismatch() {
        let input = r#"
            fn main() {
                let x: Int32 = 5 + true;
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
                let a: Bool = true and false;
                let b: Bool = true or false;
                let c: Bool = not true;
            }
        "#;

        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_boolean_type_mismatch() {
        let input = r#"
            fn main() {
                let x: Bool = 5 and true;
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
                let a: Bool = 5 < 10;
                let b: Bool = 5 <= 5;
                let c: Bool = 10 > 5;
                let d: Bool = 10 >= 10;
                let e: Bool = 5 == 5;
                let f: Bool = 5 != 3;
            }
        "#;

        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_comparison_type_mismatch() {
        let input = r#"
            fn main() {
                let x: Bool = 5 < "hello";
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
                let greeting: String = "Hello, " + "world!";
            }
        "#;

        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_if_statement() {
        let input = r#"
            fn main() {
                if (true) {
                    print(value = "yes");
                } else {
                    print(value = "no");
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
                    print(value = "yes");
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
                var x: Int32 = 0;
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
                    print(value = "loop");
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
            fn add(x: Int32, y: Int32) -> Int32 {
                return x + y;
            }

            fn main() {
                let result: Int32 = add(x = 5, y = 3);
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
            fn add(x: Int32, y: Int32) -> Int32 {
                return x + y;
            }

            fn main() {
                let result: Int32 = add(x = 5);
            }
        "#;

        let result = type_check(input);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("expects 2 arguments"));
    }

    #[test]
    fn test_function_call_wrong_arg_types() {
        let input = r#"
            fn add(x: Int32, y: Int32) -> Int32 {
                return x + y;
            }

            fn main() {
                let result: Int32 = add(x = 5, y = "hello");
            }
        "#;

        let result = type_check(input);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("parameter") && err_msg.contains("expects type"));
    }

    #[test]
    fn test_return_type_checking() {
        let input = r#"
            fn get_number() -> Int32 {
                return 42;
            }

            fn main() {
                let x: Int32 = get_number();
            }
        "#;

        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_return_type_mismatch() {
        let input = r#"
            fn get_number() -> Int32 {
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
            fn my_func() {
            }

            fn my_func() {
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
                let x: Int32 = 5;
                if (true) {
                    let y: Int32 = 10;
                    let z: Int32 = x + y; // x is visible from outer scope
                }
                // y is not visible here
            }
        "#;

        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_parameter_scoping() {
        let input = r#"
            fn my_func(x: Int32, y: Int32) -> Int32 {
                let z: Int32 = x + y;
                return z;
            }

            fn main() {
                let result: Int32 = my_func(x = 5, y = 10);
            }
        "#;

        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_duplicate_parameters() {
        let input = r#"
            fn my_func(x: Int32, x: Int32) -> Int32 {
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
                let name: String = "World";
                let greeting: String = "Hello, ${name}!";
                print(value = greeting);
            }
        "#;

        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_print_different_types() {
        let input = r#"
            fn main() {
                print(value = 42);
                print(value = true);
                print(value = "hello");
            }
        "#;

        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_unary_negation() {
        let input = r#"
            fn main() {
                let x: Int32 = -5;
                let y: Int32 = -(-10);
            }
        "#;

        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_unary_negation_wrong_type() {
        let input = r#"
            fn main() {
                let x: Bool = -true;
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
                let x: Bool = not 5;
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
                let numbers: List[Int32] = [1, 2, 3, 4, 5];
                for (num: Int32 in numbers) {
                    print(value = num);
                }
            }
        "#;

        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_for_loop_non_array_iterable() {
        let input = r#"
            fn main() {
                let x: Int32 = 42;
                for (item: Int32 in x) {
                    print(value = item);
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
                let numbers: List[Int32] = [1, 2, 3];
                for (num: Int32 in numbers) {
                    let doubled: Int32 = num * 2;
                    print(value = doubled);
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
                let num: Int32 = 42;
                let numbers: List[Int32] = [1, 2, 3];
                for (num: Int32 in numbers) {
                    print(value = num); // This shadows the outer 'num'
                }
                print(value = num); // This refers to the original 'num'
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
                let arr: List[Int32] = [1, 2, 3];
                for (x: Int32 in arr) {
                    if (x > 1) {
                        var y: Int32 = x * 2;
                        while (y > 0) {
                            y = y - 1;
                            if (y == 1) {
                                print(value = "found one");
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
                let items: List[Int32] = [10, 20, 30];
                for (item: Int32 in items) {
                    let result: Int32 = item + 5;
                    print(value = result);
                }
            }
        "#;

        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_for_loop_with_complex_expressions() {
        let input = r#"
            fn main() {
                let arrays: List[List[Int32]] = [[1, 2], [3, 4]];
                for (subarray: List[Int32] in arrays) {
                    let length: Int32 = subarray.len();
                    print(value = length);
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
                Move(Int32, Int32),
                Write(String)
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
                Move(Int32, Int32),
                Write(String)
            }

            fn main() {
                let msg1: Message = Message::Quit;
                let msg2: Message = Message::Move(field0 = 10, field1 = 20);
                let msg3: Message = Message::Write(field0 = "Hello");
            }
        "#;

        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_enum_constructor_wrong_args() {
        let input = r#"
            enum Message {
                Quit,
                Move(Int32, Int32),
                Write(String)
            }

            fn main() {
                let msg: Message = Message::Move(field0 = 10);
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
                Move(Int32, Int32),
                Write(String)
            }

            fn main() {
                let msg: Message = Message::Move(field0 = "hello", field1 = 20);
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
                Move(Int32, Int32)
            }

            fn main() {
                let msg: Message = Message::Unknown;
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
                Move(Int32, Int32),
                Write(String)
            }

            fn main() {
                let msg: Message = Message::Move(field0 = 10, field1 = 20);
                let result: Int32 = match msg {
                    Message::Quit -> 0,
                    Message::Move(x: Int32, y: Int32) -> x + y,
                    Message::Write(s: String) -> 100
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
                Move(Int32, Int32),
                Write(String)
            }

            fn main() {
                let msg: Message = Message::Move(field0 = 10, field1 = 20);
                let result: Int32 = match msg {
                    Message::Quit -> 0,
                    Message::Move(x: Int32, y: Int32) -> x + y
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
                Move(Int32, Int32)
            }

            fn main() {
                let msg: Message = Message::Move(field0 = 10, field1 = 20);
                let result: Int32 = match msg {
                    Message::Quit -> 0,
                    Message::Move(x: Int32, y: Int32) -> "hello"
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
                Move(Int32, Int32),
                Write(String)
            }

            fn main() {
                let msg: Message = Message::Write(field0 = "hello");
                let result: String = match msg {
                    Message::Move(x: Int32, y: Int32) -> x + y,
                    Message::Write(text: String) -> text
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
                Move(Int32, Int32)
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
                Move(Int32, Int32),

                fn is_quit() -> Bool {
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
                let species: String;

                init(species: String) -> Animal {
                    self.species = species;
                    return self;
                }

                virtual fn make_sound() -> String {
                    return "Generic animal sound";
                }
            }

            class Dog : Animal {
                let species: String;
                let breed: String;

                init(species: String, breed: String) -> Dog {
                    self.species = species;
                    self.breed = breed;
                    return self;
                }

                override fn make_sound() -> String {
                    return "Woof!";
                }
            }

            fn main() -> Int32 {
                let animal: Animal = Dog.init(species = "Canine", breed = "Golden");
                print(value = "Animal created");
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
                let name: String;

                init(name: String) -> Animal {
                    self.name = name;
                    return self;
                }
            }

            class Cat : Animal {
                let name: String;

                init(name: String) -> Cat {
                    self.name = name;
                    return self;
                }
            }

            fn main() -> Int32 {
                var animal: Animal = Cat.init(name = "Whiskers");
                print(value = "Cat created as Animal");
                return 0;
            }
        "#;

        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_polymorphic_assignment_transitive() {
        let input = r#"
            class Animal {
                let name: String;

                init(name: String) -> Animal {
                    self.name = name;
                    return self;
                }
            }

            class Mammal : Animal {
                let name: String;

                init(name: String) -> Mammal {
                    self.name = name;
                    return self;
                }
            }

            class Dog : Mammal {
                let name: String;

                init(name: String) -> Dog {
                    self.name = name;
                    return self;
                }
            }

            fn main() -> Int32 {
                let animal: Animal = Dog.init(name = "Rex");
                let mammal: Mammal = Dog.init(name = "Spot");
                print(value = "Transitive inheritance works!");
                return 0;
            }
        "#;

        assert!(type_check(input).is_ok());
    }

    #[test]
    fn test_polymorphic_assignment_field() {
        let input = r#"
            class Animal {
                let name: String;

                init(name: String) -> Animal {
                    self.name = name;
                    return self;
                }
            }

            class Dog : Animal {
                let name: String;

                init(name: String) -> Dog {
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

            fn main() -> Int32 {
                let dog: Dog = Dog.init(name = "Buddy");
                let container: AnimalContainer = AnimalContainer.init(animal = dog);
                print(value = "Dog stored in Animal field");
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
                let name: String;

                init(name: String) -> Animal {
                    self.name = name;
                    return self;
                }
            }

            class Dog : Animal {
                let name: String;

                init(name: String) -> Dog {
                    self.name = name;
                    return self;
                }
            }

            fn main() -> Int32 {
                let dog: Dog = Animal.init(name = "Generic");
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
                let name: String;

                init(name: String) -> Animal {
                    self.name = name;
                    return self;
                }
            }

            class Vehicle {
                let model: String;

                init(model: String) -> Vehicle {
                    self.model = model;
                    return self;
                }
            }

            fn main() -> Int32 {
                let animal: Animal = Vehicle.init(model = "Car");
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
                let name: String;

                init(name: String) -> Animal {
                    self.name = name;
                    return self;
                }
            }

            class Dog : Animal {
                let name: String;

                init(name: String) -> Dog {
                    self.name = name;
                    return self;
                }
            }

            class Cat : Animal {
                let name: String;

                init(name: String) -> Cat {
                    self.name = name;
                    return self;
                }
            }

            fn main() -> Int32 {
                var animal: Animal = Dog.init(name = "Buddy");
                animal = Cat.init(name = "Whiskers");
                print(value = "Can reassign different derived types to base type variable");
                return 0;
            }
        "#;

        assert!(type_check(input).is_ok());
    }
}