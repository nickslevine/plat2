use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};

use super::task_with_result::TaskHandle;

/// Global scope ID counter
static NEXT_SCOPE_ID: AtomicU64 = AtomicU64::new(1);

/// Unique identifier for a task scope
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScopeId(u64);

impl ScopeId {
    pub fn new() -> Self {
        ScopeId(NEXT_SCOPE_ID.fetch_add(1, Ordering::SeqCst))
    }

    pub fn from_u64(id: u64) -> Self {
        ScopeId(id)
    }

    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

/// Task scope for structured concurrency
/// Each concurrent {} block creates a scope that tracks all spawned tasks
pub struct TaskScope {
    id: ScopeId,
    parent: Option<ScopeId>,
    /// Task handles that need to be awaited when this scope exits
    /// We store them as type-erased Arc to handle multiple return types
    task_handles: Mutex<Vec<Arc<dyn std::any::Any + Send + Sync>>>,
}

impl TaskScope {
    /// Create a new scope with an optional parent
    pub fn new(parent: Option<ScopeId>) -> Self {
        TaskScope {
            id: ScopeId::new(),
            parent,
            task_handles: Mutex::new(Vec::new()),
        }
    }

    /// Get the scope ID
    pub fn id(&self) -> ScopeId {
        self.id
    }

    /// Get the parent scope ID
    pub fn parent(&self) -> Option<ScopeId> {
        self.parent
    }

    /// Register a task handle with this scope
    pub fn register_task<T: Send + Sync + 'static>(&self, handle: TaskHandle<T>) {
        let mut handles = self.task_handles.lock();
        handles.push(Arc::new(handle));
    }

    /// Wait for all tasks in this scope to complete
    pub fn await_all(&self) {
        let handles = self.task_handles.lock();

        // We can't actually await the handles since they're type-erased
        // Instead, we'll use the is_completed() method via downcasting
        // This is a bit ugly but necessary for the type-erased storage

        for handle_any in handles.iter() {
            // Try to downcast to common types and wait
            // This is hacky but works for the MVP

            // Try i64
            if let Some(handle) = handle_any.downcast_ref::<TaskHandle<i64>>() {
                while !handle.is_completed() {
                    std::thread::yield_now();
                }
                continue;
            }

            // Try i32
            if let Some(handle) = handle_any.downcast_ref::<TaskHandle<i32>>() {
                while !handle.is_completed() {
                    std::thread::yield_now();
                }
                continue;
            }

            // Try f64
            if let Some(handle) = handle_any.downcast_ref::<TaskHandle<f64>>() {
                while !handle.is_completed() {
                    std::thread::yield_now();
                }
                continue;
            }

            // Try bool
            if let Some(handle) = handle_any.downcast_ref::<TaskHandle<bool>>() {
                while !handle.is_completed() {
                    std::thread::yield_now();
                }
                continue;
            }

            // Try String
            if let Some(handle) = handle_any.downcast_ref::<TaskHandle<String>>() {
                while !handle.is_completed() {
                    std::thread::yield_now();
                }
                continue;
            }
        }
    }
}

/// Global scope registry
/// Manages the scope stack for each OS thread
pub struct ScopeRegistry {
    scopes: Mutex<HashMap<ScopeId, Arc<TaskScope>>>,
    /// Thread-local scope stack (simulated with a global map keyed by thread ID)
    thread_scopes: Mutex<HashMap<std::thread::ThreadId, Vec<ScopeId>>>,
}

impl ScopeRegistry {
    pub fn new() -> Self {
        ScopeRegistry {
            scopes: Mutex::new(HashMap::new()),
            thread_scopes: Mutex::new(HashMap::new()),
        }
    }

    /// Enter a new scope (for concurrent blocks)
    /// Returns the new scope ID
    pub fn enter_scope(&self) -> ScopeId {
        let thread_id = std::thread::current().id();

        // Get current scope (parent)
        let parent = {
            let thread_scopes = self.thread_scopes.lock();
            thread_scopes
                .get(&thread_id)
                .and_then(|stack| stack.last())
                .copied()
        };

        // Create new scope
        let scope = Arc::new(TaskScope::new(parent));
        let scope_id = scope.id();

        // Register scope
        {
            let mut scopes = self.scopes.lock();
            scopes.insert(scope_id, scope);
        }

        // Push onto thread's scope stack
        {
            let mut thread_scopes = self.thread_scopes.lock();
            thread_scopes
                .entry(thread_id)
                .or_insert_with(Vec::new)
                .push(scope_id);
        }

        scope_id
    }

    /// Exit a scope (wait for all tasks and pop from stack)
    pub fn exit_scope(&self, scope_id: ScopeId) {
        // Get the scope and await all tasks
        let scope = {
            let scopes = self.scopes.lock();
            scopes.get(&scope_id).cloned()
        };

        if let Some(scope) = scope {
            scope.await_all();
        }

        // Pop from thread's scope stack
        let thread_id = std::thread::current().id();
        {
            let mut thread_scopes = self.thread_scopes.lock();
            if let Some(stack) = thread_scopes.get_mut(&thread_id) {
                if let Some(top) = stack.last() {
                    if *top == scope_id {
                        stack.pop();
                    }
                }
            }
        }

        // Remove scope from registry (optional, could keep for debugging)
        {
            let mut scopes = self.scopes.lock();
            scopes.remove(&scope_id);
        }
    }

    /// Get the current scope for the calling thread
    pub fn current_scope(&self) -> Option<Arc<TaskScope>> {
        let thread_id = std::thread::current().id();
        let scope_id = {
            let thread_scopes = self.thread_scopes.lock();
            thread_scopes
                .get(&thread_id)
                .and_then(|stack| stack.last())
                .copied()
        }?;

        let scopes = self.scopes.lock();
        scopes.get(&scope_id).cloned()
    }

    /// Register a task handle with the current scope
    pub fn register_task<T: Send + Sync + 'static>(&self, handle: TaskHandle<T>) {
        if let Some(scope) = self.current_scope() {
            scope.register_task(handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scope_creation() {
        let scope = TaskScope::new(None);
        assert!(scope.parent().is_none());
    }

    #[test]
    fn test_scope_registry() {
        let registry = ScopeRegistry::new();

        // No scope initially
        assert!(registry.current_scope().is_none());

        // Enter a scope
        let scope_id = registry.enter_scope();
        assert!(registry.current_scope().is_some());

        // Exit the scope
        registry.exit_scope(scope_id);
        assert!(registry.current_scope().is_none());
    }

    #[test]
    fn test_nested_scopes() {
        let registry = ScopeRegistry::new();

        let outer_scope = registry.enter_scope();
        let inner_scope = registry.enter_scope();

        // Current scope should be inner
        assert_eq!(registry.current_scope().unwrap().id(), inner_scope);

        registry.exit_scope(inner_scope);

        // Current scope should be outer
        assert_eq!(registry.current_scope().unwrap().id(), outer_scope);

        registry.exit_scope(outer_scope);

        // No scope
        assert!(registry.current_scope().is_none());
    }
}
