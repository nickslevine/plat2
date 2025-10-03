use std::ffi::CStr;
use std::os::raw::c_char;
use super::core::plat_gc_alloc;

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
