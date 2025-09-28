#[cfg(test)]
mod tests;

use std::fmt;
use std::ffi::CStr;
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
pub extern "C" fn plat_dict_len(dict_ptr: *const RuntimeDict) -> usize {
    if dict_ptr.is_null() {
        return 0;
    }

    unsafe {
        (*dict_ptr).length
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