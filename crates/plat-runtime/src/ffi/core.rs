use std::ffi::CStr;
use std::os::raw::c_char;
use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};
use super::gc_bindings::{gc_alloc, init_gc, gc_collect, gc_stats};

static GC_INIT: Once = Once::new();

// Global flag to track if the current test has failed
static TEST_FAILED: AtomicBool = AtomicBool::new(false);

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

/// C-compatible assert function for test mode that returns a Bool instead of exiting
///
/// # Arguments
/// * `condition` - Boolean condition to check
/// * `message_ptr` - Pointer to optional error message (can be null)
///
/// # Returns
/// * `true` if the assertion passed, `false` if it failed
///
/// # Safety
/// This function is unsafe because it dereferences raw pointers
#[no_mangle]
pub extern "C" fn plat_assert_test(condition: bool, message_ptr: *const c_char) -> bool {
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

        eprintln!("  ✗ {}", message);
        TEST_FAILED.store(true, Ordering::Relaxed);
        false
    } else {
        true
    }
}

/// Reset the test failure flag before running a new test
#[no_mangle]
pub extern "C" fn plat_test_reset() {
    TEST_FAILED.store(false, Ordering::Relaxed);
}

/// Check if the current test has failed
///
/// # Returns
/// * `true` if any assertion in the current test has failed, `false` otherwise
#[no_mangle]
pub extern "C" fn plat_test_check() -> bool {
    TEST_FAILED.load(Ordering::Relaxed)
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

/// C-compatible GC allocation function for pointer-free data (atomic)
///
/// This is an optimized version of plat_gc_alloc for data that contains no pointers.
/// The GC will not scan this memory for references, making collection faster.
///
/// Use for: strings, primitive arrays (Int32[], Bool[], etc.), numeric data
///
/// # Safety
/// This function is unsafe because it returns raw pointers to GC memory
#[no_mangle]
pub extern "C" fn plat_gc_alloc_atomic(size: usize) -> *mut u8 {
    ensure_gc_initialized();

    // Allocate using Boehm GC atomic mode (no pointer scanning)
    let ptr = gc_alloc(size, true);

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
