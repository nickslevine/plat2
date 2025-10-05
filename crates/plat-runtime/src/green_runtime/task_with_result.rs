use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use std::sync::Arc;
use parking_lot::{Mutex, Condvar};

/// Global task ID counter (reuse from task.rs concept)
static NEXT_TASK_ID: AtomicU64 = AtomicU64::new(10000); // Start from 10000 to avoid conflicts

/// Unique identifier for a task
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TaskId(u64);

impl TaskId {
    fn new() -> Self {
        TaskId(NEXT_TASK_ID.fetch_add(1, Ordering::SeqCst))
    }

    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

/// Task state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    Ready,
    Running,
    Completed,
    Cancelled,
}

/// A green thread task that returns a value
pub struct TaskWithResult<T: Send + 'static> {
    id: TaskId,
    state: Arc<Mutex<TaskState>>,
    closure: Option<Box<dyn FnOnce() -> T + Send + 'static>>,
    result: Arc<Mutex<Option<T>>>,
    completed: Arc<AtomicBool>,
    condvar: Arc<Condvar>,
}

impl<T: Send + 'static> TaskWithResult<T> {
    /// Create a new task from a closure that returns a value
    pub fn new<F>(closure: F) -> Self
    where
        F: FnOnce() -> T + Send + 'static,
    {
        TaskWithResult {
            id: TaskId::new(),
            state: Arc::new(Mutex::new(TaskState::Ready)),
            closure: Some(Box::new(closure)),
            result: Arc::new(Mutex::new(None)),
            completed: Arc::new(AtomicBool::new(false)),
            condvar: Arc::new(Condvar::new()),
        }
    }

    /// Get the task ID
    pub fn id(&self) -> TaskId {
        self.id
    }

    /// Get the current state
    pub fn state(&self) -> TaskState {
        *self.state.lock()
    }

    /// Set the state
    fn set_state(&self, new_state: TaskState) {
        *self.state.lock() = new_state;
    }

    /// Check if the task is completed
    pub fn is_completed(&self) -> bool {
        self.completed.load(Ordering::SeqCst)
    }

    /// Get a handle to this task
    pub fn handle(&self) -> TaskHandle<T> {
        TaskHandle {
            id: self.id,
            result: self.result.clone(),
            completed: self.completed.clone(),
            condvar: self.condvar.clone(),
            _phantom: std::marker::PhantomData,
        }
    }

    /// Execute the task
    pub fn execute(mut self) {
        // Update state to Running
        self.set_state(TaskState::Running);

        // Take the closure and execute it
        if let Some(closure) = self.closure.take() {
            let result = closure();
            *self.result.lock() = Some(result);
        }

        // Mark as completed
        self.set_state(TaskState::Completed);
        self.completed.store(true, Ordering::SeqCst);

        // Wake up all threads waiting on this task
        self.condvar.notify_all();
    }

    /// Cancel the task
    pub fn cancel(&self) {
        self.set_state(TaskState::Cancelled);
    }
}

/// A task handle that can be used to wait for task completion and get the result
#[derive(Clone)]
pub struct TaskHandle<T> {
    id: TaskId,
    result: Arc<Mutex<Option<T>>>,
    completed: Arc<AtomicBool>,
    condvar: Arc<Condvar>,
    _phantom: std::marker::PhantomData<T>,
}

impl<T: Send + 'static> TaskHandle<T> {
    /// Get the task ID
    pub fn id(&self) -> TaskId {
        self.id
    }

    /// Check if the task is completed
    pub fn is_completed(&self) -> bool {
        self.completed.load(Ordering::SeqCst)
    }

    /// Wait for the task to complete without retrieving the result
    pub fn wait(&self) {
        if self.is_completed() {
            return;
        }

        let mut result_guard = self.result.lock();
        while !self.is_completed() {
            self.condvar.wait(&mut result_guard);
        }
    }

    /// Wait for the task to complete and get the result
    pub fn await_result(&self) -> Option<T>
    where
        T: Clone,
    {
        // Wait for completion using condition variable
        let mut result_guard = self.result.lock();
        while !self.is_completed() {
            self.condvar.wait(&mut result_guard);
        }

        // Extract the result
        result_guard.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_with_result_creation() {
        let task = TaskWithResult::new(|| {
            42i32
        });

        assert_eq!(task.state(), TaskState::Ready);
        assert!(!task.is_completed());
    }

    #[test]
    fn test_task_with_result_execution() {
        let task = TaskWithResult::new(|| {
            100i32
        });

        let handle = task.handle();
        task.execute();

        assert_eq!(handle.await_result(), Some(100i32));
    }

    #[test]
    fn test_task_handle_await() {
        let task = TaskWithResult::new(|| {
            std::thread::sleep(std::time::Duration::from_millis(10));
            "Hello from task!".to_string()
        });

        let handle = task.handle();

        // Spawn in another thread
        std::thread::spawn(move || {
            task.execute();
        });

        let result = handle.await_result();
        assert_eq!(result, Some("Hello from task!".to_string()));
    }
}
