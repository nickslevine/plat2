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

// Global task handle registry (shared between spawn and await)
lazy_static::lazy_static! {
    static ref TASK_HANDLES: std::sync::Mutex<std::collections::HashMap<u64, std::sync::Arc<dyn std::any::Any + Send + Sync>>> =
        std::sync::Mutex::new(std::collections::HashMap::new());
}

/// Spawn a task that returns an i64 value
/// Returns an opaque task handle (pointer)
#[no_mangle]
pub extern "C" fn plat_spawn_task_i64(func: extern "C" fn() -> i64) -> u64 {
    use green_runtime::{GreenThreadRuntime, task_with_result::TaskWithResult, get_scope_registry};
    use std::sync::Arc;

    let task = TaskWithResult::new(move || func());
    let handle = task.handle();
    let task_id = task.id().as_u64();

    // Register the handle with the current scope (for structured concurrency)
    let scope_registry = get_scope_registry();
    scope_registry.register_task(handle.clone());

    let runtime = GreenThreadRuntime::get();
    let mut guard = runtime.lock();
    if let Some(rt) = guard.as_mut() {
        rt.spawn_with_result(task);
    }

    // Store handle in global registry using task_id as key
    TASK_HANDLES.lock().unwrap().insert(task_id, Arc::new(handle));

    task_id
}

/// Await a task and get its i64 result
/// Blocks until the task completes
#[no_mangle]
pub extern "C" fn plat_task_await_i64(handle_id: u64) -> i64 {
    use green_runtime::task_with_result::TaskHandle;

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

/// Spawn a task that returns an i32 value
#[no_mangle]
pub extern "C" fn plat_spawn_task_i32(func: extern "C" fn() -> i32) -> u64 {
    use green_runtime::{GreenThreadRuntime, task_with_result::TaskWithResult, get_scope_registry};
    use std::sync::Arc;

    let task = TaskWithResult::new(move || func());
    let handle = task.handle();
    let task_id = task.id().as_u64();

    let scope_registry = get_scope_registry();
    scope_registry.register_task(handle.clone());

    let runtime = GreenThreadRuntime::get();
    let mut guard = runtime.lock();
    if let Some(rt) = guard.as_mut() {
        rt.spawn_with_result(task);
    }

    TASK_HANDLES.lock().unwrap().insert(task_id, Arc::new(handle));
    task_id
}

/// Await a task and get its i32 result
#[no_mangle]
pub extern "C" fn plat_task_await_i32(handle_id: u64) -> i32 {
    use green_runtime::task_with_result::TaskHandle;

    let handles = TASK_HANDLES.lock().unwrap();
    if let Some(handle_any) = handles.get(&handle_id) {
        if let Some(handle) = handle_any.downcast_ref::<TaskHandle<i32>>() {
            return handle.await_result().unwrap_or(0);
        }
    }
    0
}

/// Spawn a task that returns a bool value
#[no_mangle]
pub extern "C" fn plat_spawn_task_bool(func: extern "C" fn() -> bool) -> u64 {
    use green_runtime::{GreenThreadRuntime, task_with_result::TaskWithResult, get_scope_registry};
    use std::sync::Arc;

    let task = TaskWithResult::new(move || func());
    let handle = task.handle();
    let task_id = task.id().as_u64();

    let scope_registry = get_scope_registry();
    scope_registry.register_task(handle.clone());

    let runtime = GreenThreadRuntime::get();
    let mut guard = runtime.lock();
    if let Some(rt) = guard.as_mut() {
        rt.spawn_with_result(task);
    }

    TASK_HANDLES.lock().unwrap().insert(task_id, Arc::new(handle));
    task_id
}

/// Await a task and get its bool result
#[no_mangle]
pub extern "C" fn plat_task_await_bool(handle_id: u64) -> bool {
    use green_runtime::task_with_result::TaskHandle;

    let handles = TASK_HANDLES.lock().unwrap();
    if let Some(handle_any) = handles.get(&handle_id) {
        if let Some(handle) = handle_any.downcast_ref::<TaskHandle<bool>>() {
            return handle.await_result().unwrap_or(false);
        }
    }
    false
}

/// Spawn a task that returns an f32 value
#[no_mangle]
pub extern "C" fn plat_spawn_task_f32(func: extern "C" fn() -> f32) -> u64 {
    use green_runtime::{GreenThreadRuntime, task_with_result::TaskWithResult, get_scope_registry};
    use std::sync::Arc;

    let task = TaskWithResult::new(move || func());
    let handle = task.handle();
    let task_id = task.id().as_u64();

    let scope_registry = get_scope_registry();
    scope_registry.register_task(handle.clone());

    let runtime = GreenThreadRuntime::get();
    let mut guard = runtime.lock();
    if let Some(rt) = guard.as_mut() {
        rt.spawn_with_result(task);
    }

    TASK_HANDLES.lock().unwrap().insert(task_id, Arc::new(handle));
    task_id
}

/// Await a task and get its f32 result
#[no_mangle]
pub extern "C" fn plat_task_await_f32(handle_id: u64) -> f32 {
    use green_runtime::task_with_result::TaskHandle;

    let handles = TASK_HANDLES.lock().unwrap();
    if let Some(handle_any) = handles.get(&handle_id) {
        if let Some(handle) = handle_any.downcast_ref::<TaskHandle<f32>>() {
            return handle.await_result().unwrap_or(0.0);
        }
    }
    0.0
}

/// Spawn a task that returns an f64 value
#[no_mangle]
pub extern "C" fn plat_spawn_task_f64(func: extern "C" fn() -> f64) -> u64 {
    use green_runtime::{GreenThreadRuntime, task_with_result::TaskWithResult, get_scope_registry};
    use std::sync::Arc;

    let task = TaskWithResult::new(move || func());
    let handle = task.handle();
    let task_id = task.id().as_u64();

    let scope_registry = get_scope_registry();
    scope_registry.register_task(handle.clone());

    let runtime = GreenThreadRuntime::get();
    let mut guard = runtime.lock();
    if let Some(rt) = guard.as_mut() {
        rt.spawn_with_result(task);
    }

    TASK_HANDLES.lock().unwrap().insert(task_id, Arc::new(handle));
    task_id
}

/// Await a task and get its f64 result
#[no_mangle]
pub extern "C" fn plat_task_await_f64(handle_id: u64) -> f64 {
    use green_runtime::task_with_result::TaskHandle;

    let handles = TASK_HANDLES.lock().unwrap();
    if let Some(handle_any) = handles.get(&handle_id) {
        if let Some(handle) = handle_any.downcast_ref::<TaskHandle<f64>>() {
            return handle.await_result().unwrap_or(0.0);
        }
    }
    0.0
}

// ============================================================================
// Context-aware Spawn Functions (for variable capture)
// ============================================================================

/// Spawn a task with context that returns an i32 value
/// The context pointer is passed to the closure
#[no_mangle]
pub extern "C" fn plat_spawn_task_i32_ctx(func: extern "C" fn(*mut u8) -> i32, ctx: *mut u8) -> u64 {
    use green_runtime::{GreenThreadRuntime, task_with_result::TaskWithResult, get_scope_registry};
    use std::sync::Arc;

    // Convert raw pointer to usize for Send safety
    let ctx_addr = ctx as usize;
    let task = TaskWithResult::new(move || {
        let ctx_ptr = ctx_addr as *mut u8;
        func(ctx_ptr)
    });
    let handle = task.handle();
    let task_id = task.id().as_u64();

    let scope_registry = get_scope_registry();
    scope_registry.register_task(handle.clone());

    let runtime = GreenThreadRuntime::get();
    let mut guard = runtime.lock();
    if let Some(rt) = guard.as_mut() {
        rt.spawn_with_result(task);
    }

    TASK_HANDLES.lock().unwrap().insert(task_id, Arc::new(handle));
    task_id
}

/// Spawn a task with context that returns an i64 value
#[no_mangle]
pub extern "C" fn plat_spawn_task_i64_ctx(func: extern "C" fn(*mut u8) -> i64, ctx: *mut u8) -> u64 {
    use green_runtime::{GreenThreadRuntime, task_with_result::TaskWithResult, get_scope_registry};
    use std::sync::Arc;

    let ctx_addr = ctx as usize;
    let task = TaskWithResult::new(move || {
        let ctx_ptr = ctx_addr as *mut u8;
        func(ctx_ptr)
    });
    let handle = task.handle();
    let task_id = task.id().as_u64();

    let scope_registry = get_scope_registry();
    scope_registry.register_task(handle.clone());

    let runtime = GreenThreadRuntime::get();
    let mut guard = runtime.lock();
    if let Some(rt) = guard.as_mut() {
        rt.spawn_with_result(task);
    }

    TASK_HANDLES.lock().unwrap().insert(task_id, Arc::new(handle));
    task_id
}

/// Spawn a task with context that returns a bool value
#[no_mangle]
pub extern "C" fn plat_spawn_task_bool_ctx(func: extern "C" fn(*mut u8) -> bool, ctx: *mut u8) -> u64 {
    use green_runtime::{GreenThreadRuntime, task_with_result::TaskWithResult, get_scope_registry};
    use std::sync::Arc;

    let ctx_addr = ctx as usize;
    let task = TaskWithResult::new(move || {
        let ctx_ptr = ctx_addr as *mut u8;
        func(ctx_ptr)
    });
    let handle = task.handle();
    let task_id = task.id().as_u64();

    let scope_registry = get_scope_registry();
    scope_registry.register_task(handle.clone());

    let runtime = GreenThreadRuntime::get();
    let mut guard = runtime.lock();
    if let Some(rt) = guard.as_mut() {
        rt.spawn_with_result(task);
    }

    TASK_HANDLES.lock().unwrap().insert(task_id, Arc::new(handle));
    task_id
}

/// Spawn a task with context that returns an f32 value
#[no_mangle]
pub extern "C" fn plat_spawn_task_f32_ctx(func: extern "C" fn(*mut u8) -> f32, ctx: *mut u8) -> u64 {
    use green_runtime::{GreenThreadRuntime, task_with_result::TaskWithResult, get_scope_registry};
    use std::sync::Arc;

    let ctx_addr = ctx as usize;
    let task = TaskWithResult::new(move || {
        let ctx_ptr = ctx_addr as *mut u8;
        func(ctx_ptr)
    });
    let handle = task.handle();
    let task_id = task.id().as_u64();

    let scope_registry = get_scope_registry();
    scope_registry.register_task(handle.clone());

    let runtime = GreenThreadRuntime::get();
    let mut guard = runtime.lock();
    if let Some(rt) = guard.as_mut() {
        rt.spawn_with_result(task);
    }

    TASK_HANDLES.lock().unwrap().insert(task_id, Arc::new(handle));
    task_id
}

/// Spawn a task with context that returns an f64 value
#[no_mangle]
pub extern "C" fn plat_spawn_task_f64_ctx(func: extern "C" fn(*mut u8) -> f64, ctx: *mut u8) -> u64 {
    use green_runtime::{GreenThreadRuntime, task_with_result::TaskWithResult, get_scope_registry};
    use std::sync::Arc;

    let ctx_addr = ctx as usize;
    let task = TaskWithResult::new(move || {
        let ctx_ptr = ctx_addr as *mut u8;
        func(ctx_ptr)
    });
    let handle = task.handle();
    let task_id = task.id().as_u64();

    let scope_registry = get_scope_registry();
    scope_registry.register_task(handle.clone());

    let runtime = GreenThreadRuntime::get();
    let mut guard = runtime.lock();
    if let Some(rt) = guard.as_mut() {
        rt.spawn_with_result(task);
    }

    TASK_HANDLES.lock().unwrap().insert(task_id, Arc::new(handle));
    task_id
}

// ============================================================================
// Scope Management for Structured Concurrency
// ============================================================================

/// Enter a new concurrent scope
/// Returns the scope ID
#[no_mangle]
pub extern "C" fn plat_scope_enter() -> u64 {
    use green_runtime::get_scope_registry;

    let registry = get_scope_registry();
    let scope_id = registry.enter_scope();
    scope_id.as_u64()
}

/// Exit a concurrent scope (waits for all spawned tasks)
#[no_mangle]
pub extern "C" fn plat_scope_exit(scope_id: u64) {
    use green_runtime::{get_scope_registry, scope::ScopeId};

    let registry = get_scope_registry();

    // We need to reconstruct the ScopeId from the u64
    // This is a bit hacky but works for now
    // In a real implementation, we might want to validate the scope_id

    // For now, we'll just call exit_scope with the raw value
    // We need a way to create a ScopeId from a u64
    // Let's add a from_u64 method to ScopeId

    registry.exit_scope(green_runtime::scope::ScopeId::from_u64(scope_id));
}
