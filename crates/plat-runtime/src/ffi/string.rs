use std::ffi::CStr;
use std::os::raw::c_char;
use super::core::{plat_gc_alloc, plat_gc_alloc_atomic};
use super::array::plat_array_create_string;

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
        let gc_ptr = plat_gc_alloc_atomic(size);

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
        let gc_ptr = plat_gc_alloc_atomic(size);

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
        let gc_ptr = plat_gc_alloc_atomic(size);

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
        let gc_ptr = plat_gc_alloc_atomic(size);

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
        let gc_ptr = plat_gc_alloc_atomic(size);

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
        let gc_ptr = plat_gc_alloc_atomic(size);

        if gc_ptr.is_null() {
            return std::ptr::null();
        }

        std::ptr::copy_nonoverlapping(result_bytes.as_ptr(), gc_ptr, size);
        gc_ptr as *const c_char
    }
}

/// Split string by delimiter and return as string array
#[no_mangle]
pub extern "C" fn plat_string_split(str_ptr: *const c_char, delimiter_ptr: *const c_char) -> *mut crate::ffi::RuntimeArray {
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
            let gc_ptr = plat_gc_alloc_atomic(size);

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

/// Helper to create error message on GC heap
unsafe fn create_error_message(msg: &str) -> *const c_char {
    let mut msg_bytes = msg.as_bytes().to_vec();
    msg_bytes.push(0); // null terminator

    let size = msg_bytes.len();
    let gc_ptr = plat_gc_alloc_atomic(size);

    if gc_ptr.is_null() {
        return std::ptr::null();
    }

    std::ptr::copy_nonoverlapping(msg_bytes.as_ptr(), gc_ptr, size);
    gc_ptr as *const c_char
}

/// Compute variant discriminant using same hash as codegen
fn variant_hash(name: &str) -> u32 {
    let mut hash = 0u32;
    for byte in name.bytes() {
        hash = hash.wrapping_mul(31).wrapping_add(byte as u32);
    }
    hash
}

/// Parse string to Int32
/// Returns Result<Int32, String> as heap-allocated enum
#[no_mangle]
pub extern "C" fn plat_string_parse_int(str_ptr: *const c_char) -> i64 {
    unsafe {
        if str_ptr.is_null() {
            let err_msg = create_error_message("Cannot parse null string");
            return create_result_enum_err_string(err_msg);
        }

        let str_val = match CStr::from_ptr(str_ptr).to_str() {
            Ok(s) => s.trim(),
            Err(_) => {
                let err_msg = create_error_message("Invalid UTF-8 string");
                return create_result_enum_err_string(err_msg);
            }
        };

        match str_val.parse::<i32>() {
            Ok(val) => create_result_enum_ok_i32(val),
            Err(_) => {
                let err_msg = create_error_message(&format!("Cannot parse '{}' as Int32", str_val));
                create_result_enum_err_string(err_msg)
            }
        }
    }
}

/// Parse string to Int64
#[no_mangle]
pub extern "C" fn plat_string_parse_int64(str_ptr: *const c_char) -> i64 {
    unsafe {
        if str_ptr.is_null() {
            let err_msg = create_error_message("Cannot parse null string");
            return create_result_enum_err_string(err_msg);
        }

        let str_val = match CStr::from_ptr(str_ptr).to_str() {
            Ok(s) => s.trim(),
            Err(_) => {
                let err_msg = create_error_message("Invalid UTF-8 string");
                return create_result_enum_err_string(err_msg);
            }
        };

        match str_val.parse::<i64>() {
            Ok(val) => create_result_enum_ok_i64(val),
            Err(_) => {
                let err_msg = create_error_message(&format!("Cannot parse '{}' as Int64", str_val));
                create_result_enum_err_string(err_msg)
            }
        }
    }
}

/// Parse string to Float64
#[no_mangle]
pub extern "C" fn plat_string_parse_float(str_ptr: *const c_char) -> i64 {
    unsafe {
        if str_ptr.is_null() {
            let err_msg = create_error_message("Cannot parse null string");
            return create_result_enum_err_string(err_msg);
        }

        let str_val = match CStr::from_ptr(str_ptr).to_str() {
            Ok(s) => s.trim(),
            Err(_) => {
                let err_msg = create_error_message("Invalid UTF-8 string");
                return create_result_enum_err_string(err_msg);
            }
        };

        match str_val.parse::<f64>() {
            Ok(val) => create_result_enum_ok_f64(val),
            Err(_) => {
                let err_msg = create_error_message(&format!("Cannot parse '{}' as Float64", str_val));
                create_result_enum_err_string(err_msg)
            }
        }
    }
}

/// Parse string to Bool
#[no_mangle]
pub extern "C" fn plat_string_parse_bool(str_ptr: *const c_char) -> i64 {
    unsafe {
        if str_ptr.is_null() {
            let err_msg = create_error_message("Cannot parse null string");
            return create_result_enum_err_string(err_msg);
        }

        let str_val = match CStr::from_ptr(str_ptr).to_str() {
            Ok(s) => s.trim().to_lowercase(),
            Err(_) => {
                let err_msg = create_error_message("Invalid UTF-8 string");
                return create_result_enum_err_string(err_msg);
            }
        };

        let bool_val = match str_val.as_str() {
            "true" => true,
            "false" => false,
            _ => {
                let err_msg = create_error_message(&format!("Cannot parse '{}' as Bool (expected 'true' or 'false')", str_val));
                return create_result_enum_err_string(err_msg);
            }
        };

        create_result_enum_ok_bool(bool_val)
    }
}

/// Create Result::Ok(i32) enum value
unsafe fn create_result_enum_ok_i32(value: i32) -> i64 {
    let ok_disc = variant_hash("Ok");
    // Heap-allocated: [discriminant:i32][value:i32]
    let ptr = plat_gc_alloc(8) as *mut i32;
    *ptr = ok_disc as i32;
    *ptr.add(1) = value;
    ptr as i64
}

/// Create Result::Ok(i64) enum value
unsafe fn create_result_enum_ok_i64(value: i64) -> i64 {
    let ok_disc = variant_hash("Ok");
    // Heap-allocated: [discriminant:i32][padding:i32][value:i64]
    let ptr = plat_gc_alloc(16) as *mut i32;
    *ptr = ok_disc as i32;
    let value_ptr = ptr.add(2) as *mut i64;
    *value_ptr = value;
    ptr as i64
}

/// Create Result::Ok(f64) enum value
unsafe fn create_result_enum_ok_f64(value: f64) -> i64 {
    let ok_disc = variant_hash("Ok");
    // Heap-allocated: [discriminant:i32][padding:i32][value:f64]
    let ptr = plat_gc_alloc(16) as *mut i32;
    *ptr = ok_disc as i32;
    let value_ptr = ptr.add(2) as *mut f64;
    *value_ptr = value;
    ptr as i64
}

/// Create Result::Ok(bool) enum value
unsafe fn create_result_enum_ok_bool(value: bool) -> i64 {
    let ok_disc = variant_hash("Ok");
    // Heap-allocated: [discriminant:i32][value:i32]
    let ptr = plat_gc_alloc(8) as *mut i32;
    *ptr = ok_disc as i32;
    *ptr.add(1) = if value { 1 } else { 0 };
    ptr as i64
}

/// Create Result::Err(String) enum value
unsafe fn create_result_enum_err_string(error_msg: *const c_char) -> i64 {
    let err_disc = variant_hash("Err");
    // Heap-allocated: [discriminant:i32][padding:i32][error_ptr:i64]
    let ptr = plat_gc_alloc(16) as *mut i32;
    *ptr = err_disc as i32;
    let msg_ptr = ptr.add(2) as *mut i64;
    *msg_ptr = error_msg as i64;
    ptr as i64
}
