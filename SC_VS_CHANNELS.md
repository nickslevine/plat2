# Structured Concurrency vs Channels: Decision Guide

## When You DON'T Need Channels

### Pattern 1: **Parallel Computation (Fan-out/Gather)**
```plat
// You know what work to do upfront, just want results faster
fn main() -> Int32 {
  let results: List<Int32> = concurrent {
    let t1: Task<Int32> = spawn { expensive_computation(x = 1) };
    let t2: Task<Int32> = spawn { expensive_computation(x = 2) };
    let t3: Task<Int32> = spawn { expensive_computation(x = 3) };

    return [t1.await(), t2.await(), t3.await()];
  };

  return results[0] + results[1] + results[2];
}
```

**Why no channels?** You spawn a fixed number of tasks, wait for all results. Direct returns via `Task<T>` are simpler and more efficient.

### Pattern 2: **Racing for First Result**
```plat
fn check_server(url: String) -> Bool {
  // Returns true if server responds
  return tcp_connect(host = url, port = 80).is_ok();
}

fn main() -> Int32 {
  // Race multiple servers, use whoever responds first
  let fastest: String = race {
    spawn {
      if (check_server(url = "server1.com")) { return "server1"; }
      return "none";
    },
    spawn {
      if (check_server(url = "server2.com")) { return "server2"; }
      return "none";
    }
  };

  print(value = "Fastest: ${fastest}");
  return 0;
}
```

**Why no channels?** You only care about the first result. Race semantics handle this perfectly.

### Pattern 3: **Fire-and-Forget**
```plat
fn log_to_file(msg: String) {
  // Write to disk, don't care about result
}

fn main() -> Int32 {
  concurrent {
    spawn { log_to_file(msg = "Event 1"); };
    spawn { log_to_file(msg = "Event 2"); };
  }; // Wait for both to finish, but don't need return values

  return 0;
}
```

**Why no channels?** No communication needed, just parallel side effects.

---

## When You NEED Channels

### Pattern 1: **Producer-Consumer (Unknown Count)**
```plat
fn crawl_website(ch: Channel<String>) {
  // Don't know how many pages we'll find!
  let pages: List<String> = discover_pages();

  for (page: String in pages) {
    ch.send(value = page);
  }

  ch.close(); // Signal we're done
}

fn process_pages(ch: Channel<String>) -> Int32 {
  var count: Int32 = 0;

  // Keep receiving until channel closes
  loop {
    let maybe_page: Option<String> = ch.recv();

    match maybe_page {
      Option::Some(page: String) -> {
        process(page = page);
        count = count + 1;
      },
      Option::None -> break // Channel closed
    };
  }

  return count;
}

fn main() -> Int32 {
  let ch: Channel<String> = Channel.init();

  concurrent {
    spawn { crawl_website(ch = ch); };
    spawn { process_pages(ch = ch); };
  };

  return 0;
}
```

**Why channels?**
- Producer doesn't know count upfront
- Consumer processes as items arrive (streaming)
- Decouples producer/consumer timing

**Without channels:** You'd have to collect everything into a List first, then process. Memory spike! No pipelining!

### Pattern 2: **Pipeline (Multi-Stage Processing)**
```plat
fn stage1_fetch(out: Channel<String>) {
  for (url: String in urls) {
    let data: String = fetch(url = url);
    out.send(value = data);
  }
  out.close();
}

fn stage2_parse(input: Channel<String>, out: Channel<Document>) {
  loop {
    match input.recv() {
      Option::Some(html: String) -> {
        let doc: Document = parse_html(html = html);
        out.send(value = doc);
      },
      Option::None -> {
        out.close();
        break;
      }
    };
  }
}

fn stage3_save(input: Channel<Document>) {
  loop {
    match input.recv() {
      Option::Some(doc: Document) -> save_to_db(doc = doc),
      Option::None -> break
    };
  }
}

fn main() -> Int32 {
  let ch1: Channel<String> = Channel.init(capacity = 10);
  let ch2: Channel<Document> = Channel.init(capacity = 10);

  concurrent {
    spawn { stage1_fetch(out = ch1); };
    spawn { stage2_parse(input = ch1, out = ch2); };
    spawn { stage3_save(input = ch2); };
  };

  return 0;
}
```

**Why channels?**
- Each stage runs concurrently (pipelining!)
- Bounded channels provide **backpressure** (if parser is slow, fetcher waits)
- Natural flow of data through stages

**Without channels:** You'd have to batch everything, losing pipeline parallelism and memory efficiency.

### Pattern 3: **Work Queue (N Workers, M Tasks)**
```plat
fn worker(id: Int32, jobs: Channel<Job>, results: Channel<Result>) {
  loop {
    match jobs.recv() {
      Option::Some(job: Job) -> {
        let result: Result = process_job(job = job);
        results.send(value = result);
      },
      Option::None -> break // No more jobs
    };
  }
}

fn main() -> Int32 {
  let jobs: Channel<Job> = Channel.init(capacity = 100);
  let results: Channel<Result> = Channel.init(capacity = 100);

  concurrent {
    // Spawn 10 workers
    for (i: Int32 in 0..10) {
      spawn { worker(id = i, jobs = jobs, results = results); };
    }

    // Producer: send 1000 jobs
    spawn {
      for (i: Int32 in 0..1000) {
        jobs.send(value = create_job(id = i));
      }
      jobs.close();
    };

    // Consumer: collect results
    spawn {
      for (i: Int32 in 0..1000) {
        let result: Result = results.recv().unwrap();
        print(value = "Result: ${result}");
      }
    };
  };

  return 0;
}
```

**Why channels?**
- Dynamic load balancing (workers pull jobs as they finish)
- N:M relationship (many workers, many jobs)
- Natural queue abstraction

**Without channels:** You'd have to manually partition jobs across workers upfront. If one worker gets slow jobs, others sit idle!

### Pattern 4: **Event Streams**
```plat
fn network_listener(events: Channel<Event>) {
  loop {
    let event: Event = wait_for_network_event();
    events.send(value = event);
  }
}

fn event_processor(events: Channel<Event>) {
  loop {
    match events.recv() {
      Option::Some(event: Event) -> handle_event(event = event),
      Option::None -> break
    };
  }
}

fn main() -> Int32 {
  let events: Channel<Event> = Channel.init(capacity = 100);

  concurrent {
    spawn { network_listener(events = events); };
    spawn { event_processor(events = events); };
  };

  return 0;
}
```

**Why channels?**
- Infinite stream (no end count)
- Buffering smooths bursts
- Decouples event generation from processing

---

## The Decision Tree

```
Do you know how many tasks upfront?
├─ YES
│  └─ Do you need all results?
│     ├─ YES → Use Task.await() on each
│     ├─ NO (just side effects) → Use spawn without await
│     └─ NO (just first) → Use race
│
└─ NO (streaming/unknown count)
   └─ Use Channels

Is there a pipeline (A → B → C)?
├─ YES → Use Channels
└─ NO → Use Task.await()

Do you need backpressure?
├─ YES → Use bounded Channels
└─ NO → Use Task.await() or unbounded Channels

Is there an N:M relationship (workers:jobs)?
├─ YES → Use Channels (work queue)
└─ NO → Use Task.await() or race
```

---

## Key Insight

**Channels are for STREAMS, Tasks are for RESULTS**

- **Task<T>**: "I spawned work, I want the answer" (request/response)
- **Channel<T>**: "I'm producing data over time, consume as you can" (stream)

### Channels Give You:
1. **Buffering** - Producer can run ahead of consumer
2. **Backpressure** - Bounded channels make fast producers wait
3. **Decoupling** - Lifetime independence (producer finishes before consumer)
4. **Fan-in/Fan-out** - Multiple producers → one channel → multiple consumers
5. **Select** - Wait on multiple channels at once (advanced)

### Tasks Give You:
1. **Simplicity** - Just spawn and await
2. **Type safety** - Each task returns specific type
3. **Structured cleanup** - Tasks can't outlive scope
4. **Direct results** - No queue overhead

---

## Concrete Example: When to Switch

### Start with Tasks:
```plat
// Simple parallel fetch
fn main() -> Int32 {
  concurrent {
    let t1: Task<String> = spawn { fetch(url = "url1") };
    let t2: Task<String> = spawn { fetch(url = "url2") };
    let t3: Task<String> = spawn { fetch(url = "url3") };

    process(data = t1.await());
    process(data = t2.await());
    process(data = t3.await());
  };

  return 0;
}
```

### Need Channels When:
```plat
// URL list is dynamic, process as we fetch
fn main() -> Int32 {
  let urls: List<String> = load_urls_from_file(); // Could be 10 or 10,000!
  let fetched: Channel<String> = Channel.init(capacity = 10);

  concurrent {
    // Producer
    spawn {
      for (url: String in urls) {
        let data: String = fetch(url = url);
        fetched.send(value = data);
      }
      fetched.close();
    };

    // Consumer (can start processing before all are fetched!)
    spawn {
      loop {
        match fetched.recv() {
          Option::Some(data: String) -> process(data = data),
          Option::None -> break
        };
      }
    };
  };

  return 0;
}
```

---

## Summary Table

| Pattern | Use Case | Tool |
|---------|----------|------|
| Fixed parallel work | 10 known tasks | `Task.await()` |
| Race to first | First of N to respond | `race { }` |
| Streaming data | Unknown count, process as you go | `Channel<T>` |
| Pipeline | Stage 1 → Stage 2 → Stage 3 | Multiple `Channel<T>` |
| Work queue | N workers, M jobs | `Channel<Job>` |
| Backpressure | Slow consumer needs to slow producer | Bounded `Channel<T>` |

**The pattern:** Start with `Task<T>` for 80% of cases. Add `Channel<T>` when you need streaming, unknown counts, or pipelines.
