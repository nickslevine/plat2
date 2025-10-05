use plat_runtime::green_runtime::{runtime_init, runtime_shutdown, GreenThreadRuntime, task::Task};
use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};
use std::thread;
use std::time::Duration;

#[test]
fn test_spawn_100_tasks() {
    // Initialize the runtime
    runtime_init();

    // Create a counter that will be incremented by each task
    let counter = Arc::new(AtomicI32::new(0));

    // Spawn 100 tasks
    for _ in 0..100 {
        let counter_clone = counter.clone();
        let task = Task::new(move || {
            counter_clone.fetch_add(1, Ordering::SeqCst);
        });

        // Spawn the task on the runtime
        let runtime = GreenThreadRuntime::get();
        let mut guard = runtime.lock();
        if let Some(rt) = guard.as_mut() {
            rt.spawn(task);
        }
    }

    // Wait for all tasks to complete
    // We'll sleep for a bit to give them time to run
    thread::sleep(Duration::from_millis(500));

    // Verify all tasks completed
    assert_eq!(counter.load(Ordering::SeqCst), 100, "All 100 tasks should have completed");

    // Shutdown the runtime
    runtime_shutdown();
}

#[test]
fn test_task_execution_order() {
    // Initialize the runtime
    runtime_init();

    // Create a shared vector to track execution order
    let execution_order = Arc::new(parking_lot::Mutex::new(Vec::new()));

    // Spawn 10 tasks
    for i in 0..10 {
        let execution_order_clone = execution_order.clone();
        let task = Task::new(move || {
            execution_order_clone.lock().push(i);
        });

        let runtime = GreenThreadRuntime::get();
        let mut guard = runtime.lock();
        if let Some(rt) = guard.as_mut() {
            rt.spawn(task);
        }
    }

    // Wait for all tasks to complete
    thread::sleep(Duration::from_millis(200));

    // Verify all tasks executed
    let order = execution_order.lock();
    assert_eq!(order.len(), 10, "All 10 tasks should have executed");

    // Shutdown the runtime
    runtime_shutdown();
}

#[test]
fn test_concurrent_task_spawning() {
    // Initialize the runtime
    runtime_init();

    let counter = Arc::new(AtomicI32::new(0));

    // Spawn tasks from multiple threads
    let handles: Vec<_> = (0..4).map(|_| {
        let counter_clone = counter.clone();
        thread::spawn(move || {
            for _ in 0..25 {
                let counter_task = counter_clone.clone();
                let task = Task::new(move || {
                    counter_task.fetch_add(1, Ordering::SeqCst);
                });

                let runtime = GreenThreadRuntime::get();
                let mut guard = runtime.lock();
                if let Some(rt) = guard.as_mut() {
                    rt.spawn(task);
                }
            }
        })
    }).collect();

    // Wait for all spawning threads to complete
    for handle in handles {
        handle.join().unwrap();
    }

    // Wait for all tasks to execute
    thread::sleep(Duration::from_millis(500));

    // Verify all 100 tasks (4 threads × 25 tasks) completed
    assert_eq!(counter.load(Ordering::SeqCst), 100, "All 100 tasks should have completed");

    // Shutdown the runtime
    runtime_shutdown();
}
