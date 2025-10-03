use crate::errors::RuntimeError;
use crate::types::{PlatValue, PlatString};

/// Runtime functions and builtins
pub struct Runtime;

impl Runtime {
    /// Initialize the runtime system and GC
    pub fn initialize() -> Self {
        // The gc crate initializes automatically, no need for explicit init
        Runtime
    }

    /// Print a value to stdout
    pub fn print(&self, value: &PlatValue) {
        println!("{}", value);
    }

    /// String interpolation: replace ${expr} with evaluated expressions
    pub fn interpolate_string(&self, template: &str, values: &[PlatValue]) -> PlatString {
        let mut result = template.to_string();

        // Simple interpolation - in a real implementation, this would be more sophisticated
        for (i, value) in values.iter().enumerate() {
            let placeholder = format!("${{{}}}", i);
            result = result.replace(&placeholder, &value.to_string());
        }

        PlatString::new(result)
    }

    /// Arithmetic operations
    pub fn add(&self, left: &PlatValue, right: &PlatValue) -> Result<PlatValue, RuntimeError> {
        match (left, right) {
            (PlatValue::I32(a), PlatValue::I32(b)) => Ok(PlatValue::I32(a + b)),
            (PlatValue::I64(a), PlatValue::I64(b)) => Ok(PlatValue::I64(a + b)),
            (PlatValue::String(a), PlatValue::String(b)) => Ok(PlatValue::String(a.concat(b))),
            _ => Err(RuntimeError::TypeMismatch(format!("Cannot add {:?} and {:?}", left, right))),
        }
    }

    pub fn subtract(&self, left: &PlatValue, right: &PlatValue) -> Result<PlatValue, RuntimeError> {
        match (left, right) {
            (PlatValue::I32(a), PlatValue::I32(b)) => Ok(PlatValue::I32(a - b)),
            (PlatValue::I64(a), PlatValue::I64(b)) => Ok(PlatValue::I64(a - b)),
            _ => Err(RuntimeError::TypeMismatch(format!("Cannot subtract {:?} and {:?}", left, right))),
        }
    }

    pub fn multiply(&self, left: &PlatValue, right: &PlatValue) -> Result<PlatValue, RuntimeError> {
        match (left, right) {
            (PlatValue::I32(a), PlatValue::I32(b)) => Ok(PlatValue::I32(a * b)),
            (PlatValue::I64(a), PlatValue::I64(b)) => Ok(PlatValue::I64(a * b)),
            _ => Err(RuntimeError::TypeMismatch(format!("Cannot multiply {:?} and {:?}", left, right))),
        }
    }

    pub fn divide(&self, left: &PlatValue, right: &PlatValue) -> Result<PlatValue, RuntimeError> {
        match (left, right) {
            (PlatValue::I32(a), PlatValue::I32(b)) => {
                if *b == 0 {
                    Err(RuntimeError::DivisionByZero)
                } else {
                    Ok(PlatValue::I32(a / b))
                }
            },
            (PlatValue::I64(a), PlatValue::I64(b)) => {
                if *b == 0 {
                    Err(RuntimeError::DivisionByZero)
                } else {
                    Ok(PlatValue::I64(a / b))
                }
            },
            _ => Err(RuntimeError::TypeMismatch(format!("Cannot divide {:?} and {:?}", left, right))),
        }
    }

    pub fn modulo(&self, left: &PlatValue, right: &PlatValue) -> Result<PlatValue, RuntimeError> {
        match (left, right) {
            (PlatValue::I32(a), PlatValue::I32(b)) => {
                if *b == 0 {
                    Err(RuntimeError::DivisionByZero)
                } else {
                    Ok(PlatValue::I32(a % b))
                }
            },
            (PlatValue::I64(a), PlatValue::I64(b)) => {
                if *b == 0 {
                    Err(RuntimeError::DivisionByZero)
                } else {
                    Ok(PlatValue::I64(a % b))
                }
            },
            _ => Err(RuntimeError::TypeMismatch(format!("Cannot modulo {:?} and {:?}", left, right))),
        }
    }

    /// Comparison operations
    pub fn equal(&self, left: &PlatValue, right: &PlatValue) -> PlatValue {
        PlatValue::Bool(left == right)
    }

    pub fn not_equal(&self, left: &PlatValue, right: &PlatValue) -> PlatValue {
        PlatValue::Bool(left != right)
    }

    pub fn less_than(&self, left: &PlatValue, right: &PlatValue) -> Result<PlatValue, RuntimeError> {
        match (left, right) {
            (PlatValue::I32(a), PlatValue::I32(b)) => Ok(PlatValue::Bool(a < b)),
            (PlatValue::I64(a), PlatValue::I64(b)) => Ok(PlatValue::Bool(a < b)),
            _ => Err(RuntimeError::TypeMismatch(format!("Cannot compare {:?} and {:?}", left, right))),
        }
    }

    pub fn less_than_or_equal(&self, left: &PlatValue, right: &PlatValue) -> Result<PlatValue, RuntimeError> {
        match (left, right) {
            (PlatValue::I32(a), PlatValue::I32(b)) => Ok(PlatValue::Bool(a <= b)),
            (PlatValue::I64(a), PlatValue::I64(b)) => Ok(PlatValue::Bool(a <= b)),
            _ => Err(RuntimeError::TypeMismatch(format!("Cannot compare {:?} and {:?}", left, right))),
        }
    }

    pub fn greater_than(&self, left: &PlatValue, right: &PlatValue) -> Result<PlatValue, RuntimeError> {
        match (left, right) {
            (PlatValue::I32(a), PlatValue::I32(b)) => Ok(PlatValue::Bool(a > b)),
            (PlatValue::I64(a), PlatValue::I64(b)) => Ok(PlatValue::Bool(a > b)),
            _ => Err(RuntimeError::TypeMismatch(format!("Cannot compare {:?} and {:?}", left, right))),
        }
    }

    pub fn greater_than_or_equal(&self, left: &PlatValue, right: &PlatValue) -> Result<PlatValue, RuntimeError> {
        match (left, right) {
            (PlatValue::I32(a), PlatValue::I32(b)) => Ok(PlatValue::Bool(a >= b)),
            (PlatValue::I64(a), PlatValue::I64(b)) => Ok(PlatValue::Bool(a >= b)),
            _ => Err(RuntimeError::TypeMismatch(format!("Cannot compare {:?} and {:?}", left, right))),
        }
    }

    /// Logical operations
    pub fn logical_and(&self, left: &PlatValue, right: &PlatValue) -> Result<PlatValue, RuntimeError> {
        match (left, right) {
            (PlatValue::Bool(a), PlatValue::Bool(b)) => Ok(PlatValue::Bool(*a && *b)),
            _ => Err(RuntimeError::TypeMismatch(format!("Logical and requires boolean operands, got {:?} and {:?}", left, right))),
        }
    }

    pub fn logical_or(&self, left: &PlatValue, right: &PlatValue) -> Result<PlatValue, RuntimeError> {
        match (left, right) {
            (PlatValue::Bool(a), PlatValue::Bool(b)) => Ok(PlatValue::Bool(*a || *b)),
            _ => Err(RuntimeError::TypeMismatch(format!("Logical or requires boolean operands, got {:?} and {:?}", left, right))),
        }
    }

    pub fn logical_not(&self, operand: &PlatValue) -> Result<PlatValue, RuntimeError> {
        match operand {
            PlatValue::Bool(b) => Ok(PlatValue::Bool(!b)),
            _ => Err(RuntimeError::TypeMismatch(format!("Logical not requires boolean operand, got {:?}", operand))),
        }
    }

    /// Unary negation
    pub fn negate(&self, operand: &PlatValue) -> Result<PlatValue, RuntimeError> {
        match operand {
            PlatValue::I32(i) => Ok(PlatValue::I32(-i)),
            PlatValue::I64(i) => Ok(PlatValue::I64(-i)),
            _ => Err(RuntimeError::TypeMismatch(format!("Cannot negate {:?}", operand))),
        }
    }

    /// Force garbage collection
    pub fn gc_collect(&self) {
        gc::force_collect();
    }

    /// Get statistics about GC
    pub fn gc_stats(&self) -> (usize, usize) {
        // Return (allocated_bytes, collected_objects) - mock implementation for now
        // The gc crate doesn't expose detailed stats
        (0, 0)
    }
}
