use std::ffi::CStr;
use std::os::raw::c_char;

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
