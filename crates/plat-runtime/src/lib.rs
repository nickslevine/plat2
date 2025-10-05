#[cfg(test)]
mod tests;

// Module declarations
mod types;
mod errors;
mod runtime;
pub mod ffi;

// Concurrency runtime (green threads)
pub mod green_runtime;

// Re-export public types
pub use types::{
    PlatValue, PlatString, PlatArray, PlatDict, PlatSet, PlatClass,
};
pub use errors::RuntimeError;
pub use runtime::Runtime;

// Re-export FFI types
pub use ffi::{
    RuntimeArray, RuntimeDict, RuntimeSet,
    ARRAY_TYPE_I32, ARRAY_TYPE_I64, ARRAY_TYPE_BOOL, ARRAY_TYPE_STRING, ARRAY_TYPE_CLASS,
    DICT_KEY_TYPE_STRING, DICT_VALUE_TYPE_I32, DICT_VALUE_TYPE_I64, DICT_VALUE_TYPE_BOOL, DICT_VALUE_TYPE_STRING,
    SET_VALUE_TYPE_I32, SET_VALUE_TYPE_I64, SET_VALUE_TYPE_BOOL, SET_VALUE_TYPE_STRING,
};

// ============================================================================
// Concurrency C FFI
// ============================================================================

use green_runtime::{runtime_init, runtime_shutdown};

/// Initialize the green thread runtime
#[no_mangle]
pub extern "C" fn plat_runtime_init() {
    runtime_init();
}

/// Shutdown the green thread runtime
#[no_mangle]
pub extern "C" fn plat_runtime_shutdown() {
    runtime_shutdown();
}

/// Spawn a new task (basic version - takes function pointer)
/// Returns the task ID
#[no_mangle]
pub extern "C" fn plat_spawn_task(func: extern "C" fn()) -> u64 {
    use green_runtime::{GreenThreadRuntime, task::Task};

    let task = Task::new(move || {
        func();
    });

    let task_id = task.id();

    let runtime = GreenThreadRuntime::get();
    let mut guard = runtime.lock();
    if let Some(rt) = guard.as_mut() {
        rt.spawn(task);
    }

    task_id.as_u64()
}

/// Spawn a task that returns an i64 value
/// Returns an opaque task handle (pointer)
#[no_mangle]
pub extern "C" fn plat_spawn_task_i64(func: extern "C" fn() -> i64) -> u64 {
    use green_runtime::{GreenThreadRuntime, task_with_result::TaskWithResult};
    use std::sync::Arc;

    let task = TaskWithResult::new(move || func());
    let handle = task.handle();
    let task_id = task.id().as_u64();

    let runtime = GreenThreadRuntime::get();
    let mut guard = runtime.lock();
    if let Some(rt) = guard.as_mut() {
        rt.spawn_with_result(task);
    }

    // Store handle in global registry and return handle ID
    use std::sync::Mutex;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU64, Ordering};

    lazy_static::lazy_static! {
        static ref TASK_HANDLES: Mutex<HashMap<u64, Arc<dyn std::any::Any + Send + Sync>>> = Mutex::new(HashMap::new());
        static ref NEXT_HANDLE_ID: AtomicU64 = AtomicU64::new(1);
    }

    let handle_id = NEXT_HANDLE_ID.fetch_add(1, Ordering::SeqCst);
    TASK_HANDLES.lock().unwrap().insert(handle_id, Arc::new(handle));

    handle_id
}

/// Await a task and get its i64 result
/// Blocks until the task completes
#[no_mangle]
pub extern "C" fn plat_task_await_i64(handle_id: u64) -> i64 {
    use green_runtime::task_with_result::TaskHandle;
    use std::sync::Mutex;
    use std::collections::HashMap;
    use std::sync::Arc;

    lazy_static::lazy_static! {
        static ref TASK_HANDLES: Mutex<HashMap<u64, Arc<dyn std::any::Any + Send + Sync>>> = Mutex::new(HashMap::new());
    }

    // Retrieve handle from registry
    let handles = TASK_HANDLES.lock().unwrap();
    if let Some(handle_any) = handles.get(&handle_id) {
        if let Some(handle) = handle_any.downcast_ref::<TaskHandle<i64>>() {
            // Wait for result
            return handle.await_result().unwrap_or(0);
        }
    }

    // If handle not found or wrong type, return 0
    0
}
