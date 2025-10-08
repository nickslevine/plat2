use std::env;
use std::ffi::CString;
use std::os::raw::c_char;
use super::core::plat_gc_alloc;

/// Compute variant discriminant using same hash as codegen
fn variant_hash(name: &str) -> u32 {
    let mut hash = 0u32;
    for byte in name.bytes() {
        hash = hash.wrapping_mul(31).wrapping_add(byte as u32);
    }
    hash
}

/// Get an environment variable
/// Returns an Option<String> enum pointer
#[no_mangle]
pub extern "C" fn plat_env_get(name_ptr: *const c_char) -> i64 {
    unsafe fn create_option_none() -> i64 {
        let none_disc = variant_hash("None");
        // Heap-allocated: [discriminant:i32][padding:i32][dummy:i64]
        let ptr = plat_gc_alloc(16) as *mut i32;
        *ptr = none_disc as i32;
        ptr as i64
    }

    unsafe fn create_option_some(value_ptr: i64) -> i64 {
        let some_disc = variant_hash("Some");
        // Heap-allocated: [discriminant:i32][padding:i32][value_ptr:i64]
        let ptr = plat_gc_alloc(16) as *mut i32;
        *ptr = some_disc as i32;
        let val_ptr = ptr.add(2) as *mut i64;
        *val_ptr = value_ptr;
        ptr as i64
    }

    if name_ptr.is_null() {
        return unsafe { create_option_none() };
    }

    let name_str = unsafe {
        match std::ffi::CStr::from_ptr(name_ptr).to_str() {
            Ok(s) => s,
            Err(_) => return create_option_none(),
        }
    };

    match env::var(name_str) {
        Ok(value) => {
            match CString::new(value) {
                Ok(c_string) => {
                    let value_ptr = c_string.into_raw() as i64;
                    unsafe { create_option_some(value_ptr) }
                }
                Err(_) => unsafe { create_option_none() },
            }
        }
        Err(_) => unsafe { create_option_none() },
    }
}

/// Set an environment variable
/// Returns 1 on success, 0 on failure
#[no_mangle]
pub extern "C" fn plat_env_set(name: *const c_char, value: *const c_char) -> i32 {
    if name.is_null() || value.is_null() {
        return 0;
    }

    let name_str = unsafe {
        match std::ffi::CStr::from_ptr(name).to_str() {
            Ok(s) => s,
            Err(_) => return 0,
        }
    };

    let value_str = unsafe {
        match std::ffi::CStr::from_ptr(value).to_str() {
            Ok(s) => s,
            Err(_) => return 0,
        }
    };

    env::set_var(name_str, value_str);
    1
}

/// Get all environment variables as a newline-separated string
/// Returns null pointer on error
#[no_mangle]
pub extern "C" fn plat_env_vars() -> *mut c_char {
    let mut result = String::new();

    for (key, value) in env::vars() {
        result.push_str(&key);
        result.push('=');
        result.push_str(&value);
        result.push('\n');
    }

    match CString::new(result) {
        Ok(c_string) => c_string.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}
