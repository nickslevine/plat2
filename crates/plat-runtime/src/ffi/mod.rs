// FFI module for C-compatible runtime functions

pub mod core;
pub mod conversions;
pub mod string;
pub mod array;
pub mod dict;
pub mod set;
pub mod class;
pub mod gc_bindings;
pub mod net;
pub mod fs;

// Re-export commonly used items
pub use array::{RuntimeArray, ARRAY_TYPE_I32, ARRAY_TYPE_I64, ARRAY_TYPE_BOOL, ARRAY_TYPE_STRING, ARRAY_TYPE_CLASS};
pub use dict::{RuntimeDict, DICT_KEY_TYPE_STRING, DICT_VALUE_TYPE_I32, DICT_VALUE_TYPE_I64, DICT_VALUE_TYPE_BOOL, DICT_VALUE_TYPE_STRING};
pub use set::{RuntimeSet, SET_VALUE_TYPE_I32, SET_VALUE_TYPE_I64, SET_VALUE_TYPE_BOOL, SET_VALUE_TYPE_STRING};
pub use gc_bindings::*;
