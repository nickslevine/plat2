# Plat Concurrency Implementation Plan

**Goal:** Add structured concurrency with green threads, channels, and no function coloring.

**Design Principles:**
- No async/await (no colored functions)
- Structured concurrency (scoped task lifetimes)
- Result-based error handling
- Simple mental model
- Zero-cost abstractions where possible

---

## Architecture Overview

```
┌─────────────────────────────────────────┐
│         User Code (Plat)                │
│  concurrent { spawn { ... } }           │
└──────────────┬──────────────────────────┘
               │
┌──────────────▼──────────────────────────┐
│      Plat Runtime (plat-runtime)        │
│  - Task spawning                        │
│  - Task handles (Task<T>)               │
│  - Channel operations                   │
│  - Scheduler interface                  │
└──────────────┬──────────────────────────┘
               │
┌──────────────▼──────────────────────────┐
│    Green Thread Scheduler (Rust)        │
│  - M:N threading (green threads → OS)   │
│  - Work-stealing queue                  │
│  - Async I/O integration                │
│  - Cooperative yielding                 │
└─────────────────────────────────────────┘
```

---

## Phase 1: Green Thread Runtime Foundation

**Goal:** Build M:N threading runtime that executes closures on a thread pool.

### 1.1 Core Runtime Infrastructure

- [ ] **Create `plat-runtime/src/runtime/mod.rs`**
  - Runtime singleton with thread pool
  - Work-stealing scheduler (use crossbeam-deque)
  - Worker thread lifecycle management
  - Global runtime initialization

- [ ] **Create `plat-runtime/src/runtime/task.rs`**
  - Task struct (holds closure + metadata)
  - Task state machine (Ready, Running, Completed, Cancelled)
  - Task ID generation
  - Task local storage (thread-local for green threads)

- [ ] **Create `plat-runtime/src/runtime/scheduler.rs`**
  - Work-stealing deques (one per worker)
  - Task spawning (push to local queue)
  - Task stealing (pop from other queues when idle)
  - Park/unpark for idle workers

**Commit:** `feat: Add green thread runtime with work-stealing scheduler`

### 1.2 Basic Task Execution

- [ ] **Add `spawn_task()` C API in `plat-runtime/src/lib.rs`**
  - Accept function pointer + args
  - Wrap in Task
  - Push to scheduler
  - Return task ID

- [ ] **Add simple test in `plat-runtime/tests/`**
  - Spawn 100 tasks
  - Each increments atomic counter
  - Verify all completed

**Commit:** `feat: Add task spawning API with basic execution`

### 1.3 Integration with Existing Runtime

- [ ] **Update `plat-runtime/src/lib.rs`**
  - Initialize runtime on first use
  - Shutdown on program exit
  - Thread-safe global state

- [ ] **Add runtime lifecycle management**
  - `runtime_init()` called from main
  - `runtime_shutdown()` on exit
  - Graceful worker shutdown

**Commit:** `feat: Integrate green thread runtime with Plat lifecycle`

---

## Phase 2: Structured Concurrency

**Goal:** Add `concurrent {}` blocks, `spawn`, `Task<T>`, and `.await()`.

### 2.1 Language Support

- [ ] **Add `concurrent` keyword to lexer** (`plat-lexer`)
  - Add token variant `Token::Concurrent`

- [ ] **Add `spawn` keyword to lexer**
  - Add token variant `Token::Spawn`

- [ ] **Parse `concurrent { ... }` blocks** (`plat-parser`)
  - New AST node: `Stmt::ConcurrentBlock { body: Vec<Stmt> }`
  - Parse block contents normally

- [ ] **Parse `spawn { ... }` expressions** (`plat-parser`)
  - New AST node: `Expr::Spawn { body: Box<Expr> }`
  - Must be inside `concurrent` block (validation in HIR)

**Commit:** `feat: Add concurrent and spawn keywords to parser`

### 2.2 HIR Representation

- [ ] **Add HIR nodes** (`plat-hir`)
  - `HIRStmt::ConcurrentBlock { scope_id, body }`
  - `HIRExpr::Spawn { task_id, body, return_type }`
  - `HIRExpr::Await { task_expr }`

- [ ] **Type checking for concurrent blocks**
  - Track scope nesting (concurrent blocks create new scopes)
  - Validate `spawn` only inside `concurrent`
  - Infer return type of spawned closure

- [ ] **Type checking for Task<T>**
  - Add built-in generic type `Task<T>`
  - `spawn { expr }` returns `Task<T>` where T is expr's type
  - `.await()` method on Task<T> returns T

**Commit:** `feat: Add HIR support for concurrent blocks and task spawning`

### 2.3 Codegen for Spawn

- [ ] **Generate code for `concurrent` blocks** (`plat-codegen`)
  - Create scope tracker (TaskScope)
  - Allocate scope ID
  - Generate body code
  - Insert scope cleanup (await all tasks in scope)

- [ ] **Generate code for `spawn`**
  - Extract closure body into separate function
  - Capture variables (pass as args)
  - Call runtime `spawn_task(fn_ptr, args, scope_id)`
  - Return opaque Task<T> handle

- [ ] **Generate code for `.await()`**
  - Call runtime `task_await(task_id)` → blocks until complete
  - Return result value

**Commit:** `feat: Add codegen for concurrent blocks and task spawning`

### 2.4 Runtime Support for Task<T>

- [ ] **Add `Task<T>` representation in runtime**
  - Store return value in task struct
  - Use type-erased pointer (void*) for now
  - Add completion flag (atomic bool)

- [ ] **Implement `task_await()` C API**
  - Busy-wait or condition variable
  - Return result when task completes
  - Handle panics (propagate to caller)

- [ ] **Implement scope tracking**
  - Each scope has list of child task IDs
  - On scope exit, await all children
  - Ensures no tasks outlive scope

**Commit:** `feat: Add runtime support for Task<T> and structured scopes`

### 2.5 Testing

- [ ] **Write Plat tests** (`examples/test_concurrent.plat`)
  - Simple spawn + await
  - Multiple tasks in same scope
  - Nested concurrent blocks
  - Return values from tasks

- [ ] **Add to test suite**
  - Verify no leaks (all tasks complete)
  - Test error propagation (panic in task)

**Commit:** `test: Add tests for basic structured concurrency`

---

## Phase 3: Channels

**Goal:** Add `Channel<T>` for producer-consumer patterns.

### 3.1 Language Support

- [ ] **Add Channel<T> as built-in generic type** (`plat-hir`)
  - Similar to List<T>, Dict<K,V>
  - Methods: `init()`, `send()`, `recv()`, `close()`

- [ ] **Type checking for Channel<T>**
  - `Channel.init(capacity = n)` → Channel<T>
  - `send(value = x)` where x: T → unit
  - `recv()` → Option<T>
  - `close()` → unit

**Commit:** `feat: Add Channel<T> type to language`

### 3.2 Runtime Implementation

- [ ] **Create `plat-runtime/src/channel.rs`**
  - Use crossbeam-channel internally
  - Bounded vs unbounded variants
  - Thread-safe send/recv
  - Close semantics (recv returns None after close)

- [ ] **Add C API for channels**
  - `channel_new(capacity)` → channel_id
  - `channel_send(channel_id, value_ptr)`
  - `channel_recv(channel_id)` → value_ptr or NULL
  - `channel_close(channel_id)`

**Commit:** `feat: Add channel runtime implementation`

### 3.3 Codegen for Channels

- [ ] **Generate channel operations** (`plat-codegen`)
  - `Channel.init(capacity = n)` → call `channel_new(n)`
  - `ch.send(value = x)` → call `channel_send(ch_id, &x)`
  - `ch.recv()` → call `channel_recv(ch_id)`, wrap in Option<T>
  - `ch.close()` → call `channel_close(ch_id)`

- [ ] **Handle Option<T> wrapping**
  - `recv()` returns raw pointer
  - NULL → Option::None
  - Non-NULL → Option::Some(value)

**Commit:** `feat: Add codegen for channel operations`

### 3.4 Testing

- [ ] **Write Plat tests** (`examples/test_channels.plat`)
  - Producer-consumer pattern
  - Multiple producers, one consumer
  - Channel close semantics
  - Bounded channel backpressure

**Commit:** `test: Add channel tests`

---

## Phase 4: Advanced Features

**Goal:** Add `race`, `select`, timeouts, and other conveniences.

### 4.1 Race Combinator

- [ ] **Add `race { ... }` syntax**
  - Similar to `concurrent`, but returns first result
  - Cancel remaining tasks when first completes

- [ ] **Runtime support for cancellation**
  - Add cancellation token to tasks
  - Check token periodically in yielding code
  - Propagate cancellation to child tasks

**Commit:** `feat: Add race combinator for task racing`

### 4.2 Select for Channels

- [ ] **Add `select!` macro/syntax**
  - Wait on multiple channel recv operations
  - Return first available
  - Use crossbeam-channel's select internally

**Commit:** `feat: Add select for multiple channel operations`

### 4.3 Timeouts

- [ ] **Add `recv_timeout(duration)` to channels**
  - Returns Option<T> or timeout error
  - Use Result<Option<T>, TimeoutError>?

- [ ] **Add `with_timeout` for tasks**
  - `with_timeout(duration = 1000ms, body = { ... })`
  - Cancel task if exceeds duration

**Commit:** `feat: Add timeout support for channels and tasks`

### 4.4 Async I/O Integration

- [ ] **Integrate tokio/async-std for I/O**
  - Wrap blocking I/O (tcp_read, tcp_write) with async
  - Runtime spawns tasks on async executor
  - Automatic yielding on I/O operations

- [ ] **Update tcp_* functions**
  - Make them yield automatically when blocking
  - No API changes (still look synchronous!)

**Commit:** `feat: Integrate async I/O for automatic yielding`

---

## Phase 5: Optimization & Polish

### 5.1 Performance

- [ ] **Benchmark suite** (`examples/bench_concurrent.plat`)
  - Task spawning overhead
  - Channel throughput
  - Work-stealing efficiency

- [ ] **Optimize hot paths**
  - Fast path for local task queue
  - Reduce atomic operations
  - Cache-friendly data structures

**Commit:** `perf: Optimize runtime hot paths`

### 5.2 Error Messages

- [ ] **Better diagnostics**
  - Error when `spawn` outside `concurrent`
  - Suggest `concurrent` block when needed
  - Type errors for Task<T> mismatches

**Commit:** `feat: Improve concurrency error messages`

### 5.3 Documentation

- [ ] **Update CLAUDE.md**
  - Add concurrency section
  - Examples for common patterns
  - Link to SC_VS_CHANNELS.md

- [ ] **Add examples**
  - `examples/concurrent_web_crawler.plat`
  - `examples/pipeline_processing.plat`
  - `examples/work_queue.plat`

**Commit:** `docs: Add concurrency documentation and examples`

---

## Implementation Details

### Task Storage & Return Values

**Problem:** Tasks return arbitrary types, but runtime is in Rust/C.

**Solution 1: Type-erased heap allocation**
```rust
struct Task {
  id: TaskId,
  closure: Box<dyn FnOnce() -> ()>,
  result: Option<Box<dyn Any>>, // Type-erased result
  completed: AtomicBool,
}
```

**Solution 2: Fixed-size buffer with large enough capacity**
```rust
const MAX_TASK_RESULT_SIZE: usize = 128; // bytes

struct Task {
  id: TaskId,
  closure: Box<dyn FnOnce() -> ()>,
  result_buffer: [u8; MAX_TASK_RESULT_SIZE],
  result_size: usize,
  completed: AtomicBool,
}
```

**Recommendation:** Use Solution 1 (heap allocation) for simplicity. Optimize later if needed.

### Scope Tracking

**Each `concurrent` block creates a TaskScope:**
```rust
struct TaskScope {
  id: ScopeId,
  parent: Option<ScopeId>,
  children: Vec<TaskId>,
}

// On scope entry:
let scope_id = runtime.enter_scope();

// On spawn:
runtime.spawn_in_scope(scope_id, closure);

// On scope exit:
runtime.exit_scope(scope_id); // Awaits all children
```

### Channel Implementation

**Use crossbeam-channel internally:**
```rust
pub struct Channel<T> {
  id: ChannelId,
  sender: Sender<T>,
  receiver: Receiver<T>,
}

// C API:
#[no_mangle]
pub extern "C" fn channel_new(capacity: i32) -> u64 {
  let (tx, rx) = if capacity > 0 {
    crossbeam_channel::bounded(capacity as usize)
  } else {
    crossbeam_channel::unbounded()
  };

  let id = CHANNEL_REGISTRY.insert(tx, rx);
  id
}
```

### Automatic Yielding

**Cooperative scheduling requires yield points:**

**Option 1: Manual yields**
- Insert `yield_now()` calls in long-running code
- User responsibility (easy to forget!)

**Option 2: Preemptive yielding**
- Use tokio's yield_now() automatically
- Wrap I/O operations with async
- Transparent to user

**Recommendation:** Start with Option 2 (automatic). Add manual yields later if needed.

---

## Testing Strategy

### Unit Tests (Rust)

- [ ] Task spawning and execution
- [ ] Work-stealing behavior
- [ ] Channel send/recv correctness
- [ ] Scope cleanup

### Integration Tests (Plat)

- [ ] Concurrent blocks with multiple tasks
- [ ] Task return values and awaiting
- [ ] Channel producer-consumer
- [ ] Nested concurrent blocks
- [ ] Error propagation

### Stress Tests

- [ ] Spawn 10,000 tasks simultaneously
- [ ] High-throughput channel passing
- [ ] Deep nesting of concurrent blocks

---

## Dependencies to Add

```toml
# plat-runtime/Cargo.toml

[dependencies]
crossbeam-deque = "0.8"      # Work-stealing queues
crossbeam-channel = "0.5"    # Channels
parking_lot = "0.12"         # Better mutexes
tokio = { version = "1", features = ["rt-multi-thread", "net", "io-util"] }  # Async I/O
```

---

## Rollout Plan

**Phase 1 (Week 1-2): Foundation**
- Green thread runtime
- Basic task spawning
- Work-stealing scheduler

**Phase 2 (Week 3-4): Structured Concurrency**
- concurrent blocks
- spawn + Task<T>
- .await()
- Scope tracking

**Phase 3 (Week 5-6): Channels**
- Channel<T> type
- send/recv/close
- Bounded/unbounded

**Phase 4 (Week 7-8): Advanced**
- race combinator
- select for channels
- Timeouts
- Async I/O integration

**Phase 5 (Week 9-10): Polish**
- Performance optimization
- Error messages
- Documentation
- Example programs

---

## Success Metrics

- [ ] Can run parallel computations faster than sequential
- [ ] Zero task leaks (all tasks complete when scope exits)
- [ ] Channels support streaming data without memory spikes
- [ ] No function coloring (any function can spawn tasks)
- [ ] Error messages are clear and helpful
- [ ] Performance competitive with Go/Tokio for I/O-bound workloads

---

## Open Questions

1. **Panic handling:** Should panics in tasks propagate to parent? Or return Result?
   - **Proposal:** Panics propagate to parent scope (structured concurrency principle)

2. **Task priorities:** Do we need priority scheduling?
   - **Proposal:** No, keep simple. Add later if users request.

3. **Work stealing vs FIFO:** Work stealing for CPU-bound, FIFO for latency-sensitive?
   - **Proposal:** Start with work-stealing only. Add FIFO option later.

4. **Channel buffering strategy:** Always bounded, or allow unbounded?
   - **Proposal:** Support both. Bounded is default, unbounded for simple cases.

5. **Async function transforms:** Do we need to transform all I/O functions?
   - **Proposal:** Yes, but do it automatically in runtime, not user code.

---

## Future Extensions (Post-MVP)

- [ ] Task-local storage (TLS for green threads)
- [ ] Async file I/O
- [ ] UDP support
- [ ] Parallel iterators (`for item in list.par() { ... }`)
- [ ] Work-stealing async runtime (no tokio dependency)
- [ ] SIMD parallelism
- [ ] GPU compute integration

---

## References & Inspiration

- **Go:** Goroutines + channels (CSP model)
- **Swift:** Structured concurrency (async/await with task trees)
- **Kotlin:** Coroutines with structured concurrency
- **Trio (Python):** Nurseries for scope-based task management
- **Pony:** Reference capabilities for data race prevention
- **Rust:** Tokio runtime, crossbeam channels

---

**Next Steps:**
1. Review this plan
2. Start with Phase 1.1 (runtime infrastructure)
3. Commit frequently, test everything
4. Update checkboxes as we progress
