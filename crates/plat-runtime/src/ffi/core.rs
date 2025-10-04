use std::ffi::CStr;
use std::os::raw::c_char;
use std::sync::Once;
use super::gc_bindings::{gc_alloc, init_gc, gc_collect, gc_stats};

static GC_INIT: Once = Once::new();

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

/// Initialize GC on first allocation
fn ensure_gc_initialized() {
    GC_INIT.call_once(|| {
        init_gc();
        eprintln!("[GC] Boehm GC initialized");
    });
}

/// C-compatible GC allocation function that can be called from generated code
///
/// # Safety
/// This function is unsafe because it returns raw pointers to GC memory
#[no_mangle]
pub extern "C" fn plat_gc_alloc(size: usize) -> *mut u8 {
    ensure_gc_initialized();

    // Allocate using Boehm GC (conservative, scans for pointers)
    let ptr = gc_alloc(size, false);

    if ptr.is_null() {
        eprintln!("[GC] FATAL: Out of memory (requested {} bytes)", size);
        std::process::abort();
    }

    // Zero the memory (Boehm GC doesn't guarantee zeroing)
    unsafe {
        std::ptr::write_bytes(ptr, 0, size);
    }

    ptr
}

/// C-compatible GC collection function that can be called from generated code
#[no_mangle]
pub extern "C" fn plat_gc_collect() {
    gc_collect();
}

/// C-compatible function to get GC stats (returns heap size)
#[no_mangle]
pub extern "C" fn plat_gc_stats() -> usize {
    let stats = gc_stats();
    stats.heap_size
}
