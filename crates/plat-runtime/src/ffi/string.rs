use std::ffi::CStr;
use std::os::raw::c_char;
use super::core::plat_gc_alloc;
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
