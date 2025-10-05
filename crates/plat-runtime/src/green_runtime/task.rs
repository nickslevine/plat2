use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use std::sync::Arc;
use parking_lot::Mutex;

/// Global task ID counter
static NEXT_TASK_ID: AtomicU64 = AtomicU64::new(1);

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

/// A green thread task
pub struct Task {
    id: TaskId,
    state: Arc<Mutex<TaskState>>,
    closure: Option<Box<dyn FnOnce() + Send + 'static>>,
    result: Arc<Mutex<Option<Box<dyn std::any::Any + Send>>>>,
    completed: Arc<AtomicBool>,
}

impl Task {
    /// Create a new task from a closure
    pub fn new<F>(closure: F) -> Self
    where
        F: FnOnce() + Send + 'static,
    {
        Task {
            id: TaskId::new(),
            state: Arc::new(Mutex::new(TaskState::Ready)),
            closure: Some(Box::new(closure)),
            result: Arc::new(Mutex::new(None)),
            completed: Arc::new(AtomicBool::new(false)),
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

    /// Execute the task
    pub fn execute(mut self) {
        // Update state to Running
        self.set_state(TaskState::Running);

        // Take the closure and execute it
        if let Some(closure) = self.closure.take() {
            closure();
        }

        // Mark as completed
        self.set_state(TaskState::Completed);
        self.completed.store(true, Ordering::SeqCst);
    }

    /// Cancel the task
    pub fn cancel(&self) {
        self.set_state(TaskState::Cancelled);
    }
}

/// A task handle that can be used to wait for task completion
pub struct TaskHandle<T> {
    id: TaskId,
    result: Arc<Mutex<Option<Box<dyn std::any::Any + Send>>>>,
    completed: Arc<AtomicBool>,
    _phantom: std::marker::PhantomData<T>,
}

impl<T: Send + 'static> TaskHandle<T> {
    /// Create a new task handle
    pub fn new(id: TaskId, result: Arc<Mutex<Option<Box<dyn std::any::Any + Send>>>>, completed: Arc<AtomicBool>) -> Self {
        TaskHandle {
            id,
            result,
            completed,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Get the task ID
    pub fn id(&self) -> TaskId {
        self.id
    }

    /// Check if the task is completed
    pub fn is_completed(&self) -> bool {
        self.completed.load(Ordering::SeqCst)
    }

    /// Wait for the task to complete and get the result
    pub fn await_result(&self) -> Option<T> {
        // Busy-wait for completion (TODO: use condition variable)
        while !self.is_completed() {
            std::thread::yield_now();
        }

        // Extract the result
        let mut result_guard = self.result.lock();
        if let Some(boxed_result) = result_guard.take() {
            // Downcast to the expected type
            boxed_result.downcast::<T>().ok().map(|b| *b)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_id_generation() {
        let id1 = TaskId::new();
        let id2 = TaskId::new();
        assert_ne!(id1, id2);
        assert!(id2.as_u64() > id1.as_u64());
    }

    #[test]
    fn test_task_creation() {
        let task = Task::new(|| {
            println!("Hello from task!");
        });

        assert_eq!(task.state(), TaskState::Ready);
        assert!(!task.is_completed());
    }

    #[test]
    fn test_task_execution() {
        use std::sync::atomic::AtomicI32;
        let counter = Arc::new(AtomicI32::new(0));
        let counter_clone = counter.clone();

        let task = Task::new(move || {
            counter_clone.fetch_add(1, Ordering::SeqCst);
        });

        task.execute();

        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }
}
