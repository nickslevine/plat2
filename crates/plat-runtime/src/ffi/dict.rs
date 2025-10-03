use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use super::core::plat_gc_alloc;
use super::array::{plat_array_create_string, RuntimeArray, ARRAY_TYPE_I32, ARRAY_TYPE_STRING};

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
    pub(crate) keys: *mut *const c_char,    // Array of string keys (null-terminated)
    pub(crate) values: *mut i64,            // Array of values (all as i64 for simplicity)
    pub(crate) value_types: *mut u8,        // Array indicating the type of each value
    pub(crate) length: usize,
    pub(crate) capacity: usize,
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
