use crossbeam_deque::{Injector, Stealer, Worker};
use std::sync::Arc;
use parking_lot::{Mutex, Condvar};
use std::sync::atomic::{AtomicBool, Ordering};

use super::task::Task;

/// Work-stealing scheduler for green threads
#[derive(Clone)]
pub struct Scheduler {
    /// Global injector queue for new tasks
    injector: Arc<Injector<Task>>,

    /// Stealers for each worker (used by other workers to steal tasks)
    stealers: Arc<Vec<Stealer<Task>>>,

    /// Condition variables for parking workers
    parked: Arc<Vec<(Mutex<bool>, Condvar)>>,

    /// Shutdown flag
    shutdown: Arc<AtomicBool>,
}

impl Scheduler {
    /// Create a new scheduler with the specified number of workers
    /// Returns the scheduler and a vector of worker deques (one per worker thread)
    pub fn new(num_workers: usize) -> (Self, Vec<Worker<Task>>) {
        let injector = Arc::new(Injector::new());
        let mut workers = Vec::new();
        let mut stealers = Vec::new();
        let mut parked = Vec::new();

        // Create work-stealing deques for each worker
        for _ in 0..num_workers {
            let worker = Worker::new_fifo();
            stealers.push(worker.stealer());
            workers.push(worker);
            parked.push((Mutex::new(false), Condvar::new()));
        }

        let scheduler = Scheduler {
            injector,
            stealers: Arc::new(stealers),
            parked: Arc::new(parked),
            shutdown: Arc::new(AtomicBool::new(false)),
        };

        (scheduler, workers)
    }

    /// Push a task onto the global queue
    pub fn push_task(&self, task: Task) {
        self.injector.push(task);

        // Unpark a worker to handle the new task
        self.unpark_any_worker();
    }

    /// Pop a task from the worker's local queue, or steal from others
    pub fn pop_task(&self, worker: &Worker<Task>, worker_id: usize) -> Option<Task> {
        // First, try to pop from the local queue
        if let Some(task) = worker.pop() {
            return Some(task);
        }

        // If local queue is empty, try to steal from the global injector
        loop {
            if let crossbeam_deque::Steal::Success(task) = self.injector.steal() {
                return Some(task);
            }

            // Try to steal from other workers
            for (i, stealer) in self.stealers.iter().enumerate() {
                if i == worker_id {
                    continue; // Don't steal from ourselves
                }

                match stealer.steal() {
                    crossbeam_deque::Steal::Success(task) => return Some(task),
                    crossbeam_deque::Steal::Empty => continue,
                    crossbeam_deque::Steal::Retry => continue,
                }
            }

            // No tasks available anywhere
            break;
        }

        None
    }

    /// Park a worker thread (wait for more work)
    pub fn park_worker(&self, worker_id: usize) {
        let (lock, condvar) = &self.parked[worker_id];
        let mut parked = lock.lock();
        *parked = true;

        // Wait for notification or timeout
        let timeout = std::time::Duration::from_millis(100);
        let _ = condvar.wait_for(&mut parked, timeout);

        *parked = false;
    }

    /// Unpark any worker that might be sleeping
    fn unpark_any_worker(&self) {
        // Try to find a parked worker and wake it up
        for (lock, condvar) in self.parked.iter() {
            let parked = lock.lock();
            if *parked {
                condvar.notify_one();
                break;
            }
        }
    }

    /// Check if the scheduler is in shutdown mode
    pub fn is_shutdown(&self) -> bool {
        self.shutdown.load(Ordering::SeqCst)
    }

    /// Signal shutdown to all workers
    pub fn signal_shutdown(&self) {
        self.shutdown.store(true, Ordering::SeqCst);

        // Wake up all parked workers so they can exit
        for (lock, condvar) in self.parked.iter() {
            let _parked = lock.lock();
            condvar.notify_all();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicI32;

    #[test]
    fn test_scheduler_creation() {
        let (scheduler, _workers) = Scheduler::new(4);
        assert!(!scheduler.is_shutdown());
    }

    #[test]
    fn test_push_and_pop_task() {
        let (scheduler, mut workers) = Scheduler::new(4);
        let counter = Arc::new(AtomicI32::new(0));
        let counter_clone = counter.clone();

        let task = Task::new(move || {
            counter_clone.fetch_add(1, Ordering::SeqCst);
        });

        scheduler.push_task(task);

        // Pop the task from worker 0
        let task = scheduler.pop_task(&workers[0], 0);
        assert!(task.is_some());

        // Execute it
        if let Some(task) = task {
            task.execute();
        }

        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_work_stealing() {
        let (scheduler, mut workers) = Scheduler::new(2);

        // Push a task
        let counter = Arc::new(AtomicI32::new(0));
        let counter_clone = counter.clone();

        let task = Task::new(move || {
            counter_clone.fetch_add(1, Ordering::SeqCst);
        });

        scheduler.push_task(task);

        // Worker 1 should be able to steal from the global queue
        let task = scheduler.pop_task(&workers[1], 1);
        assert!(task.is_some());
    }
}
