pub mod task;
pub mod task_with_result;
pub mod scheduler;
pub mod scope;

use std::sync::Arc;
use parking_lot::Mutex;
use lazy_static::lazy_static;
use crossbeam_deque::Worker;

use task::{Task, TaskId};
use scheduler::Scheduler;
use scope::ScopeRegistry;

lazy_static! {
    static ref RUNTIME: Arc<Mutex<Option<GreenThreadRuntime>>> = Arc::new(Mutex::new(None));
    static ref SCOPE_REGISTRY: Arc<ScopeRegistry> = Arc::new(ScopeRegistry::new());
}

/// Get the global scope registry
pub fn get_scope_registry() -> Arc<ScopeRegistry> {
    SCOPE_REGISTRY.clone()
}

/// Green thread runtime with M:N threading
pub struct GreenThreadRuntime {
    scheduler: Scheduler,
    num_workers: usize,
    workers: Vec<Worker<Task>>,
    worker_handles: Vec<std::thread::JoinHandle<()>>,
}

impl GreenThreadRuntime {
    /// Create a new runtime with the specified number of worker threads
    pub fn new(num_workers: usize) -> Self {
        let num_workers = if num_workers == 0 {
            num_cpus::get()
        } else {
            num_workers
        };

        let (scheduler, workers) = Scheduler::new(num_workers);
        let worker_handles = Vec::new();

        GreenThreadRuntime {
            scheduler,
            num_workers,
            workers,
            worker_handles,
        }
    }

    /// Initialize the global runtime
    pub fn init() {
        let mut runtime_guard = RUNTIME.lock();
        if runtime_guard.is_none() {
            let runtime = Self::new(0); // Use default number of workers
            *runtime_guard = Some(runtime);
        }
    }

    /// Get a reference to the global runtime
    pub fn get() -> Arc<Mutex<Option<GreenThreadRuntime>>> {
        RUNTIME.clone()
    }

    /// Start worker threads
    pub fn start_workers(&mut self) {
        // Take the workers (we can only start once)
        let workers = std::mem::take(&mut self.workers);
        let scheduler = self.scheduler.clone();

        // Spawn worker threads using the workers that are connected to our scheduler
        for (worker_id, worker) in workers.into_iter().enumerate() {
            let scheduler_clone = scheduler.clone();

            let handle = std::thread::spawn(move || {
                Self::worker_loop(worker_id, worker, scheduler_clone);
            });

            self.worker_handles.push(handle);
        }
    }

    /// Worker thread main loop
    fn worker_loop(worker_id: usize, worker: Worker<Task>, scheduler: Scheduler) {
        loop {
            // Try to get a task from the scheduler
            match scheduler.pop_task(&worker, worker_id) {
                Some(task) => {
                    // Execute the task
                    task.execute();
                }
                None => {
                    // No tasks available, check if we should shutdown
                    if scheduler.is_shutdown() {
                        break;
                    }

                    // Park this thread until more work arrives
                    scheduler.park_worker(worker_id);
                }
            }
        }
    }

    /// Spawn a new task
    pub fn spawn(&mut self, task: Task) -> TaskId {
        let task_id = task.id();
        self.scheduler.push_task(task);
        task_id
    }

    /// Spawn a task with result by converting it to a regular Task
    pub fn spawn_with_result<T: Send + 'static>(&mut self, task_with_result: task_with_result::TaskWithResult<T>) {
        // Convert TaskWithResult to a regular Task by wrapping the execution
        let task = Task::new(move || {
            task_with_result.execute();
        });
        self.scheduler.push_task(task);
    }

    /// Shutdown the runtime
    pub fn shutdown(&mut self) {
        self.scheduler.signal_shutdown();

        // Wait for all worker threads to finish
        while let Some(handle) = self.worker_handles.pop() {
            let _ = handle.join();
        }
    }
}

impl Drop for GreenThreadRuntime {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Initialize the runtime (called from C FFI)
pub fn runtime_init() {
    GreenThreadRuntime::init();

    // Start worker threads
    let runtime = GreenThreadRuntime::get();
    let mut guard = runtime.lock();
    if let Some(rt) = guard.as_mut() {
        rt.start_workers();
    }
}

/// Shutdown the runtime (called from C FFI)
pub fn runtime_shutdown() {
    let runtime = GreenThreadRuntime::get();
    let mut guard = runtime.lock();
    if let Some(rt) = guard.take() {
        drop(rt); // This will trigger the Drop implementation
    }
}
