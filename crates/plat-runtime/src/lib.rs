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
