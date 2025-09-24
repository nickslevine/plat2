#[cfg(test)]
mod tests {
    use crate::{PlatValue, PlatString, Runtime, RuntimeError};

    #[test]
    fn test_plat_value_creation() {
        let bool_val = PlatValue::from(true);
        let i32_val = PlatValue::from(42i32);
        let i64_val = PlatValue::from(42i64);
        let str_val = PlatValue::from("hello");
        let string_val = PlatValue::from("world".to_string());

        assert_eq!(bool_val, PlatValue::Bool(true));
        assert_eq!(i32_val, PlatValue::I32(42));
        assert_eq!(i64_val, PlatValue::I64(42));
        assert_eq!(str_val, PlatValue::String(PlatString::from_str("hello")));
        assert_eq!(string_val, PlatValue::String(PlatString::new("world".to_string())));
    }

    #[test]
    fn test_plat_string_operations() {
        let str1 = PlatString::from_str("Hello, ");
        let str2 = PlatString::from_str("world!");
        let concatenated = str1.concat(&str2);

        assert_eq!(concatenated.as_str(), "Hello, world!");
        assert_eq!(str1.len(), 7);
        assert_eq!(str2.len(), 6);
        assert!(!str1.is_empty());

        let empty_str = PlatString::from_str("");
        assert!(empty_str.is_empty());
    }

    #[test]
    fn test_display_formatting() {
        assert_eq!(PlatValue::Bool(true).to_string(), "true");
        assert_eq!(PlatValue::Bool(false).to_string(), "false");
        assert_eq!(PlatValue::I32(42).to_string(), "42");
        assert_eq!(PlatValue::I64(-123).to_string(), "-123");
        assert_eq!(PlatValue::String(PlatString::from_str("test")).to_string(), "test");
        assert_eq!(PlatValue::Unit.to_string(), "()");
    }

    #[test]
    fn test_runtime_arithmetic() {
        let runtime = Runtime::initialize();

        // Addition
        let result = runtime.add(&PlatValue::I32(5), &PlatValue::I32(3)).unwrap();
        assert_eq!(result, PlatValue::I32(8));

        let result = runtime.add(&PlatValue::I64(10), &PlatValue::I64(20)).unwrap();
        assert_eq!(result, PlatValue::I64(30));

        // String concatenation
        let str1 = PlatValue::String(PlatString::from_str("Hello, "));
        let str2 = PlatValue::String(PlatString::from_str("world!"));
        let result = runtime.add(&str1, &str2).unwrap();
        assert_eq!(result, PlatValue::String(PlatString::from_str("Hello, world!")));

        // Type mismatch error
        let result = runtime.add(&PlatValue::I32(5), &PlatValue::Bool(true));
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RuntimeError::TypeMismatch(_)));
    }

    #[test]
    fn test_runtime_subtraction() {
        let runtime = Runtime::initialize();

        let result = runtime.subtract(&PlatValue::I32(10), &PlatValue::I32(3)).unwrap();
        assert_eq!(result, PlatValue::I32(7));

        let result = runtime.subtract(&PlatValue::I64(100), &PlatValue::I64(25)).unwrap();
        assert_eq!(result, PlatValue::I64(75));

        // Type mismatch
        let result = runtime.subtract(&PlatValue::I32(5), &PlatValue::String(PlatString::from_str("test")));
        assert!(result.is_err());
    }

    #[test]
    fn test_runtime_multiplication() {
        let runtime = Runtime::initialize();

        let result = runtime.multiply(&PlatValue::I32(4), &PlatValue::I32(5)).unwrap();
        assert_eq!(result, PlatValue::I32(20));

        let result = runtime.multiply(&PlatValue::I64(7), &PlatValue::I64(8)).unwrap();
        assert_eq!(result, PlatValue::I64(56));
    }

    #[test]
    fn test_runtime_division() {
        let runtime = Runtime::initialize();

        let result = runtime.divide(&PlatValue::I32(20), &PlatValue::I32(4)).unwrap();
        assert_eq!(result, PlatValue::I32(5));

        let result = runtime.divide(&PlatValue::I64(100), &PlatValue::I64(10)).unwrap();
        assert_eq!(result, PlatValue::I64(10));

        // Division by zero
        let result = runtime.divide(&PlatValue::I32(5), &PlatValue::I32(0));
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RuntimeError::DivisionByZero));
    }

    #[test]
    fn test_runtime_modulo() {
        let runtime = Runtime::initialize();

        let result = runtime.modulo(&PlatValue::I32(17), &PlatValue::I32(5)).unwrap();
        assert_eq!(result, PlatValue::I32(2));

        let result = runtime.modulo(&PlatValue::I64(23), &PlatValue::I64(7)).unwrap();
        assert_eq!(result, PlatValue::I64(2));

        // Modulo by zero
        let result = runtime.modulo(&PlatValue::I32(5), &PlatValue::I32(0));
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RuntimeError::DivisionByZero));
    }

    #[test]
    fn test_runtime_comparisons() {
        let runtime = Runtime::initialize();

        // Equality
        let result = runtime.equal(&PlatValue::I32(5), &PlatValue::I32(5));
        assert_eq!(result, PlatValue::Bool(true));

        let result = runtime.equal(&PlatValue::I32(5), &PlatValue::I32(3));
        assert_eq!(result, PlatValue::Bool(false));

        let result = runtime.not_equal(&PlatValue::Bool(true), &PlatValue::Bool(false));
        assert_eq!(result, PlatValue::Bool(true));

        // Ordering
        let result = runtime.less_than(&PlatValue::I32(3), &PlatValue::I32(5)).unwrap();
        assert_eq!(result, PlatValue::Bool(true));

        let result = runtime.less_than(&PlatValue::I32(5), &PlatValue::I32(3)).unwrap();
        assert_eq!(result, PlatValue::Bool(false));

        let result = runtime.greater_than(&PlatValue::I64(10), &PlatValue::I64(5)).unwrap();
        assert_eq!(result, PlatValue::Bool(true));

        let result = runtime.less_than_or_equal(&PlatValue::I32(5), &PlatValue::I32(5)).unwrap();
        assert_eq!(result, PlatValue::Bool(true));

        let result = runtime.greater_than_or_equal(&PlatValue::I32(5), &PlatValue::I32(3)).unwrap();
        assert_eq!(result, PlatValue::Bool(true));

        // Type mismatch
        let result = runtime.less_than(&PlatValue::I32(5), &PlatValue::String(PlatString::from_str("test")));
        assert!(result.is_err());
    }

    #[test]
    fn test_runtime_logical_operations() {
        let runtime = Runtime::initialize();

        // Logical AND
        let result = runtime.logical_and(&PlatValue::Bool(true), &PlatValue::Bool(true)).unwrap();
        assert_eq!(result, PlatValue::Bool(true));

        let result = runtime.logical_and(&PlatValue::Bool(true), &PlatValue::Bool(false)).unwrap();
        assert_eq!(result, PlatValue::Bool(false));

        let result = runtime.logical_and(&PlatValue::Bool(false), &PlatValue::Bool(false)).unwrap();
        assert_eq!(result, PlatValue::Bool(false));

        // Logical OR
        let result = runtime.logical_or(&PlatValue::Bool(true), &PlatValue::Bool(false)).unwrap();
        assert_eq!(result, PlatValue::Bool(true));

        let result = runtime.logical_or(&PlatValue::Bool(false), &PlatValue::Bool(false)).unwrap();
        assert_eq!(result, PlatValue::Bool(false));

        // Logical NOT
        let result = runtime.logical_not(&PlatValue::Bool(true)).unwrap();
        assert_eq!(result, PlatValue::Bool(false));

        let result = runtime.logical_not(&PlatValue::Bool(false)).unwrap();
        assert_eq!(result, PlatValue::Bool(true));

        // Type mismatch
        let result = runtime.logical_and(&PlatValue::I32(5), &PlatValue::Bool(true));
        assert!(result.is_err());

        let result = runtime.logical_not(&PlatValue::I32(5));
        assert!(result.is_err());
    }

    #[test]
    fn test_runtime_negation() {
        let runtime = Runtime::initialize();

        let result = runtime.negate(&PlatValue::I32(5)).unwrap();
        assert_eq!(result, PlatValue::I32(-5));

        let result = runtime.negate(&PlatValue::I32(-3)).unwrap();
        assert_eq!(result, PlatValue::I32(3));

        let result = runtime.negate(&PlatValue::I64(42)).unwrap();
        assert_eq!(result, PlatValue::I64(-42));

        // Type mismatch
        let result = runtime.negate(&PlatValue::Bool(true));
        assert!(result.is_err());
    }

    #[test]
    fn test_string_interpolation() {
        let runtime = Runtime::initialize();

        let values = vec![
            PlatValue::from("World"),
            PlatValue::from(42i32),
        ];

        let result = runtime.interpolate_string("Hello, ${0}! The answer is ${1}.", &values);
        assert_eq!(result.as_str(), "Hello, World! The answer is 42.");

        // Empty interpolation
        let result = runtime.interpolate_string("No interpolation here", &[]);
        assert_eq!(result.as_str(), "No interpolation here");
    }

    #[test]
    fn test_print_function() {
        let runtime = Runtime::initialize();

        // Test that print doesn't panic (output goes to stdout)
        runtime.print(&PlatValue::Bool(true));
        runtime.print(&PlatValue::I32(42));
        runtime.print(&PlatValue::String(PlatString::from_str("Hello, world!")));
        runtime.print(&PlatValue::Unit);
    }

    #[test]
    fn test_runtime_error_display() {
        let error = RuntimeError::TypeMismatch("test error".to_string());
        assert_eq!(error.to_string(), "Type mismatch: test error");

        let error = RuntimeError::DivisionByZero;
        assert_eq!(error.to_string(), "Division by zero");

        let error = RuntimeError::UndefinedVariable("x".to_string());
        assert_eq!(error.to_string(), "Undefined variable: x");

        let error = RuntimeError::UndefinedFunction("foo".to_string());
        assert_eq!(error.to_string(), "Undefined function: foo");
    }

    #[test]
    fn test_value_equality() {
        assert_eq!(PlatValue::Bool(true), PlatValue::Bool(true));
        assert_ne!(PlatValue::Bool(true), PlatValue::Bool(false));

        assert_eq!(PlatValue::I32(42), PlatValue::I32(42));
        assert_ne!(PlatValue::I32(42), PlatValue::I32(43));

        assert_eq!(PlatValue::I64(100), PlatValue::I64(100));
        assert_ne!(PlatValue::I64(100), PlatValue::I64(101));

        let str1 = PlatValue::String(PlatString::from_str("test"));
        let str2 = PlatValue::String(PlatString::from_str("test"));
        let str3 = PlatValue::String(PlatString::from_str("other"));

        assert_eq!(str1, str2);
        assert_ne!(str1, str3);

        assert_eq!(PlatValue::Unit, PlatValue::Unit);

        // Different types should not be equal
        assert_ne!(PlatValue::I32(5), PlatValue::I64(5));
        assert_ne!(PlatValue::Bool(true), PlatValue::I32(1));
    }
}