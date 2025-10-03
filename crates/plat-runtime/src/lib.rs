#[cfg(test)]
mod tests;

use std::fmt;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use gc::{Gc, Trace, Finalize};

/// Runtime value types for the Plat language
#[derive(Debug, Clone, PartialEq, Trace, Finalize)]
pub enum PlatValue {
    Bool(bool),
    I32(i32),
    I64(i64),
    String(PlatString),
    Array(PlatArray),
    Dict(PlatDict),
    Set(PlatSet),
    Class(PlatClass),
    Unit,
}

/// GC-managed string type using gc crate
#[derive(Debug, Clone, Trace, Finalize)]
pub struct PlatString {
    data: Gc<String>,
}

impl PartialEq for PlatString {
    fn eq(&self, other: &Self) -> bool {
        self.data.as_str() == other.data.as_str()
    }
}

impl PlatString {
    pub fn new(s: String) -> Self {
        Self { data: Gc::new(s) }
    }

    pub fn from_str(s: &str) -> Self {
        Self { data: Gc::new(s.to_string()) }
    }

    pub fn as_str(&self) -> &str {
        self.data.as_str()
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// String concatenation
    pub fn concat(&self, other: &PlatString) -> PlatString {
        PlatString::new(format!("{}{}", self.data.as_str(), other.data.as_str()))
    }
}

impl fmt::Display for PlatString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.data.as_str())
    }
}

/// GC-managed homogeneous array type
#[derive(Debug, Clone, Trace, Finalize)]
pub struct PlatArray {
    data: Gc<Vec<i32>>, // For now, only support i32 arrays (we can extend later)
    length: usize,
}

impl PartialEq for PlatArray {
    fn eq(&self, other: &Self) -> bool {
        self.data.as_ref() == other.data.as_ref()
    }
}

impl PlatArray {
    pub fn new_i32(elements: Vec<i32>) -> Self {
        let length = elements.len();
        Self {
            data: Gc::new(elements),
            length,
        }
    }

    pub fn len(&self) -> usize {
        self.length
    }

    pub fn is_empty(&self) -> bool {
        self.length == 0
    }

    pub fn get(&self, index: usize) -> Option<i32> {
        self.data.as_ref().get(index).copied()
    }

    pub fn as_slice(&self) -> &[i32] {
        self.data.as_ref().as_slice()
    }
}

/// GC-managed dictionary type (using vector of key-value pairs for simplicity)
#[derive(Debug, Clone, Trace, Finalize)]
pub struct PlatDict {
    data: Gc<Vec<(String, PlatValue)>>,
}

impl PartialEq for PlatDict {
    fn eq(&self, other: &Self) -> bool {
        self.data.as_ref() == other.data.as_ref()
    }
}

impl PlatDict {
    pub fn new() -> Self {
        Self {
            data: Gc::new(Vec::new()),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            data: Gc::new(Vec::with_capacity(capacity)),
        }
    }

    pub fn from_pairs(pairs: Vec<(String, PlatValue)>) -> Self {
        Self {
            data: Gc::new(pairs),
        }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn get(&self, key: &str) -> Option<&PlatValue> {
        self.data.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.data.iter().map(|(k, _)| k)
    }

    pub fn values(&self) -> impl Iterator<Item = &PlatValue> {
        self.data.iter().map(|(_, v)| v)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &PlatValue)> {
        self.data.iter().map(|(k, v)| (k, v))
    }
}

impl fmt::Display for PlatDict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{")?;
        for (i, (key, value)) in self.data.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "\"{}\": {}", key, value)?;
        }
        write!(f, "}}")
    }
}

/// GC-managed set type (using vector for simplicity, maintaining uniqueness)
#[derive(Debug, Clone, Trace, Finalize)]
pub struct PlatSet {
    data: Gc<Vec<PlatValue>>,
}

impl PartialEq for PlatSet {
    fn eq(&self, other: &Self) -> bool {
        self.data.as_ref() == other.data.as_ref()
    }
}

impl PlatSet {
    pub fn new() -> Self {
        Self {
            data: Gc::new(Vec::new()),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            data: Gc::new(Vec::with_capacity(capacity)),
        }
    }

    pub fn insert(&mut self, value: PlatValue) {
        // Only insert if value is not already present
        if !self.data.contains(&value) {
            unsafe {
                let ptr = Gc::into_raw(self.data.clone()) as *mut Vec<PlatValue>;
                (*ptr).push(value);
                self.data = Gc::from_raw(ptr);
            }
        }
    }

    pub fn contains(&self, value: &PlatValue) -> bool {
        self.data.contains(value)
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &PlatValue> {
        self.data.iter()
    }

    pub fn as_slice(&self) -> &[PlatValue] {
        self.data.as_ref().as_slice()
    }
}

impl fmt::Display for PlatSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{")?;
        for (i, value) in self.data.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}", value)?;
        }
        write!(f, "}}")
    }
}

/// GC-managed class object for storing class instances with their fields
#[derive(Debug, Clone, Trace, Finalize)]
pub struct PlatClass {
    pub class_name: String,
    pub fields: Gc<std::collections::HashMap<String, PlatValue>>,
}

impl PartialEq for PlatClass {
    fn eq(&self, other: &Self) -> bool {
        self.class_name == other.class_name &&
        self.fields.as_ref() == other.fields.as_ref()
    }
}

impl PlatClass {
    pub fn new(class_name: String) -> Self {
        Self {
            class_name,
            fields: Gc::new(std::collections::HashMap::new()),
        }
    }

    pub fn get_field(&self, field_name: &str) -> Option<PlatValue> {
        self.fields.get(field_name).cloned()
    }

    pub fn set_field(&mut self, field_name: String, value: PlatValue) {
        unsafe {
            let ptr = Gc::into_raw(self.fields.clone()) as *mut std::collections::HashMap<String, PlatValue>;
            (*ptr).insert(field_name, value);
            self.fields = Gc::from_raw(ptr);
        }
    }
}

impl fmt::Display for PlatClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{{", self.class_name)?;
        for (i, (key, value)) in self.fields.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}: {}", key, value)?;
        }
        write!(f, "}}")
    }
}

impl fmt::Display for PlatValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlatValue::Bool(b) => write!(f, "{}", b),
            PlatValue::I32(i) => write!(f, "{}", i),
            PlatValue::I64(i) => write!(f, "{}", i),
            PlatValue::String(s) => write!(f, "{}", s),
            PlatValue::Array(arr) => {
                write!(f, "[")?;
                for (i, elem) in arr.as_slice().iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", elem)?;
                }
                write!(f, "]")
            }
            PlatValue::Dict(dict) => write!(f, "{}", dict),
            PlatValue::Set(set) => write!(f, "{}", set),
            PlatValue::Class(class) => write!(f, "{}", class),
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

impl From<PlatArray> for PlatValue {
    fn from(arr: PlatArray) -> Self {
        PlatValue::Array(arr)
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

impl From<PlatDict> for PlatValue {
    fn from(dict: PlatDict) -> Self {
        PlatValue::Dict(dict)
    }
}

impl From<PlatSet> for PlatValue {
    fn from(set: PlatSet) -> Self {
        PlatValue::Set(set)
    }
}

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
            Ok(s) => {
                println!("{}", s);
            }
            Err(_) => {
                println!("<invalid UTF-8>");
            }
        }
    }
}

/// C-compatible assert function for testing
///
/// # Arguments
/// * `condition` - Boolean condition to check
/// * `message_ptr` - Pointer to optional error message (can be null)
///
/// # Safety
/// This function is unsafe because it dereferences raw pointers
#[no_mangle]
pub extern "C" fn plat_assert(condition: bool, message_ptr: *const c_char) {
    if !condition {
        let message = if message_ptr.is_null() {
            "Assertion failed".to_string()
        } else {
            unsafe {
                CStr::from_ptr(message_ptr)
                    .to_str()
                    .unwrap_or("Assertion failed (invalid UTF-8 in message)")
                    .to_string()
            }
        };

        eprintln!("✗ {}", message);
        std::process::exit(1);
    }
}

/// C-compatible GC allocation function that can be called from generated code
///
/// # Safety
/// This function is unsafe because it returns raw pointers to GC memory
#[no_mangle]
pub extern "C" fn plat_gc_alloc(size: usize) -> *mut u8 {
    // Temporary fix: use simple heap allocation instead of GC
    // TODO: Replace with proper GC allocation once the issue is resolved
    let layout = std::alloc::Layout::from_size_align(size, 1).unwrap();
    let ptr = unsafe { std::alloc::alloc_zeroed(layout) };

    if ptr.is_null() {
        return std::ptr::null_mut();
    }

    ptr
}

/// C-compatible GC collection function that can be called from generated code
#[no_mangle]
pub extern "C" fn plat_gc_collect() {
    gc::force_collect();
}

/// C-compatible function to get GC stats (mock)
#[no_mangle]
pub extern "C" fn plat_gc_stats() -> usize {
    // The gc crate doesn't expose detailed stats
    0
}

/// Convert an i32 to a C string (null-terminated) on the GC heap
///
/// # Safety
/// This function returns a raw pointer to GC memory
#[no_mangle]
pub extern "C" fn plat_i32_to_string(value: i32) -> *const c_char {
    let string_repr = value.to_string();
    let mut bytes = string_repr.into_bytes();
    bytes.push(0); // null terminator

    // Allocate on GC heap
    let size = bytes.len();
    let gc_ptr = plat_gc_alloc(size);

    if gc_ptr.is_null() {
        return std::ptr::null();
    }

    // Copy string data to GC memory
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), gc_ptr, size);
    }

    gc_ptr as *const c_char
}

/// Convert an i64 to a C string (null-terminated) on the GC heap
///
/// # Safety
/// This function returns a raw pointer to GC memory
#[no_mangle]
pub extern "C" fn plat_i64_to_string(value: i64) -> *const c_char {
    let string_repr = value.to_string();
    let mut bytes = string_repr.into_bytes();
    bytes.push(0); // null terminator

    // Allocate on GC heap
    let size = bytes.len();
    let gc_ptr = plat_gc_alloc(size);

    if gc_ptr.is_null() {
        return std::ptr::null();
    }

    // Copy string data to GC memory
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), gc_ptr, size);
    }

    gc_ptr as *const c_char
}

/// Convert a bool to a C string (null-terminated) on the GC heap
///
/// # Safety
/// This function returns a raw pointer to GC memory
#[no_mangle]
pub extern "C" fn plat_bool_to_string(value: bool) -> *const c_char {
    let string_repr = if value { "true" } else { "false" };
    let mut bytes = string_repr.as_bytes().to_vec();
    bytes.push(0); // null terminator

    // Allocate on GC heap
    let size = bytes.len();
    let gc_ptr = plat_gc_alloc(size);

    if gc_ptr.is_null() {
        return std::ptr::null();
    }

    // Copy string data to GC memory
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), gc_ptr, size);
    }

    gc_ptr as *const c_char
}

/// Convert an f32 to a C string (null-terminated) on the GC heap
///
/// # Safety
/// This function returns a raw pointer to GC memory
#[no_mangle]
pub extern "C" fn plat_f32_to_string(value: f32) -> *const c_char {
    let string_repr = value.to_string();
    let mut bytes = string_repr.into_bytes();
    bytes.push(0); // null terminator

    // Allocate on GC heap
    let size = bytes.len();
    let gc_ptr = plat_gc_alloc(size);

    if gc_ptr.is_null() {
        return std::ptr::null();
    }

    // Copy string data to GC memory
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), gc_ptr, size);
    }

    gc_ptr as *const c_char
}

/// Convert an f64 to a C string (null-terminated) on the GC heap
///
/// # Safety
/// This function returns a raw pointer to GC memory
#[no_mangle]
pub extern "C" fn plat_f64_to_string(value: f64) -> *const c_char {
    let string_repr = value.to_string();
    let mut bytes = string_repr.into_bytes();
    bytes.push(0); // null terminator

    // Allocate on GC heap
    let size = bytes.len();
    let gc_ptr = plat_gc_alloc(size);

    if gc_ptr.is_null() {
        return std::ptr::null();
    }

    // Copy string data to GC memory
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), gc_ptr, size);
    }

    gc_ptr as *const c_char
}

/// Perform string interpolation by replacing ${N} placeholders with values
///
/// # Safety
/// This function takes raw pointers and returns raw pointers to GC memory
#[no_mangle]
pub extern "C" fn plat_string_interpolate(
    template_ptr: *const c_char,
    values_ptr: *const *const c_char,
    values_count: usize
) -> *const c_char {
    if template_ptr.is_null() {
        return std::ptr::null();
    }

    let template = unsafe {
        match CStr::from_ptr(template_ptr).to_str() {
            Ok(s) => s,
            Err(_) => {
                return std::ptr::null();
            }
        }
    };

    let mut result = template.to_string();

    // Replace ${N} placeholders with actual values
    for i in 0..values_count {
        let placeholder = format!("${{{}}}", i);

        if !values_ptr.is_null() {
            let value_ptr = unsafe { *values_ptr.add(i) };

            if !value_ptr.is_null() {
                let value_str = unsafe {
                    match CStr::from_ptr(value_ptr).to_str() {
                        Ok(s) => s,
                        Err(_) => "<invalid>",
                    }
                };

                result = result.replace(&placeholder, value_str);
            }
        }
    }

    // Allocate result on GC heap
    let mut result_bytes = result.into_bytes();
    result_bytes.push(0); // null terminator

    let size = result_bytes.len();
    let gc_ptr = plat_gc_alloc(size);

    if gc_ptr.is_null() {
        return std::ptr::null();
    }

    // Copy result to GC memory
    unsafe {
        std::ptr::copy_nonoverlapping(result_bytes.as_ptr(), gc_ptr, size);
    }

    gc_ptr as *const c_char
}

// Array element type constants
pub const ARRAY_TYPE_I32: u8 = 0;
pub const ARRAY_TYPE_I64: u8 = 1;
pub const ARRAY_TYPE_BOOL: u8 = 2;
pub const ARRAY_TYPE_STRING: u8 = 3;
pub const ARRAY_TYPE_CLASS: u8 = 4; // Custom class pointers (8 bytes like strings)

/// Array structure for runtime (C-compatible)
/// Generic data pointer that can hold any type
#[repr(C)]
pub struct RuntimeArray {
    data: *mut u8, // Generic byte pointer for any type
    length: usize,
    capacity: usize,
    element_size: usize, // Size of each element in bytes
    element_type: u8, // Type discriminant: 0=i32, 1=i64, 2=bool, 3=string
}

/// Create a new i32 array on the GC heap
#[no_mangle]
pub extern "C" fn plat_array_create_i32(elements: *const i32, count: usize) -> *mut RuntimeArray {
    create_typed_array(elements as *const u8, count, std::mem::size_of::<i32>(), ARRAY_TYPE_I32)
}

/// Create a new i64 array on the GC heap
#[no_mangle]
pub extern "C" fn plat_array_create_i64(elements: *const i64, count: usize) -> *mut RuntimeArray {
    create_typed_array(elements as *const u8, count, std::mem::size_of::<i64>(), ARRAY_TYPE_I64)
}

/// Create a new bool array on the GC heap
#[no_mangle]
pub extern "C" fn plat_array_create_bool(elements: *const bool, count: usize) -> *mut RuntimeArray {
    create_typed_array(elements as *const u8, count, std::mem::size_of::<bool>(), ARRAY_TYPE_BOOL)
}

/// Create a new string array on the GC heap
#[no_mangle]
pub extern "C" fn plat_array_create_string(elements: *const *const c_char, count: usize) -> *mut RuntimeArray {
    create_typed_array(elements as *const u8, count, std::mem::size_of::<*const c_char>(), ARRAY_TYPE_STRING)
}

/// Create a new class array on the GC heap (custom class pointers)
#[no_mangle]
pub extern "C" fn plat_array_create_class(elements: *const *const u8, count: usize) -> *mut RuntimeArray {
    create_typed_array(elements as *const u8, count, std::mem::size_of::<*const u8>(), ARRAY_TYPE_CLASS)
}

/// Generic array creation helper
fn create_typed_array(elements: *const u8, count: usize, element_size: usize, element_type: u8) -> *mut RuntimeArray {
    if elements.is_null() && count > 0 {
        return std::ptr::null_mut();
    }

    // Allocate the array struct
    let array_size = std::mem::size_of::<RuntimeArray>();
    let array_ptr = plat_gc_alloc(array_size) as *mut RuntimeArray;

    if array_ptr.is_null() {
        return std::ptr::null_mut();
    }

    // Allocate space for the data
    let data_size = count * element_size;
    let data_ptr = if count > 0 {
        plat_gc_alloc(data_size)
    } else {
        std::ptr::null_mut()
    };

    if count > 0 && data_ptr.is_null() {
        return std::ptr::null_mut();
    }

    // Copy elements to the data array
    if count > 0 {
        unsafe {
            std::ptr::copy_nonoverlapping(elements, data_ptr, data_size);
        }
    }

    // Initialize the array struct
    unsafe {
        (*array_ptr) = RuntimeArray {
            data: data_ptr,
            length: count,
            capacity: count,
            element_size,
            element_type,
        };
    }

    array_ptr
}

/// Legacy function for backward compatibility
#[no_mangle]
pub extern "C" fn plat_array_create(elements: *const i32, count: usize) -> *mut RuntimeArray {
    plat_array_create_i32(elements, count)
}

/// Legacy function that returns the appropriate type based on array discriminant
/// Returns as i64 to handle all types uniformly (bool fits in i32, strings return pointer)
#[no_mangle]
pub extern "C" fn plat_array_get(array_ptr: *const RuntimeArray, index: usize) -> i64 {
    if array_ptr.is_null() {
        return 0;
    }

    unsafe {
        let array = &*array_ptr;
        if index >= array.length || array.data.is_null() {
            return 0;
        }

        match array.element_type {
            ARRAY_TYPE_I32 => {
                let data_ptr = array.data as *const i32;
                *data_ptr.add(index) as i64
            },
            ARRAY_TYPE_I64 => {
                let data_ptr = array.data as *const i64;
                *data_ptr.add(index)
            },
            ARRAY_TYPE_BOOL => {
                let data_ptr = array.data as *const bool;
                if *data_ptr.add(index) { 1 } else { 0 }
            },
            ARRAY_TYPE_STRING => {
                let data_ptr = array.data as *const *const c_char;
                *data_ptr.add(index) as i64
            },
            ARRAY_TYPE_CLASS => {
                let data_ptr = array.data as *const *const u8;
                *data_ptr.add(index) as i64
            },
            _ => 0,
        }
    }
}

/// Get the length of an array
///
/// # Safety
/// This function works with raw pointers from generated code
#[no_mangle]
pub extern "C" fn plat_array_len(array_ptr: *const RuntimeArray) -> usize {
    if array_ptr.is_null() {
        return 0;
    }

    unsafe {
        (*array_ptr).length
    }
}

/// Convert an array to a string for interpolation
///
/// # Safety
/// This function works with raw pointers and returns GC memory
#[no_mangle]
pub extern "C" fn plat_array_to_string(array_ptr: *const RuntimeArray) -> *const c_char {
    if array_ptr.is_null() {
        return std::ptr::null();
    }

    unsafe {
        let array = &*array_ptr;

        let mut result = String::from("[");

        for i in 0..array.length {
            if i > 0 {
                result.push_str(", ");
            }
            if !array.data.is_null() {
                match array.element_type {
                    ARRAY_TYPE_I32 => {
                        let data_ptr = array.data as *const i32;
                        let value = *data_ptr.add(i);
                        result.push_str(&value.to_string());
                    },
                    ARRAY_TYPE_I64 => {
                        let data_ptr = array.data as *const i64;
                        let value = *data_ptr.add(i);
                        result.push_str(&value.to_string());
                    },
                    ARRAY_TYPE_BOOL => {
                        let data_ptr = array.data as *const bool;
                        let value = *data_ptr.add(i);
                        result.push_str(if value { "true" } else { "false" });
                    },
                    ARRAY_TYPE_STRING => {
                        let data_ptr = array.data as *const *const c_char;
                        let string_ptr = *data_ptr.add(i);
                        if !string_ptr.is_null() {
                            let c_str = std::ffi::CStr::from_ptr(string_ptr);
                            if let Ok(str_slice) = c_str.to_str() {
                                result.push('"');
                                result.push_str(str_slice);
                                result.push('"');
                            } else {
                                result.push_str("\"<invalid>\"");
                            }
                        } else {
                            result.push_str("\"<null>\"");
                        }
                    },
                    ARRAY_TYPE_CLASS => {
                        let data_ptr = array.data as *const *const u8;
                        let class_ptr = *data_ptr.add(i);
                        // For class instances, show pointer address
                        result.push_str(&format!("<instance@{:p}>", class_ptr));
                    },
                    _ => {
                        result.push_str("<unknown>");
                    }
                }
            }
        }

        result.push(']');

        // Allocate result on GC heap
        let mut result_bytes = result.into_bytes();
        result_bytes.push(0); // null terminator

        let size = result_bytes.len();
        let gc_ptr = plat_gc_alloc(size);

        if gc_ptr.is_null() {
            return std::ptr::null();
        }

        // Copy result to GC memory
        std::ptr::copy_nonoverlapping(result_bytes.as_ptr(), gc_ptr, size);

        gc_ptr as *const c_char
    }
}

// Dict type constants
pub const DICT_KEY_TYPE_STRING: u8 = 0;
pub const DICT_VALUE_TYPE_I32: u8 = 0;
pub const DICT_VALUE_TYPE_I64: u8 = 1;
pub const DICT_VALUE_TYPE_BOOL: u8 = 2;
pub const DICT_VALUE_TYPE_STRING: u8 = 3;

// Set type constants
pub const SET_VALUE_TYPE_I32: u8 = 0;
pub const SET_VALUE_TYPE_I64: u8 = 1;
pub const SET_VALUE_TYPE_BOOL: u8 = 2;
pub const SET_VALUE_TYPE_STRING: u8 = 3;

/// Dict structure for runtime (C-compatible)
/// For simplicity, using string keys and generic values
#[repr(C)]
pub struct RuntimeDict {
    keys: *mut *const c_char,    // Array of string keys (null-terminated)
    values: *mut i64,            // Array of values (all as i64 for simplicity)
    value_types: *mut u8,        // Array indicating the type of each value
    length: usize,
    capacity: usize,
}

/// Create a new dict on the GC heap
#[no_mangle]
pub extern "C" fn plat_dict_create(
    keys: *const *const c_char,
    values: *const i64,
    value_types: *const u8,
    count: usize
) -> *mut RuntimeDict {
    if (keys.is_null() || values.is_null() || value_types.is_null()) && count > 0 {
        return std::ptr::null_mut();
    }

    // Allocate the dict struct
    let dict_size = std::mem::size_of::<RuntimeDict>();
    let dict_ptr = plat_gc_alloc(dict_size) as *mut RuntimeDict;

    if dict_ptr.is_null() {
        return std::ptr::null_mut();
    }

    // Allocate space for keys, values, and types arrays
    let keys_size = count * std::mem::size_of::<*const c_char>();
    let values_size = count * std::mem::size_of::<i64>();
    let types_size = count * std::mem::size_of::<u8>();

    let keys_ptr = if count > 0 {
        plat_gc_alloc(keys_size) as *mut *const c_char
    } else {
        std::ptr::null_mut()
    };

    let values_ptr = if count > 0 {
        plat_gc_alloc(values_size) as *mut i64
    } else {
        std::ptr::null_mut()
    };

    let types_ptr = if count > 0 {
        plat_gc_alloc(types_size) as *mut u8
    } else {
        std::ptr::null_mut()
    };

    if count > 0 && (keys_ptr.is_null() || values_ptr.is_null() || types_ptr.is_null()) {
        return std::ptr::null_mut();
    }

    // Copy data
    if count > 0 {
        unsafe {
            std::ptr::copy_nonoverlapping(keys, keys_ptr, count);
            std::ptr::copy_nonoverlapping(values, values_ptr, count);
            std::ptr::copy_nonoverlapping(value_types, types_ptr, count);
        }
    }

    // Initialize the dict struct
    unsafe {
        (*dict_ptr) = RuntimeDict {
            keys: keys_ptr,
            values: values_ptr,
            value_types: types_ptr,
            length: count,
            capacity: count,
        };
    }

    dict_ptr
}

/// Get a value from the dict by key
#[no_mangle]
pub extern "C" fn plat_dict_get(dict_ptr: *const RuntimeDict, key: *const c_char) -> i64 {
    if dict_ptr.is_null() || key.is_null() {
        return 0;
    }

    unsafe {
        let dict = &*dict_ptr;
        if dict.keys.is_null() || dict.values.is_null() {
            return 0;
        }

        let key_str = match CStr::from_ptr(key).to_str() {
            Ok(s) => s,
            Err(_) => return 0,
        };

        // Linear search for the key (simple implementation)
        for i in 0..dict.length {
            let dict_key_ptr = *dict.keys.add(i);
            if !dict_key_ptr.is_null() {
                if let Ok(dict_key_str) = CStr::from_ptr(dict_key_ptr).to_str() {
                    if dict_key_str == key_str {
                        return *dict.values.add(i);
                    }
                }
            }
        }

        0 // Not found
    }
}

/// Get the length of a dict
#[no_mangle]
pub extern "C" fn plat_dict_len(dict_ptr: *const RuntimeDict) -> i32 {
    if dict_ptr.is_null() {
        return 0;
    }

    unsafe {
        (*dict_ptr).length as i32
    }
}

/// Convert a dict to a string for interpolation
#[no_mangle]
pub extern "C" fn plat_dict_to_string(dict_ptr: *const RuntimeDict) -> *const c_char {
    if dict_ptr.is_null() {
        return std::ptr::null();
    }

    unsafe {
        let dict = &*dict_ptr;

        let mut result = String::from("{");

        for i in 0..dict.length {
            if i > 0 {
                result.push_str(", ");
            }

            // Get key
            if !dict.keys.is_null() {
                let key_ptr = *dict.keys.add(i);
                if !key_ptr.is_null() {
                    if let Ok(key_str) = CStr::from_ptr(key_ptr).to_str() {
                        result.push('"');
                        result.push_str(key_str);
                        result.push_str("\": ");
                    }
                }
            }

            // Get value based on type
            if !dict.values.is_null() && !dict.value_types.is_null() {
                let value = *dict.values.add(i);
                let value_type = *dict.value_types.add(i);

                match value_type {
                    DICT_VALUE_TYPE_I32 => {
                        result.push_str(&(value as i32).to_string());
                    },
                    DICT_VALUE_TYPE_I64 => {
                        result.push_str(&value.to_string());
                    },
                    DICT_VALUE_TYPE_BOOL => {
                        result.push_str(if value != 0 { "true" } else { "false" });
                    },
                    DICT_VALUE_TYPE_STRING => {
                        let string_ptr = value as *const c_char;
                        if !string_ptr.is_null() {
                            if let Ok(string_val) = CStr::from_ptr(string_ptr).to_str() {
                                result.push('"');
                                result.push_str(string_val);
                                result.push('"');
                            } else {
                                result.push_str("\"<invalid>\"");
                            }
                        } else {
                            result.push_str("\"<null>\"");
                        }
                    },
                    _ => {
                        result.push_str("<unknown>");
                    }
                }
            }
        }

        result.push('}');

        // Allocate result on GC heap
        let mut result_bytes = result.into_bytes();
        result_bytes.push(0); // null terminator

        let size = result_bytes.len();
        let gc_ptr = plat_gc_alloc(size);

        if gc_ptr.is_null() {
            return std::ptr::null();
        }

        // Copy result to GC memory
        std::ptr::copy_nonoverlapping(result_bytes.as_ptr(), gc_ptr, size);

        gc_ptr as *const c_char
    }
}

/// Set a value in the dict by key (returns 1 on success, 0 on failure)
#[no_mangle]
pub extern "C" fn plat_dict_set(dict_ptr: *mut RuntimeDict, key: *const c_char, value: i64, value_type: i32) -> i32 {
    if dict_ptr.is_null() || key.is_null() {
        return 0;
    }

    unsafe {
        let dict = &mut *dict_ptr;
        let key_cstr = CStr::from_ptr(key);

        // First check if key exists
        for i in 0..dict.length {
            if !dict.keys.is_null() {
                let existing_key_ptr = *dict.keys.add(i);
                if !existing_key_ptr.is_null() {
                    let existing_key = CStr::from_ptr(existing_key_ptr);
                    if key_cstr == existing_key {
                        // Update existing value
                        if !dict.values.is_null() && !dict.value_types.is_null() {
                            *dict.values.add(i) = value;
                            *dict.value_types.add(i) = value_type as u8;
                        }
                        return 1;
                    }
                }
            }
        }

        // Key doesn't exist, need to add it
        // Check if we need to grow the arrays
        if dict.length >= dict.capacity {
            let new_capacity = if dict.capacity == 0 { 8 } else { dict.capacity * 2 };

            // Allocate new arrays
            let keys_size = new_capacity * std::mem::size_of::<*const c_char>();
            let values_size = new_capacity * std::mem::size_of::<i64>();
            let types_size = new_capacity * std::mem::size_of::<u8>();

            let new_keys_ptr = plat_gc_alloc(keys_size) as *mut *const c_char;
            let new_values_ptr = plat_gc_alloc(values_size) as *mut i64;
            let new_types_ptr = plat_gc_alloc(types_size) as *mut u8;

            if new_keys_ptr.is_null() || new_values_ptr.is_null() || new_types_ptr.is_null() {
                return 0;
            }

            // Copy existing data
            if dict.length > 0 {
                std::ptr::copy_nonoverlapping(dict.keys, new_keys_ptr, dict.length);
                std::ptr::copy_nonoverlapping(dict.values, new_values_ptr, dict.length);
                std::ptr::copy_nonoverlapping(dict.value_types, new_types_ptr, dict.length);
            }

            dict.keys = new_keys_ptr;
            dict.values = new_values_ptr;
            dict.value_types = new_types_ptr;
            dict.capacity = new_capacity;
        }

        // Add new key-value pair
        // Create a copy of the key string
        let key_str = key_cstr.to_str().unwrap_or("");
        let key_copy = CString::new(key_str).unwrap();
        let key_copy_ptr = key_copy.into_raw();

        *dict.keys.add(dict.length) = key_copy_ptr;
        *dict.values.add(dict.length) = value;
        *dict.value_types.add(dict.length) = value_type as u8;
        dict.length += 1;

        1
    }
}

/// Remove a key-value pair from the dict (returns the value if found, 0 otherwise)
#[no_mangle]
pub extern "C" fn plat_dict_remove(dict_ptr: *mut RuntimeDict, key: *const c_char) -> i64 {
    if dict_ptr.is_null() || key.is_null() {
        return 0;
    }

    unsafe {
        let dict = &mut *dict_ptr;
        let key_cstr = CStr::from_ptr(key);

        for i in 0..dict.length {
            if !dict.keys.is_null() {
                let existing_key_ptr = *dict.keys.add(i);
                if !existing_key_ptr.is_null() {
                    let existing_key = CStr::from_ptr(existing_key_ptr);
                    if key_cstr == existing_key {
                        // Found the key, get the value
                        let value = if !dict.values.is_null() {
                            *dict.values.add(i)
                        } else {
                            0
                        };

                        // Shift remaining elements
                        for j in i..dict.length - 1 {
                            *dict.keys.add(j) = *dict.keys.add(j + 1);
                            *dict.values.add(j) = *dict.values.add(j + 1);
                            *dict.value_types.add(j) = *dict.value_types.add(j + 1);
                        }

                        dict.length -= 1;
                        return value;
                    }
                }
            }
        }

        0 // Key not found
    }
}

/// Clear all key-value pairs from the dict
#[no_mangle]
pub extern "C" fn plat_dict_clear(dict_ptr: *mut RuntimeDict) {
    if dict_ptr.is_null() {
        return;
    }

    unsafe {
        let dict = &mut *dict_ptr;
        dict.length = 0;
    }
}

/// Get all keys from the dict as a List[string]
#[no_mangle]
pub extern "C" fn plat_dict_keys(dict_ptr: *const RuntimeDict) -> *mut RuntimeArray {
    if dict_ptr.is_null() {
        return std::ptr::null_mut();
    }

    unsafe {
        let dict = &*dict_ptr;

        // Create array for keys
        let array_size = std::mem::size_of::<RuntimeArray>();
        let array_ptr = plat_gc_alloc(array_size) as *mut RuntimeArray;

        if array_ptr.is_null() {
            return std::ptr::null_mut();
        }

        // Allocate memory for string pointers
        let data_size = dict.length * std::mem::size_of::<*const c_char>();
        let data_ptr = if dict.length > 0 {
            plat_gc_alloc(data_size) as *mut u8
        } else {
            std::ptr::null_mut()
        };

        // Copy keys
        if !data_ptr.is_null() && !dict.keys.is_null() {
            let keys_array = data_ptr as *mut *const c_char;
            for i in 0..dict.length {
                *keys_array.add(i) = *dict.keys.add(i);
            }
        }

        (*array_ptr) = RuntimeArray {
            data: data_ptr,
            length: dict.length,
            capacity: dict.length,
            element_size: std::mem::size_of::<*const c_char>(),
            element_type: ARRAY_TYPE_STRING,
        };

        array_ptr
    }
}

/// Get all values from the dict as a generic array
#[no_mangle]
pub extern "C" fn plat_dict_values(dict_ptr: *const RuntimeDict) -> *mut RuntimeArray {
    if dict_ptr.is_null() {
        return std::ptr::null_mut();
    }

    unsafe {
        let dict = &*dict_ptr;

        // Create array for values
        let array_size = std::mem::size_of::<RuntimeArray>();
        let array_ptr = plat_gc_alloc(array_size) as *mut RuntimeArray;

        if array_ptr.is_null() {
            return std::ptr::null_mut();
        }

        // Allocate memory for values
        let data_size = dict.length * std::mem::size_of::<i64>();
        let data_ptr = if dict.length > 0 {
            plat_gc_alloc(data_size) as *mut u8
        } else {
            std::ptr::null_mut()
        };

        // Copy values
        if !data_ptr.is_null() && !dict.values.is_null() {
            let values_array = data_ptr as *mut i64;
            for i in 0..dict.length {
                *values_array.add(i) = *dict.values.add(i);
            }
        }

        (*array_ptr) = RuntimeArray {
            data: data_ptr,
            length: dict.length,
            capacity: dict.length,
            element_size: std::mem::size_of::<i64>(),
            element_type: ARRAY_TYPE_I32, // Will contain mixed types
        };

        array_ptr
    }
}

/// Check if a key exists in the dict
#[no_mangle]
pub extern "C" fn plat_dict_has_key(dict_ptr: *const RuntimeDict, key: *const c_char) -> i32 {
    if dict_ptr.is_null() || key.is_null() {
        return 0;
    }

    unsafe {
        let dict = &*dict_ptr;
        let key_cstr = CStr::from_ptr(key);

        for i in 0..dict.length {
            if !dict.keys.is_null() {
                let existing_key_ptr = *dict.keys.add(i);
                if !existing_key_ptr.is_null() {
                    let existing_key = CStr::from_ptr(existing_key_ptr);
                    if key_cstr == existing_key {
                        return 1;
                    }
                }
            }
        }

        0
    }
}

/// Check if a value exists in the dict
#[no_mangle]
pub extern "C" fn plat_dict_has_value(dict_ptr: *const RuntimeDict, value: i64, value_type: i32) -> i32 {
    if dict_ptr.is_null() {
        return 0;
    }

    unsafe {
        let dict = &*dict_ptr;

        for i in 0..dict.length {
            if !dict.values.is_null() && !dict.value_types.is_null() {
                let existing_value = *dict.values.add(i);
                let existing_type = *dict.value_types.add(i);

                if existing_type == (value_type as u8) && existing_value == value {
                    return 1;
                }
            }
        }

        0
    }
}

/// Merge another dict into this dict
#[no_mangle]
pub extern "C" fn plat_dict_merge(dict_ptr: *mut RuntimeDict, other_ptr: *const RuntimeDict) {
    if dict_ptr.is_null() || other_ptr.is_null() {
        return;
    }

    unsafe {
        let other = &*other_ptr;

        // Add all key-value pairs from other dict
        for i in 0..other.length {
            if !other.keys.is_null() && !other.values.is_null() && !other.value_types.is_null() {
                let key_ptr = *other.keys.add(i);
                let value = *other.values.add(i);
                let value_type = *other.value_types.add(i);

                if !key_ptr.is_null() {
                    plat_dict_set(dict_ptr, key_ptr, value, value_type as i32);
                }
            }
        }
    }
}

/// Get a value or return a default if not found
#[no_mangle]
pub extern "C" fn plat_dict_get_or(dict_ptr: *const RuntimeDict, key: *const c_char, default: i64) -> i64 {
    if dict_ptr.is_null() || key.is_null() {
        return default;
    }

    unsafe {
        let dict = &*dict_ptr;
        let key_cstr = CStr::from_ptr(key);

        for i in 0..dict.length {
            if !dict.keys.is_null() {
                let existing_key_ptr = *dict.keys.add(i);
                if !existing_key_ptr.is_null() {
                    let existing_key = CStr::from_ptr(existing_key_ptr);
                    if key_cstr == existing_key {
                        if !dict.values.is_null() {
                            return *dict.values.add(i);
                        }
                    }
                }
            }
        }

        default
    }
}

/// Set structure for runtime (C-compatible)
/// Using vector-based implementation with type information
#[repr(C)]
pub struct RuntimeSet {
    values: *mut i64,               // Array of values (stored as i64)
    value_types: *mut u8,          // Array indicating the type of each value
    length: usize,
    capacity: usize,
}

/// Create a new set on the GC heap
#[no_mangle]
pub extern "C" fn plat_set_create(
    values: *const i64,
    value_types: *const u8,
    count: usize
) -> *mut RuntimeSet {
    if (values.is_null() || value_types.is_null()) && count > 0 {
        return std::ptr::null_mut();
    }

    // Allocate the set struct
    let set_size = std::mem::size_of::<RuntimeSet>();
    let set_ptr = plat_gc_alloc(set_size) as *mut RuntimeSet;

    if set_ptr.is_null() {
        return std::ptr::null_mut();
    }

    // Allocate space for values and types arrays
    let values_size = count * std::mem::size_of::<i64>();
    let types_size = count * std::mem::size_of::<u8>();

    let values_ptr = if count > 0 {
        plat_gc_alloc(values_size) as *mut i64
    } else {
        std::ptr::null_mut()
    };

    let types_ptr = if count > 0 {
        plat_gc_alloc(types_size) as *mut u8
    } else {
        std::ptr::null_mut()
    };

    // Copy data if provided and arrays allocated successfully
    if count > 0 && !values_ptr.is_null() && !types_ptr.is_null() {
        unsafe {
            // For Set, we need to deduplicate values
            let mut unique_values: Vec<(i64, u8)> = Vec::new();

            for i in 0..count {
                let value = *values.add(i);
                let value_type = *value_types.add(i);

                // Check if this value is already in the set
                let mut already_exists = false;
                for (existing_value, existing_type) in &unique_values {
                    if *existing_value == value && *existing_type == value_type {
                        already_exists = true;
                        break;
                    }
                }

                if !already_exists {
                    unique_values.push((value, value_type));
                }
            }

            // Copy the unique values
            for (i, (value, value_type)) in unique_values.iter().enumerate() {
                *values_ptr.add(i) = *value;
                *types_ptr.add(i) = *value_type;
            }

            // Initialize the set struct with unique count
            (*set_ptr) = RuntimeSet {
                values: values_ptr,
                value_types: types_ptr,
                length: unique_values.len(),
                capacity: unique_values.len(),
            };
        }
    } else {
        // Initialize empty set
        unsafe {
            (*set_ptr) = RuntimeSet {
                values: values_ptr,
                value_types: types_ptr,
                length: 0,
                capacity: 0,
            };
        }
    }

    set_ptr
}

/// Check if a value is in the set
#[no_mangle]
pub extern "C" fn plat_set_contains(set_ptr: *const RuntimeSet, value: i64, value_type: u8) -> bool {
    if set_ptr.is_null() {
        return false;
    }

    unsafe {
        let set = &*set_ptr;
        if set.values.is_null() || set.value_types.is_null() {
            return false;
        }

        // Linear search for the value
        for i in 0..set.length {
            let set_value = *set.values.add(i);
            let set_value_type = *set.value_types.add(i);
            if set_value == value && set_value_type == value_type {
                return true;
            }
        }

        false
    }
}

/// Get the length of a set
#[no_mangle]
pub extern "C" fn plat_set_len(set_ptr: *const RuntimeSet) -> usize {
    if set_ptr.is_null() {
        return 0;
    }

    unsafe {
        (*set_ptr).length
    }
}

/// Convert a set to a string for interpolation
#[no_mangle]
pub extern "C" fn plat_set_to_string(set_ptr: *const RuntimeSet) -> *const c_char {
    if set_ptr.is_null() {
        return std::ptr::null();
    }

    unsafe {
        let set = &*set_ptr;

        let mut result = String::from("{");

        for i in 0..set.length {
            if i > 0 {
                result.push_str(", ");
            }

            // Get value based on type
            if !set.values.is_null() && !set.value_types.is_null() {
                let value = *set.values.add(i);
                let value_type = *set.value_types.add(i);

                match value_type {
                    SET_VALUE_TYPE_I32 => {
                        result.push_str(&(value as i32).to_string());
                    },
                    SET_VALUE_TYPE_I64 => {
                        result.push_str(&value.to_string());
                    },
                    SET_VALUE_TYPE_BOOL => {
                        result.push_str(if value != 0 { "true" } else { "false" });
                    },
                    SET_VALUE_TYPE_STRING => {
                        let string_ptr = value as *const c_char;
                        if !string_ptr.is_null() {
                            if let Ok(string_val) = CStr::from_ptr(string_ptr).to_str() {
                                result.push('"');
                                result.push_str(string_val);
                                result.push('"');
                            } else {
                                result.push_str("\"<invalid>\"");
                            }
                        } else {
                            result.push_str("\"<null>\"");
                        }
                    },
                    _ => {
                        result.push_str("<unknown>");
                    }
                }
            }
        }

        result.push('}');

        // Allocate result on GC heap
        let mut result_bytes = result.into_bytes();
        result_bytes.push(0); // null terminator

        let size = result_bytes.len();
        let gc_ptr = plat_gc_alloc(size);

        if gc_ptr.is_null() {
            return std::ptr::null();
        }

        // Copy result to GC memory
        std::ptr::copy_nonoverlapping(result_bytes.as_ptr(), gc_ptr, size);

        gc_ptr as *const c_char
    }
}

/// Add a value to a set (returns true if value was added, false if already exists)
#[no_mangle]
pub extern "C" fn plat_set_add(set_ptr: *mut RuntimeSet, value: i64, value_type: u8) -> bool {
    if set_ptr.is_null() {
        return false;
    }

    unsafe {
        let set = &mut *set_ptr;

        // Check if value already exists
        if plat_set_contains(set_ptr, value, value_type) {
            return false; // Value already exists
        }

        // Need to expand capacity
        let new_capacity = if set.capacity == 0 { 1 } else { set.capacity * 2 };

        // Allocate new arrays
        let new_values_size = new_capacity * std::mem::size_of::<i64>();
        let new_types_size = new_capacity * std::mem::size_of::<u8>();

        let new_values_ptr = plat_gc_alloc(new_values_size) as *mut i64;
        let new_types_ptr = plat_gc_alloc(new_types_size) as *mut u8;

        if new_values_ptr.is_null() || new_types_ptr.is_null() {
            return false;
        }

        // Copy existing data
        if set.length > 0 && !set.values.is_null() && !set.value_types.is_null() {
            std::ptr::copy_nonoverlapping(set.values, new_values_ptr, set.length);
            std::ptr::copy_nonoverlapping(set.value_types, new_types_ptr, set.length);
        }

        // Add new value
        *new_values_ptr.add(set.length) = value;
        *new_types_ptr.add(set.length) = value_type;

        // Update set
        set.values = new_values_ptr;
        set.value_types = new_types_ptr;
        set.length += 1;
        set.capacity = new_capacity;

        true
    }
}

/// Remove a value from a set (returns true if value was removed, false if not found)
#[no_mangle]
pub extern "C" fn plat_set_remove(set_ptr: *mut RuntimeSet, value: i64, value_type: u8) -> bool {
    if set_ptr.is_null() {
        return false;
    }

    unsafe {
        let set = &mut *set_ptr;
        if set.values.is_null() || set.value_types.is_null() {
            return false;
        }

        // Find the value to remove
        for i in 0..set.length {
            let set_value = *set.values.add(i);
            let set_value_type = *set.value_types.add(i);

            if set_value == value && set_value_type == value_type {
                // Found the value, remove it by shifting remaining elements
                for j in i..set.length - 1 {
                    *set.values.add(j) = *set.values.add(j + 1);
                    *set.value_types.add(j) = *set.value_types.add(j + 1);
                }
                set.length -= 1;
                return true;
            }
        }

        false // Value not found
    }
}

/// Clear all values from a set
#[no_mangle]
pub extern "C" fn plat_set_clear(set_ptr: *mut RuntimeSet) {
    if set_ptr.is_null() {
        return;
    }

    unsafe {
        let set = &mut *set_ptr;
        set.length = 0;
        // Note: We keep the allocated memory for reuse
    }
}

/// Get length of a set (alias for plat_set_len for consistency with other collections)
#[no_mangle]
pub extern "C" fn plat_set_length(set_ptr: *const RuntimeSet) -> i32 {
    plat_set_len(set_ptr) as i32
}

/// Create a union of two sets (returns new set)
#[no_mangle]
pub extern "C" fn plat_set_union(set1_ptr: *const RuntimeSet, set2_ptr: *const RuntimeSet) -> *mut RuntimeSet {
    if set1_ptr.is_null() || set2_ptr.is_null() {
        return std::ptr::null_mut();
    }

    unsafe {
        let set1 = &*set1_ptr;
        let set2 = &*set2_ptr;

        // Create a temporary vector to collect all unique values
        let mut union_values: Vec<(i64, u8)> = Vec::new();

        // Add all values from set1
        for i in 0..set1.length {
            if !set1.values.is_null() && !set1.value_types.is_null() {
                let value = *set1.values.add(i);
                let value_type = *set1.value_types.add(i);
                union_values.push((value, value_type));
            }
        }

        // Add values from set2 that aren't already in union
        for i in 0..set2.length {
            if !set2.values.is_null() && !set2.value_types.is_null() {
                let value = *set2.values.add(i);
                let value_type = *set2.value_types.add(i);

                // Check if this value is already in union
                let mut already_exists = false;
                for (existing_value, existing_type) in &union_values {
                    if *existing_value == value && *existing_type == value_type {
                        already_exists = true;
                        break;
                    }
                }

                if !already_exists {
                    union_values.push((value, value_type));
                }
            }
        }

        // Create new set with union values
        if union_values.is_empty() {
            plat_set_create(std::ptr::null(), std::ptr::null(), 0)
        } else {
            let values: Vec<i64> = union_values.iter().map(|(v, _)| *v).collect();
            let types: Vec<u8> = union_values.iter().map(|(_, t)| *t).collect();
            plat_set_create(values.as_ptr(), types.as_ptr(), union_values.len())
        }
    }
}

/// Create an intersection of two sets (returns new set)
#[no_mangle]
pub extern "C" fn plat_set_intersection(set1_ptr: *const RuntimeSet, set2_ptr: *const RuntimeSet) -> *mut RuntimeSet {
    if set1_ptr.is_null() || set2_ptr.is_null() {
        return std::ptr::null_mut();
    }

    unsafe {
        let set1 = &*set1_ptr;
        let _set2 = &*set2_ptr;

        // Create a temporary vector to collect intersection values
        let mut intersection_values: Vec<(i64, u8)> = Vec::new();

        // Check each value in set1 to see if it exists in set2
        for i in 0..set1.length {
            if !set1.values.is_null() && !set1.value_types.is_null() {
                let value = *set1.values.add(i);
                let value_type = *set1.value_types.add(i);

                // Check if this value exists in set2
                if plat_set_contains(set2_ptr, value, value_type) {
                    intersection_values.push((value, value_type));
                }
            }
        }

        // Create new set with intersection values
        if intersection_values.is_empty() {
            plat_set_create(std::ptr::null(), std::ptr::null(), 0)
        } else {
            let values: Vec<i64> = intersection_values.iter().map(|(v, _)| *v).collect();
            let types: Vec<u8> = intersection_values.iter().map(|(_, t)| *t).collect();
            plat_set_create(values.as_ptr(), types.as_ptr(), intersection_values.len())
        }
    }
}

/// Create a difference of two sets (set1 - set2, returns new set)
#[no_mangle]
pub extern "C" fn plat_set_difference(set1_ptr: *const RuntimeSet, set2_ptr: *const RuntimeSet) -> *mut RuntimeSet {
    if set1_ptr.is_null() || set2_ptr.is_null() {
        return std::ptr::null_mut();
    }

    unsafe {
        let set1 = &*set1_ptr;

        // Create a temporary vector to collect difference values
        let mut difference_values: Vec<(i64, u8)> = Vec::new();

        // Check each value in set1 to see if it does NOT exist in set2
        for i in 0..set1.length {
            if !set1.values.is_null() && !set1.value_types.is_null() {
                let value = *set1.values.add(i);
                let value_type = *set1.value_types.add(i);

                // Include value if it's NOT in set2
                if !plat_set_contains(set2_ptr, value, value_type) {
                    difference_values.push((value, value_type));
                }
            }
        }

        // Create new set with difference values
        if difference_values.is_empty() {
            plat_set_create(std::ptr::null(), std::ptr::null(), 0)
        } else {
            let values: Vec<i64> = difference_values.iter().map(|(v, _)| *v).collect();
            let types: Vec<u8> = difference_values.iter().map(|(_, t)| *t).collect();
            plat_set_create(values.as_ptr(), types.as_ptr(), difference_values.len())
        }
    }
}

/// Check if set1 is a subset of set2
#[no_mangle]
pub extern "C" fn plat_set_is_subset_of(set1_ptr: *const RuntimeSet, set2_ptr: *const RuntimeSet) -> bool {
    if set1_ptr.is_null() || set2_ptr.is_null() {
        return false;
    }

    unsafe {
        let set1 = &*set1_ptr;

        // Empty set is a subset of any set
        if set1.length == 0 {
            return true;
        }

        // Check that every element in set1 exists in set2
        for i in 0..set1.length {
            if !set1.values.is_null() && !set1.value_types.is_null() {
                let value = *set1.values.add(i);
                let value_type = *set1.value_types.add(i);

                if !plat_set_contains(set2_ptr, value, value_type) {
                    return false; // Found element in set1 that's not in set2
                }
            }
        }

        true
    }
}

/// Check if set1 is a superset of set2
#[no_mangle]
pub extern "C" fn plat_set_is_superset_of(set1_ptr: *const RuntimeSet, set2_ptr: *const RuntimeSet) -> bool {
    // set1 is a superset of set2 if set2 is a subset of set1
    plat_set_is_subset_of(set2_ptr, set1_ptr)
}

/// Check if two sets are disjoint (have no common elements)
#[no_mangle]
pub extern "C" fn plat_set_is_disjoint_from(set1_ptr: *const RuntimeSet, set2_ptr: *const RuntimeSet) -> bool {
    if set1_ptr.is_null() || set2_ptr.is_null() {
        return true;
    }

    unsafe {
        let set1 = &*set1_ptr;

        // Check if any element from set1 exists in set2
        for i in 0..set1.length {
            if !set1.values.is_null() && !set1.value_types.is_null() {
                let value = *set1.values.add(i);
                let value_type = *set1.value_types.add(i);

                if plat_set_contains(set2_ptr, value, value_type) {
                    return false; // Found common element
                }
            }
        }

        true // No common elements found
    }
}

// ===== STRING METHODS =====

/// Get the character length of a string (not byte length)
#[no_mangle]
pub extern "C" fn plat_string_length(str_ptr: *const c_char) -> i32 {
    if str_ptr.is_null() {
        return 0;
    }

    unsafe {
        match CStr::from_ptr(str_ptr).to_str() {
            Ok(s) => s.chars().count() as i32,
            Err(_) => 0,
        }
    }
}

/// Concatenate two strings
#[no_mangle]
pub extern "C" fn plat_string_concat(str1_ptr: *const c_char, str2_ptr: *const c_char) -> *const c_char {
    if str1_ptr.is_null() || str2_ptr.is_null() {
        return std::ptr::null();
    }

    unsafe {
        let str1 = match CStr::from_ptr(str1_ptr).to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null(),
        };

        let str2 = match CStr::from_ptr(str2_ptr).to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null(),
        };

        let result = format!("{}{}", str1, str2);
        let mut result_bytes = result.into_bytes();
        result_bytes.push(0); // null terminator

        let size = result_bytes.len();
        let gc_ptr = plat_gc_alloc(size);

        if gc_ptr.is_null() {
            return std::ptr::null();
        }

        std::ptr::copy_nonoverlapping(result_bytes.as_ptr(), gc_ptr, size);
        gc_ptr as *const c_char
    }
}

/// Check if string contains a substring
#[no_mangle]
pub extern "C" fn plat_string_contains(str_ptr: *const c_char, substr_ptr: *const c_char) -> bool {
    if str_ptr.is_null() || substr_ptr.is_null() {
        return false;
    }

    unsafe {
        let str_val = match CStr::from_ptr(str_ptr).to_str() {
            Ok(s) => s,
            Err(_) => return false,
        };

        let substr = match CStr::from_ptr(substr_ptr).to_str() {
            Ok(s) => s,
            Err(_) => return false,
        };

        str_val.contains(substr)
    }
}

/// Check if string starts with a prefix
#[no_mangle]
pub extern "C" fn plat_string_starts_with(str_ptr: *const c_char, prefix_ptr: *const c_char) -> bool {
    if str_ptr.is_null() || prefix_ptr.is_null() {
        return false;
    }

    unsafe {
        let str_val = match CStr::from_ptr(str_ptr).to_str() {
            Ok(s) => s,
            Err(_) => return false,
        };

        let prefix = match CStr::from_ptr(prefix_ptr).to_str() {
            Ok(s) => s,
            Err(_) => return false,
        };

        str_val.starts_with(prefix)
    }
}

/// Check if string ends with a suffix
#[no_mangle]
pub extern "C" fn plat_string_ends_with(str_ptr: *const c_char, suffix_ptr: *const c_char) -> bool {
    if str_ptr.is_null() || suffix_ptr.is_null() {
        return false;
    }

    unsafe {
        let str_val = match CStr::from_ptr(str_ptr).to_str() {
            Ok(s) => s,
            Err(_) => return false,
        };

        let suffix = match CStr::from_ptr(suffix_ptr).to_str() {
            Ok(s) => s,
            Err(_) => return false,
        };

        str_val.ends_with(suffix)
    }
}

/// Trim whitespace from both ends of string
#[no_mangle]
pub extern "C" fn plat_string_trim(str_ptr: *const c_char) -> *const c_char {
    if str_ptr.is_null() {
        return std::ptr::null();
    }

    unsafe {
        let str_val = match CStr::from_ptr(str_ptr).to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null(),
        };

        let result = str_val.trim();
        let mut result_bytes = result.as_bytes().to_vec();
        result_bytes.push(0); // null terminator

        let size = result_bytes.len();
        let gc_ptr = plat_gc_alloc(size);

        if gc_ptr.is_null() {
            return std::ptr::null();
        }

        std::ptr::copy_nonoverlapping(result_bytes.as_ptr(), gc_ptr, size);
        gc_ptr as *const c_char
    }
}

/// Trim whitespace from left end of string
#[no_mangle]
pub extern "C" fn plat_string_trim_left(str_ptr: *const c_char) -> *const c_char {
    if str_ptr.is_null() {
        return std::ptr::null();
    }

    unsafe {
        let str_val = match CStr::from_ptr(str_ptr).to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null(),
        };

        let result = str_val.trim_start();
        let mut result_bytes = result.as_bytes().to_vec();
        result_bytes.push(0); // null terminator

        let size = result_bytes.len();
        let gc_ptr = plat_gc_alloc(size);

        if gc_ptr.is_null() {
            return std::ptr::null();
        }

        std::ptr::copy_nonoverlapping(result_bytes.as_ptr(), gc_ptr, size);
        gc_ptr as *const c_char
    }
}

/// Trim whitespace from right end of string
#[no_mangle]
pub extern "C" fn plat_string_trim_right(str_ptr: *const c_char) -> *const c_char {
    if str_ptr.is_null() {
        return std::ptr::null();
    }

    unsafe {
        let str_val = match CStr::from_ptr(str_ptr).to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null(),
        };

        let result = str_val.trim_end();
        let mut result_bytes = result.as_bytes().to_vec();
        result_bytes.push(0); // null terminator

        let size = result_bytes.len();
        let gc_ptr = plat_gc_alloc(size);

        if gc_ptr.is_null() {
            return std::ptr::null();
        }

        std::ptr::copy_nonoverlapping(result_bytes.as_ptr(), gc_ptr, size);
        gc_ptr as *const c_char
    }
}

/// Replace first occurrence of substring
#[no_mangle]
pub extern "C" fn plat_string_replace(str_ptr: *const c_char, from_ptr: *const c_char, to_ptr: *const c_char) -> *const c_char {
    if str_ptr.is_null() || from_ptr.is_null() || to_ptr.is_null() {
        return std::ptr::null();
    }

    unsafe {
        let str_val = match CStr::from_ptr(str_ptr).to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null(),
        };

        let from = match CStr::from_ptr(from_ptr).to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null(),
        };

        let to = match CStr::from_ptr(to_ptr).to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null(),
        };

        let result = str_val.replacen(from, to, 1);
        let mut result_bytes = result.into_bytes();
        result_bytes.push(0); // null terminator

        let size = result_bytes.len();
        let gc_ptr = plat_gc_alloc(size);

        if gc_ptr.is_null() {
            return std::ptr::null();
        }

        std::ptr::copy_nonoverlapping(result_bytes.as_ptr(), gc_ptr, size);
        gc_ptr as *const c_char
    }
}

/// Replace all occurrences of substring
#[no_mangle]
pub extern "C" fn plat_string_replace_all(str_ptr: *const c_char, from_ptr: *const c_char, to_ptr: *const c_char) -> *const c_char {
    if str_ptr.is_null() || from_ptr.is_null() || to_ptr.is_null() {
        return std::ptr::null();
    }

    unsafe {
        let str_val = match CStr::from_ptr(str_ptr).to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null(),
        };

        let from = match CStr::from_ptr(from_ptr).to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null(),
        };

        let to = match CStr::from_ptr(to_ptr).to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null(),
        };

        let result = str_val.replace(from, to);
        let mut result_bytes = result.into_bytes();
        result_bytes.push(0); // null terminator

        let size = result_bytes.len();
        let gc_ptr = plat_gc_alloc(size);

        if gc_ptr.is_null() {
            return std::ptr::null();
        }

        std::ptr::copy_nonoverlapping(result_bytes.as_ptr(), gc_ptr, size);
        gc_ptr as *const c_char
    }
}

/// Split string by delimiter and return as string array
#[no_mangle]
pub extern "C" fn plat_string_split(str_ptr: *const c_char, delimiter_ptr: *const c_char) -> *mut RuntimeArray {
    if str_ptr.is_null() || delimiter_ptr.is_null() {
        return std::ptr::null_mut();
    }

    unsafe {
        let str_val = match CStr::from_ptr(str_ptr).to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null_mut(),
        };

        let delimiter = match CStr::from_ptr(delimiter_ptr).to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null_mut(),
        };

        let parts: Vec<&str> = str_val.split(delimiter).collect();

        // Convert parts to C strings on GC heap
        let mut c_strings: Vec<*const c_char> = Vec::new();

        for part in parts {
            let mut part_bytes = part.as_bytes().to_vec();
            part_bytes.push(0); // null terminator

            let size = part_bytes.len();
            let gc_ptr = plat_gc_alloc(size);

            if gc_ptr.is_null() {
                return std::ptr::null_mut();
            }

            std::ptr::copy_nonoverlapping(part_bytes.as_ptr(), gc_ptr, size);
            c_strings.push(gc_ptr as *const c_char);
        }

        // Create string array
        plat_array_create_string(c_strings.as_ptr(), c_strings.len())
    }
}

/// Check if all characters are alphabetic
#[no_mangle]
pub extern "C" fn plat_string_is_alpha(str_ptr: *const c_char) -> bool {
    if str_ptr.is_null() {
        return false;
    }

    unsafe {
        let str_val = match CStr::from_ptr(str_ptr).to_str() {
            Ok(s) => s,
            Err(_) => return false,
        };

        !str_val.is_empty() && str_val.chars().all(|c| c.is_alphabetic())
    }
}

/// Check if all characters are numeric
#[no_mangle]
pub extern "C" fn plat_string_is_numeric(str_ptr: *const c_char) -> bool {
    if str_ptr.is_null() {
        return false;
    }

    unsafe {
        let str_val = match CStr::from_ptr(str_ptr).to_str() {
            Ok(s) => s,
            Err(_) => return false,
        };

        !str_val.is_empty() && str_val.chars().all(|c| c.is_numeric())
    }
}

/// Check if all characters are alphanumeric
#[no_mangle]
pub extern "C" fn plat_string_is_alphanumeric(str_ptr: *const c_char) -> bool {
    if str_ptr.is_null() {
        return false;
    }

    unsafe {
        let str_val = match CStr::from_ptr(str_ptr).to_str() {
            Ok(s) => s,
            Err(_) => return false,
        };

        !str_val.is_empty() && str_val.chars().all(|c| c.is_alphanumeric())
    }
}

// ===== LIST METHODS =====

/// Get the length of an array (alias for plat_array_len)
#[no_mangle]
pub extern "C" fn plat_array_length(array_ptr: *const RuntimeArray) -> i32 {
    plat_array_len(array_ptr) as i32
}

/// Safely get an element from array, returns Option<T> encoded as (found: bool, value: i64)
#[no_mangle]
pub extern "C" fn plat_array_get_safe(array_ptr: *const RuntimeArray, index: i32) -> (bool, i64) {
    if array_ptr.is_null() || index < 0 {
        return (false, 0);
    }

    unsafe {
        let array = &*array_ptr;
        let index = index as usize;

        if index >= array.length || array.data.is_null() {
            return (false, 0);
        }

        let value = match array.element_type {
            ARRAY_TYPE_I32 => {
                let data_ptr = array.data as *const i32;
                *data_ptr.add(index) as i64
            },
            ARRAY_TYPE_I64 => {
                let data_ptr = array.data as *const i64;
                *data_ptr.add(index)
            },
            ARRAY_TYPE_BOOL => {
                let data_ptr = array.data as *const bool;
                if *data_ptr.add(index) { 1 } else { 0 }
            },
            ARRAY_TYPE_STRING => {
                let data_ptr = array.data as *const *const c_char;
                *data_ptr.add(index) as i64
            },
            ARRAY_TYPE_CLASS => {
                let data_ptr = array.data as *const *const u8;
                *data_ptr.add(index) as i64
            },
            _ => return (false, 0),
        };

        (true, value)
    }
}

/// Set an element in array at given index (mutates array)
#[no_mangle]
pub extern "C" fn plat_array_set(array_ptr: *mut RuntimeArray, index: i32, value: i64) -> bool {
    if array_ptr.is_null() || index < 0 {
        return false;
    }

    unsafe {
        let array = &mut *array_ptr;
        let index = index as usize;

        if index >= array.length || array.data.is_null() {
            return false;
        }

        match array.element_type {
            ARRAY_TYPE_I32 => {
                let data_ptr = array.data as *mut i32;
                *data_ptr.add(index) = value as i32;
            },
            ARRAY_TYPE_I64 => {
                let data_ptr = array.data as *mut i64;
                *data_ptr.add(index) = value;
            },
            ARRAY_TYPE_BOOL => {
                let data_ptr = array.data as *mut bool;
                *data_ptr.add(index) = value != 0;
            },
            ARRAY_TYPE_STRING => {
                let data_ptr = array.data as *mut *const c_char;
                *data_ptr.add(index) = value as *const c_char;
            },
            ARRAY_TYPE_CLASS => {
                let data_ptr = array.data as *mut *const u8;
                *data_ptr.add(index) = value as *const u8;
            },
            _ => return false,
        };

        true
    }
}

/// Append element to end of array (reallocates if needed)
#[no_mangle]
pub extern "C" fn plat_array_append(array_ptr: *mut RuntimeArray, value: i64) -> bool {
    if array_ptr.is_null() {
        return false;
    }

    unsafe {
        let array = &mut *array_ptr;

        // Check if we need to grow the array
        if array.length >= array.capacity {
            let new_capacity = if array.capacity == 0 { 4 } else { array.capacity * 2 };
            let new_size = new_capacity * array.element_size;
            let new_data_ptr = plat_gc_alloc(new_size);

            if new_data_ptr.is_null() {
                return false;
            }

            // Copy existing data
            if array.length > 0 && !array.data.is_null() {
                let old_size = array.length * array.element_size;
                std::ptr::copy_nonoverlapping(array.data, new_data_ptr, old_size);
            }

            array.data = new_data_ptr;
            array.capacity = new_capacity;
        }

        // Add the new element
        match array.element_type {
            ARRAY_TYPE_I32 => {
                let data_ptr = array.data as *mut i32;
                *data_ptr.add(array.length) = value as i32;
            },
            ARRAY_TYPE_I64 => {
                let data_ptr = array.data as *mut i64;
                *data_ptr.add(array.length) = value;
            },
            ARRAY_TYPE_BOOL => {
                let data_ptr = array.data as *mut bool;
                *data_ptr.add(array.length) = value != 0;
            },
            ARRAY_TYPE_STRING => {
                let data_ptr = array.data as *mut *const c_char;
                *data_ptr.add(array.length) = value as *const c_char;
            },
            ARRAY_TYPE_CLASS => {
                let data_ptr = array.data as *mut *const u8;
                *data_ptr.add(array.length) = value as *const u8;
            },
            _ => return false,
        };

        array.length += 1;
        true
    }
}

/// Insert element at specific index (shifts elements right)
#[no_mangle]
pub extern "C" fn plat_array_insert_at(array_ptr: *mut RuntimeArray, index: i32, value: i64) -> bool {
    if array_ptr.is_null() || index < 0 {
        return false;
    }

    unsafe {
        let array = &mut *array_ptr;
        let index = index as usize;

        if index > array.length {
            return false; // Can't insert beyond length
        }

        // Check if we need to grow the array
        if array.length >= array.capacity {
            let new_capacity = if array.capacity == 0 { 4 } else { array.capacity * 2 };
            let new_size = new_capacity * array.element_size;
            let new_data_ptr = plat_gc_alloc(new_size);

            if new_data_ptr.is_null() {
                return false;
            }

            // Copy existing data
            if array.length > 0 && !array.data.is_null() {
                let old_size = array.length * array.element_size;
                std::ptr::copy_nonoverlapping(array.data, new_data_ptr, old_size);
            }

            array.data = new_data_ptr;
            array.capacity = new_capacity;
        }

        // Shift elements right from insertion point
        if index < array.length {
            let elements_to_move = array.length - index;
            match array.element_type {
                ARRAY_TYPE_I32 => {
                    let data_ptr = array.data as *mut i32;
                    std::ptr::copy(data_ptr.add(index), data_ptr.add(index + 1), elements_to_move);
                },
                ARRAY_TYPE_I64 => {
                    let data_ptr = array.data as *mut i64;
                    std::ptr::copy(data_ptr.add(index), data_ptr.add(index + 1), elements_to_move);
                },
                ARRAY_TYPE_BOOL => {
                    let data_ptr = array.data as *mut bool;
                    std::ptr::copy(data_ptr.add(index), data_ptr.add(index + 1), elements_to_move);
                },
                ARRAY_TYPE_STRING => {
                    let data_ptr = array.data as *mut *const c_char;
                    std::ptr::copy(data_ptr.add(index), data_ptr.add(index + 1), elements_to_move);
                },
                ARRAY_TYPE_CLASS => {
                    let data_ptr = array.data as *mut *const u8;
                    std::ptr::copy(data_ptr.add(index), data_ptr.add(index + 1), elements_to_move);
                },
                _ => return false,
            }
        }

        // Insert the new element
        match array.element_type {
            ARRAY_TYPE_I32 => {
                let data_ptr = array.data as *mut i32;
                *data_ptr.add(index) = value as i32;
            },
            ARRAY_TYPE_I64 => {
                let data_ptr = array.data as *mut i64;
                *data_ptr.add(index) = value;
            },
            ARRAY_TYPE_BOOL => {
                let data_ptr = array.data as *mut bool;
                *data_ptr.add(index) = value != 0;
            },
            ARRAY_TYPE_STRING => {
                let data_ptr = array.data as *mut *const c_char;
                *data_ptr.add(index) = value as *const c_char;
            },
            ARRAY_TYPE_CLASS => {
                let data_ptr = array.data as *mut *const u8;
                *data_ptr.add(index) = value as *const u8;
            },
            _ => return false,
        };

        array.length += 1;
        true
    }
}

/// Remove element at specific index, returns Option<T> encoded as (found: bool, value: i64)
#[no_mangle]
pub extern "C" fn plat_array_remove_at(array_ptr: *mut RuntimeArray, index: i32) -> (bool, i64) {
    if array_ptr.is_null() || index < 0 {
        return (false, 0);
    }

    unsafe {
        let array = &mut *array_ptr;
        let index = index as usize;

        if index >= array.length || array.data.is_null() {
            return (false, 0);
        }

        // Get the value being removed
        let removed_value = match array.element_type {
            ARRAY_TYPE_I32 => {
                let data_ptr = array.data as *const i32;
                *data_ptr.add(index) as i64
            },
            ARRAY_TYPE_I64 => {
                let data_ptr = array.data as *const i64;
                *data_ptr.add(index)
            },
            ARRAY_TYPE_BOOL => {
                let data_ptr = array.data as *const bool;
                if *data_ptr.add(index) { 1 } else { 0 }
            },
            ARRAY_TYPE_STRING => {
                let data_ptr = array.data as *const *const c_char;
                *data_ptr.add(index) as i64
            },
            ARRAY_TYPE_CLASS => {
                let data_ptr = array.data as *const *const u8;
                *data_ptr.add(index) as i64
            },
            _ => return (false, 0),
        };

        // Shift elements left to fill the gap
        if index < array.length - 1 {
            let elements_to_move = array.length - index - 1;
            match array.element_type {
                ARRAY_TYPE_I32 => {
                    let data_ptr = array.data as *mut i32;
                    std::ptr::copy(data_ptr.add(index + 1), data_ptr.add(index), elements_to_move);
                },
                ARRAY_TYPE_I64 => {
                    let data_ptr = array.data as *mut i64;
                    std::ptr::copy(data_ptr.add(index + 1), data_ptr.add(index), elements_to_move);
                },
                ARRAY_TYPE_BOOL => {
                    let data_ptr = array.data as *mut bool;
                    std::ptr::copy(data_ptr.add(index + 1), data_ptr.add(index), elements_to_move);
                },
                ARRAY_TYPE_STRING => {
                    let data_ptr = array.data as *mut *const c_char;
                    std::ptr::copy(data_ptr.add(index + 1), data_ptr.add(index), elements_to_move);
                },
                ARRAY_TYPE_CLASS => {
                    let data_ptr = array.data as *mut *const u8;
                    std::ptr::copy(data_ptr.add(index + 1), data_ptr.add(index), elements_to_move);
                },
                _ => return (false, 0),
            }
        }

        array.length -= 1;
        (true, removed_value)
    }
}

/// Clear all elements from array
#[no_mangle]
pub extern "C" fn plat_array_clear(array_ptr: *mut RuntimeArray) -> bool {
    if array_ptr.is_null() {
        return false;
    }

    unsafe {
        let array = &mut *array_ptr;
        array.length = 0;
        true
    }
}

/// Check if array contains a specific value
#[no_mangle]
pub extern "C" fn plat_array_contains(array_ptr: *const RuntimeArray, value: i64) -> bool {
    if array_ptr.is_null() {
        return false;
    }

    unsafe {
        let array = &*array_ptr;
        if array.data.is_null() {
            return false;
        }

        for i in 0..array.length {
            let element_value = match array.element_type {
                ARRAY_TYPE_I32 => {
                    let data_ptr = array.data as *const i32;
                    *data_ptr.add(i) as i64
                },
                ARRAY_TYPE_I64 => {
                    let data_ptr = array.data as *const i64;
                    *data_ptr.add(i)
                },
                ARRAY_TYPE_BOOL => {
                    let data_ptr = array.data as *const bool;
                    if *data_ptr.add(i) { 1 } else { 0 }
                },
                ARRAY_TYPE_STRING => {
                    let data_ptr = array.data as *const *const c_char;
                    *data_ptr.add(i) as i64
                },
            ARRAY_TYPE_CLASS => {
                let data_ptr = array.data as *const *const u8;
                *data_ptr.add(i) as i64
            },
            _ => continue,
            };

            if element_value == value {
                return true;
            }
        }

        false
    }
}

/// Find index of first occurrence of value, returns Option<i32> encoded as (found: bool, index: i32)
#[no_mangle]
pub extern "C" fn plat_array_index_of(array_ptr: *const RuntimeArray, value: i64) -> (bool, i32) {
    if array_ptr.is_null() {
        return (false, -1);
    }

    unsafe {
        let array = &*array_ptr;
        if array.data.is_null() {
            return (false, -1);
        }

        for i in 0..array.length {
            let element_value = match array.element_type {
                ARRAY_TYPE_I32 => {
                    let data_ptr = array.data as *const i32;
                    *data_ptr.add(i) as i64
                },
                ARRAY_TYPE_I64 => {
                    let data_ptr = array.data as *const i64;
                    *data_ptr.add(i)
                },
                ARRAY_TYPE_BOOL => {
                    let data_ptr = array.data as *const bool;
                    if *data_ptr.add(i) { 1 } else { 0 }
                },
                ARRAY_TYPE_STRING => {
                    let data_ptr = array.data as *const *const c_char;
                    *data_ptr.add(i) as i64
                },
            ARRAY_TYPE_CLASS => {
                let data_ptr = array.data as *const *const u8;
                *data_ptr.add(i) as i64
            },
            _ => continue,
            };

            if element_value == value {
                return (true, i as i32);
            }
        }

        (false, -1)
    }
}

/// Count occurrences of value in array
#[no_mangle]
pub extern "C" fn plat_array_count(array_ptr: *const RuntimeArray, value: i64) -> i32 {
    if array_ptr.is_null() {
        return 0;
    }

    unsafe {
        let array = &*array_ptr;
        if array.data.is_null() {
            return 0;
        }

        let mut count = 0;
        for i in 0..array.length {
            let element_value = match array.element_type {
                ARRAY_TYPE_I32 => {
                    let data_ptr = array.data as *const i32;
                    *data_ptr.add(i) as i64
                },
                ARRAY_TYPE_I64 => {
                    let data_ptr = array.data as *const i64;
                    *data_ptr.add(i)
                },
                ARRAY_TYPE_BOOL => {
                    let data_ptr = array.data as *const bool;
                    if *data_ptr.add(i) { 1 } else { 0 }
                },
                ARRAY_TYPE_STRING => {
                    let data_ptr = array.data as *const *const c_char;
                    *data_ptr.add(i) as i64
                },
            ARRAY_TYPE_CLASS => {
                let data_ptr = array.data as *const *const u8;
                *data_ptr.add(i) as i64
            },
            _ => continue,
            };

            if element_value == value {
                count += 1;
            }
        }

        count
    }
}

/// Create a slice of array from start to end (exclusive)
#[no_mangle]
pub extern "C" fn plat_array_slice(array_ptr: *const RuntimeArray, start: i32, end: i32) -> *mut RuntimeArray {
    if array_ptr.is_null() || start < 0 || end < start {
        return std::ptr::null_mut();
    }

    unsafe {
        let array = &*array_ptr;
        let start = start as usize;
        let end = end as usize;

        if start >= array.length || end > array.length || array.data.is_null() {
            return std::ptr::null_mut();
        }

        let slice_length = end - start;

        // Create new array with sliced elements
        match array.element_type {
            ARRAY_TYPE_I32 => {
                let data_ptr = array.data as *const i32;
                plat_array_create_i32(data_ptr.add(start), slice_length)
            },
            ARRAY_TYPE_I64 => {
                let data_ptr = array.data as *const i64;
                plat_array_create_i64(data_ptr.add(start), slice_length)
            },
            ARRAY_TYPE_BOOL => {
                let data_ptr = array.data as *const bool;
                plat_array_create_bool(data_ptr.add(start), slice_length)
            },
            ARRAY_TYPE_STRING => {
                let data_ptr = array.data as *const *const c_char;
                plat_array_create_string(data_ptr.add(start), slice_length)
            },
            ARRAY_TYPE_CLASS => {
                let data_ptr = array.data as *const *const u8;
                plat_array_create_class(data_ptr.add(start), slice_length)
            },
            _ => std::ptr::null_mut(),
        }
    }
}

/// Concatenate two arrays of the same type
#[no_mangle]
pub extern "C" fn plat_array_concat(array1_ptr: *const RuntimeArray, array2_ptr: *const RuntimeArray) -> *mut RuntimeArray {
    if array1_ptr.is_null() || array2_ptr.is_null() {
        return std::ptr::null_mut();
    }

    unsafe {
        let array1 = &*array1_ptr;
        let array2 = &*array2_ptr;

        // Arrays must be same type
        if array1.element_type != array2.element_type {
            return std::ptr::null_mut();
        }

        let total_length = array1.length + array2.length;

        // Allocate new array
        let array_size = std::mem::size_of::<RuntimeArray>();
        let new_array_ptr = plat_gc_alloc(array_size) as *mut RuntimeArray;

        if new_array_ptr.is_null() {
            return std::ptr::null_mut();
        }

        // Allocate data for combined array
        let data_size = total_length * array1.element_size;
        let new_data_ptr = if total_length > 0 {
            plat_gc_alloc(data_size)
        } else {
            std::ptr::null_mut()
        };

        if total_length > 0 && new_data_ptr.is_null() {
            return std::ptr::null_mut();
        }

        // Copy data from both arrays
        if total_length > 0 && !array1.data.is_null() && !array2.data.is_null() {
            let size1 = array1.length * array1.element_size;
            let size2 = array2.length * array2.element_size;

            std::ptr::copy_nonoverlapping(array1.data, new_data_ptr, size1);
            std::ptr::copy_nonoverlapping(array2.data, new_data_ptr.add(size1), size2);
        }

        // Initialize new array
        (*new_array_ptr) = RuntimeArray {
            data: new_data_ptr,
            length: total_length,
            capacity: total_length,
            element_size: array1.element_size,
            element_type: array1.element_type,
        };

        new_array_ptr
    }
}

/// Check if all elements satisfy predicate (simplified: check if all elements are non-zero/true)
#[no_mangle]
pub extern "C" fn plat_array_all_truthy(array_ptr: *const RuntimeArray) -> bool {
    if array_ptr.is_null() {
        return true; // Empty arrays are vacuously true
    }

    unsafe {
        let array = &*array_ptr;
        if array.data.is_null() || array.length == 0 {
            return true;
        }

        for i in 0..array.length {
            let element_value = match array.element_type {
                ARRAY_TYPE_I32 => {
                    let data_ptr = array.data as *const i32;
                    *data_ptr.add(i) as i64
                },
                ARRAY_TYPE_I64 => {
                    let data_ptr = array.data as *const i64;
                    *data_ptr.add(i)
                },
                ARRAY_TYPE_BOOL => {
                    let data_ptr = array.data as *const bool;
                    if *data_ptr.add(i) { 1 } else { 0 }
                },
                ARRAY_TYPE_STRING => {
                    let data_ptr = array.data as *const *const c_char;
                    let str_ptr = *data_ptr.add(i);
                    if str_ptr.is_null() { 0 } else { 1 }
                },
                ARRAY_TYPE_CLASS => {
                    let data_ptr = array.data as *const *const u8;
                    let class_ptr = *data_ptr.add(i);
                    if class_ptr.is_null() { 0 } else { 1 }
                },
                _ => 0,
            };

            if element_value == 0 {
                return false;
            }
        }

        true
    }
}

/// Check if any element satisfies predicate (simplified: check if any element is non-zero/true)
#[no_mangle]
pub extern "C" fn plat_array_any_truthy(array_ptr: *const RuntimeArray) -> bool {
    if array_ptr.is_null() {
        return false;
    }

    unsafe {
        let array = &*array_ptr;
        if array.data.is_null() || array.length == 0 {
            return false;
        }

        for i in 0..array.length {
            let element_value = match array.element_type {
                ARRAY_TYPE_I32 => {
                    let data_ptr = array.data as *const i32;
                    *data_ptr.add(i) as i64
                },
                ARRAY_TYPE_I64 => {
                    let data_ptr = array.data as *const i64;
                    *data_ptr.add(i)
                },
                ARRAY_TYPE_BOOL => {
                    let data_ptr = array.data as *const bool;
                    if *data_ptr.add(i) { 1 } else { 0 }
                },
                ARRAY_TYPE_STRING => {
                    let data_ptr = array.data as *const *const c_char;
                    let str_ptr = *data_ptr.add(i);
                    if str_ptr.is_null() { 0 } else { 1 }
                },
                ARRAY_TYPE_CLASS => {
                    let data_ptr = array.data as *const *const u8;
                    let class_ptr = *data_ptr.add(i);
                    if class_ptr.is_null() { 0 } else { 1 }
                },
                _ => 0,
            };

            if element_value != 0 {
                return true;
            }
        }

        false
    }
}

// =============================================================================
// Class Functions
// =============================================================================

/// Create a new class instance
#[no_mangle]
pub extern "C" fn plat_class_create(class_name_ptr: *const c_char) -> *mut PlatClass {
    if class_name_ptr.is_null() {
        return std::ptr::null_mut();
    }

    unsafe {
        let class_name = CStr::from_ptr(class_name_ptr).to_string_lossy().to_string();
        let class_instance = Box::new(PlatClass::new(class_name));
        Box::into_raw(class_instance)
    }
}

/// Set a field value in a class instance
#[no_mangle]
pub extern "C" fn plat_class_set_field_i32(
    class_ptr: *mut PlatClass,
    field_name_ptr: *const c_char,
    value: i32
) {
    if class_ptr.is_null() || field_name_ptr.is_null() {
        return;
    }

    unsafe {
        let class = &mut *class_ptr;
        let field_name = CStr::from_ptr(field_name_ptr).to_string_lossy().to_string();
        class.set_field(field_name, PlatValue::I32(value));
    }
}

/// Set a string field value in a class instance
#[no_mangle]
pub extern "C" fn plat_class_set_field_string(
    class_ptr: *mut PlatClass,
    field_name_ptr: *const c_char,
    value_ptr: *const c_char
) {
    if class_ptr.is_null() || field_name_ptr.is_null() || value_ptr.is_null() {
        return;
    }

    unsafe {
        let class = &mut *class_ptr;
        let field_name = CStr::from_ptr(field_name_ptr).to_string_lossy().to_string();
        let value_str = CStr::from_ptr(value_ptr).to_string_lossy().to_string();
        let plat_string = PlatString { data: Gc::new(value_str) };
        class.set_field(field_name, PlatValue::String(plat_string));
    }
}

/// Get an i32 field value from a class instance
#[no_mangle]
pub extern "C" fn plat_class_get_field_i32(
    class_ptr: *const PlatClass,
    field_name_ptr: *const c_char
) -> i32 {
    if class_ptr.is_null() || field_name_ptr.is_null() {
        return 0;
    }

    unsafe {
        let class = &*class_ptr;
        let field_name = CStr::from_ptr(field_name_ptr).to_string_lossy();
        match class.get_field(&field_name) {
            Some(PlatValue::I32(value)) => value,
            _ => 0, // Return default if field not found or wrong type
        }
    }
}

/// Get a string field value from a class instance (returns pointer to C string)
#[no_mangle]
pub extern "C" fn plat_class_get_field_string(
    class_ptr: *const PlatClass,
    field_name_ptr: *const c_char
) -> *const c_char {
    if class_ptr.is_null() || field_name_ptr.is_null() {
        return std::ptr::null();
    }

    unsafe {
        let class = &*class_ptr;
        let field_name = CStr::from_ptr(field_name_ptr).to_string_lossy();
        match class.get_field(&field_name) {
            Some(PlatValue::String(ref plat_string)) => {
                let c_string = CString::new(plat_string.data.as_str()).unwrap();
                c_string.into_raw() // Caller is responsible for freeing
            },
            _ => std::ptr::null(),
        }
    }
}

/// Convert class instance to string representation
#[no_mangle]
pub extern "C" fn plat_class_to_string(class_ptr: *const PlatClass) -> *const c_char {
    if class_ptr.is_null() {
        return std::ptr::null();
    }

    unsafe {
        let class = &*class_ptr;
        let string_repr = format!("{}", class);
        let c_string = CString::new(string_repr).unwrap();
        c_string.into_raw() // Caller is responsible for freeing
    }
}

// =============================================================================
// Point Class Method Implementations
// =============================================================================

/// Point::add method - adds two Point instances and returns a new Point
#[no_mangle]
pub extern "C" fn Point__add(self_ptr: *const PlatClass, other_ptr: *const PlatClass) -> *mut PlatClass {
    if self_ptr.is_null() || other_ptr.is_null() {
        return std::ptr::null_mut();
    }

    unsafe {
        let self_point = &*self_ptr;
        let other_point = &*other_ptr;

        // Get x and y values from both points
        let self_x = match self_point.get_field("x") {
            Some(PlatValue::I32(x)) => x,
            _ => 0,
        };
        let self_y = match self_point.get_field("y") {
            Some(PlatValue::I32(y)) => y,
            _ => 0,
        };

        let other_x = match other_point.get_field("x") {
            Some(PlatValue::I32(x)) => x,
            _ => 0,
        };
        let other_y = match other_point.get_field("y") {
            Some(PlatValue::I32(y)) => y,
            _ => 0,
        };

        // Create new point with sum
        let new_point = Box::new(PlatClass::new("Point".to_string()));
        let new_point_ptr = Box::into_raw(new_point);

        // Set fields for new point
        plat_class_set_field_i32(new_point_ptr, CString::new("x").unwrap().as_ptr(), self_x + other_x);
        plat_class_set_field_i32(new_point_ptr, CString::new("y").unwrap().as_ptr(), self_y + other_y);
        plat_class_set_field_string(new_point_ptr, CString::new("name").unwrap().as_ptr(), CString::new("sum").unwrap().as_ptr());

        new_point_ptr
    }
}

/// Point::change_name method - changes the name field of the point
#[no_mangle]
pub extern "C" fn Point__change_name(self_ptr: *mut PlatClass, new_name_ptr: *const c_char) {
    if self_ptr.is_null() || new_name_ptr.is_null() {
        return;
    }

    // SAFETY: plat_class_set_field_string is already an unsafe function
    plat_class_set_field_string(self_ptr, CString::new("name").unwrap().as_ptr(), new_name_ptr);
}

/// Point::get_magnitude method - calculates x*x + y*y
#[no_mangle]
pub extern "C" fn Point__get_magnitude(self_ptr: *const PlatClass) -> i32 {
    if self_ptr.is_null() {
        return 0;
    }

    unsafe {
        let point = &*self_ptr;

        let x = match point.get_field("x") {
            Some(PlatValue::I32(x)) => x,
            _ => 0,
        };
        let y = match point.get_field("y") {
            Some(PlatValue::I32(y)) => y,
            _ => 0,
        };

        x * x + y * y
    }
}
