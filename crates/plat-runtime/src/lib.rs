#[cfg(test)]
mod tests;

use std::fmt;
use std::ffi::CStr;
use std::os::raw::c_char;

/// Runtime value types for the Plat language
#[derive(Debug, Clone, PartialEq)]
pub enum PlatValue {
    Bool(bool),
    I32(i32),
    I64(i64),
    String(PlatString),
    Unit,
}

/// GC-managed string type
/// For now, this is just a wrapper around Rust's String
/// In the future, this will integrate with Boehm GC
#[derive(Debug, Clone, PartialEq)]
pub struct PlatString {
    data: String,
}

impl PlatString {
    pub fn new(s: String) -> Self {
        Self { data: s }
    }

    pub fn from_str(s: &str) -> Self {
        Self { data: s.to_string() }
    }

    pub fn as_str(&self) -> &str {
        &self.data
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// String concatenation
    pub fn concat(&self, other: &PlatString) -> PlatString {
        PlatString::new(format!("{}{}", self.data, other.data))
    }
}

impl fmt::Display for PlatString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.data)
    }
}

impl fmt::Display for PlatValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlatValue::Bool(b) => write!(f, "{}", b),
            PlatValue::I32(i) => write!(f, "{}", i),
            PlatValue::I64(i) => write!(f, "{}", i),
            PlatValue::String(s) => write!(f, "{}", s),
            PlatValue::Unit => write!(f, "()"),
        }
    }
}

impl From<bool> for PlatValue {
    fn from(b: bool) -> Self {
        PlatValue::Bool(b)
    }
}

impl From<i32> for PlatValue {
    fn from(i: i32) -> Self {
        PlatValue::I32(i)
    }
}

impl From<i64> for PlatValue {
    fn from(i: i64) -> Self {
        PlatValue::I64(i)
    }
}

impl From<String> for PlatValue {
    fn from(s: String) -> Self {
        PlatValue::String(PlatString::new(s))
    }
}

impl From<&str> for PlatValue {
    fn from(s: &str) -> Self {
        PlatValue::String(PlatString::from_str(s))
    }
}

impl From<PlatString> for PlatValue {
    fn from(s: PlatString) -> Self {
        PlatValue::String(s)
    }
}

/// Runtime functions and builtins
pub struct Runtime;

impl Runtime {
    /// Initialize the runtime system
    /// In the future, this will initialize the Boehm GC
    pub fn initialize() -> Self {
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
}

/// Runtime errors
#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeError {
    TypeMismatch(String),
    DivisionByZero,
    UndefinedVariable(String),
    UndefinedFunction(String),
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuntimeError::TypeMismatch(msg) => write!(f, "Type mismatch: {}", msg),
            RuntimeError::DivisionByZero => write!(f, "Division by zero"),
            RuntimeError::UndefinedVariable(name) => write!(f, "Undefined variable: {}", name),
            RuntimeError::UndefinedFunction(name) => write!(f, "Undefined function: {}", name),
        }
    }
}

impl std::error::Error for RuntimeError {}

/// C-compatible print function that can be called from generated code
///
/// # Safety
/// This function is unsafe because it deals with raw pointers from generated code
#[no_mangle]
pub extern "C" fn plat_print(str_ptr: *const c_char) {
    if str_ptr.is_null() {
        println!("<null>");
        return;
    }

    unsafe {
        match CStr::from_ptr(str_ptr).to_str() {
            Ok(s) => println!("{}", s),
            Err(_) => println!("<invalid UTF-8>"),
        }
    }
}