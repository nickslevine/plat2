# 📜 Plat Language Compiler

**A modern compiled language with:**
- Native code generation (Rust + Cranelift)
- Strong static typing with explicit type annotations (no inference)
- Object-oriented programming (classes, inheritance, polymorphism)
- Algebraic data types (enums, pattern matching)
- Generic types and functions
- Module system with dependency resolution
- GC-managed memory

---

## 🎯 Core Language Features

### Type System
- **Primitives**: `Bool`, `Int8`, `Int16`, `Int32`, `Int64`, `Float8`, `Float16`, `Float32`, `Float64`, `String`
- **Type Aliases (Built-in)**: `Int` (alias for `Int64`), `Float` (alias for `Float64`)
- **Collections**: `List[T]`, `Dict[K, V]`, `Set[T]`
- **Built-in Enums**: `Option<T>`, `Result<T, E>`
- **Custom Type Aliases**: `type UserID = String;` (interchangeable with underlying type)
- **Newtypes**: `newtype DocumentID = String;` (distinct type at compile-time, same runtime representation)
- **Numeric Literals**: Support underscores for readability (e.g., `1_000_000`, `3.141_592_653`)

### Naming Conventions (Enforced at Compile-Time)
- **snake_case**: Variables, functions, parameters, module names, field names
- **TitleCase**: Types, classes, enums, enum variants, type aliases, newtypes, type parameters

### Object-Oriented Programming
- **Classes**: Field declarations with `let`/`var` mutability
- **Default Constructors**: Classes without explicit `init` get auto-generated constructors
- **Inheritance**: `class Dog : Animal` with virtual methods
- **Polymorphism**: Safe upcasting, vtable-based dynamic dispatch
- **Generics**: `class Container<T>`, `fn identity<T>(value: T) -> T`

### Pattern Matching
- **Enums**: Unit variants, data variants, multi-field variants
- **Match Expressions**: Exhaustiveness checking, pattern binding
- **Example**: `match status { Status::Success -> 1, Status::Error(code) -> code }`

### Function Calls
- **Named Arguments Required**: All function, method, and constructor calls must use explicit named arguments
- **Format**: `function_name(param1 = value1, param2 = value2)`
- **Benefits**: Prevents argument order mistakes, improves code clarity and self-documentation
- **Example**: `add(x = 5, y = 3)` instead of `add(5, 3)`

### Control Flow
- **If-Expressions**: `let max: Int32 = if (x > y) { x } else { y }`
- **Range Loops**: `for (i: Int32 in 0..10)` (exclusive), `for (i: Int32 in 0..=10)` (inclusive)
- **For-Each**: `for (item: Type in array)` works with arrays and custom classes (type annotation required)

### Testing
- **Test Blocks**: `test "description" { ... }` groups related tests
- **Test Functions**: Functions starting with `test_` are automatically discovered and run
- **Assertions**: `assert(condition = expr)` or `assert(condition = expr, message = "...")`
- **Helper Functions**: Non-test functions in test blocks provide shared setup/fixtures
- **Test Execution**: `plat test` compiles and runs all tests, reports results
- **Fail-Fast**: Assertion failures immediately stop the test and report the failure

### Benchmarking
- **Bench Blocks**: `bench "description" { ... }` groups related benchmarks
- **Bench Functions**: Functions starting with `bench_` are automatically discovered and run
- **Automatic Timing**: Framework handles iteration loops and timing measurement
- **Statistical Output**: Reports mean, median, standard deviation for each benchmark
- **Helper Functions**: Non-bench functions in bench blocks provide shared setup/fixtures
- **Bench Execution**: `plat bench` compiles and runs all benchmarks, reports performance metrics
- **Warmup Phase**: Executes warmup iterations before measurement to stabilize JIT/cache
- **Adaptive Iterations**: Automatically adjusts iteration count based on execution time

### Module System
- **Module Declarations**: `mod database;` at top of file
- **Imports**: `use database;` for namespace imports
- **Qualified Access**: `database::connect()` for cross-module calls
- **Multi-file Modules**: Multiple files can share the same module name
- **Folder Structure**: Must match module path (e.g., `database/connection.plat` → `mod database;`)

---

## 🛠️ CLI Commands

```bash
plat run <file.plat>     # Compile and run a single file
plat run                 # Run main.plat in current directory
plat build <file.plat>   # Compile to executable
plat build               # Compile all .plat files in project
plat test <file.plat>    # Run tests in a single file
plat test                # Run all tests in project
plat bench <file.plat>   # Run benchmarks in a single file
plat bench               # Run all benchmarks in project
plat fmt <file.plat>     # Format code with 2-space indentation
```

---

## 📦 Project Structure

```
plat2/
├── plat-cli/         # Main binary, CLI commands
├── plat-lexer/       # Tokenization
├── plat-parser/      # Recursive-descent parser
├── plat-ast/         # Abstract syntax tree
├── plat-hir/         # Type checking & semantic analysis
├── plat-codegen/     # Cranelift IR generation
├── plat-runtime/     # GC bindings, built-in functions
├── plat-fmt/         # Code formatter
├── plat-diags/       # Error reporting (Ariadne)
└── plat-modules/     # Module resolution & dependency graphs
```

---

## 🚀 Current Status

**✅ PRODUCTION READY:**
- Complete compiler pipeline (lexer → parser → HIR → codegen)
- String interpolation with expression evaluation
- Enums with pattern matching and exhaustiveness checking
- Generic collections (List, Dict, Set) with type safety
- Custom classes with OOP features
- Inheritance and polymorphism with vtables
- Generic functions with monomorphization
- Range-based for loops
- If-expressions
- Module system with cross-module function calls
- Type aliases
- Newtypes (zero-cost distinct types)
- Full numeric type support (Int8, Int16, Int32, Int64, Float8, Float16, Float32, Float64)
- Numeric literals with underscores (e.g., 1_000_000, 3.141_592_653)
- String methods (13 built-in functions)
- Set methods (11 built-in operations)
- Dict methods (11 built-in operations)
- Naming convention enforcement (compile-time validation)
- Default constructors (auto-generated init methods)
- Named arguments (required for all function/method/constructor/print calls)
- Built-in test framework with automatic test discovery, assertions, and runner

**📋 TODO (Stretch Goals):**
- [ ] Rich error messages with Ariadne spans
- [ ] Colored CLI output
- [ ] Generic constraints (`T: Display`)
- [ ] Type casting operators (`as i32`)
- [ ] `?` operator for Option/Result
- [ ] `if let` pattern matching

---

## 📝 Quick Reference

### Class Definition
```plat
// With explicit init
class Point {
  let x: Int32;
  var name: String;

  init(x: Int32, name: String) -> Point {
    self.x = x;
    self.name = name;
    return self;
  }

  fn get_x() -> Int32 {
    return self.x;
  }
}

// With default init (auto-generated)
class Point {
  let x: Int32;
  let y: Int32;
}

fn main() -> Int32 {
  let p: Point = Point.init(x = 10, y = 20);  // Constructor call with type annotation
  print(value = "Point created!");  // Named argument required
  return p.x;
}
```

### Enum with Pattern Matching
```plat
enum Status {
  Success,
  Warning(Int32),
  Error(Int32)
}

fn main() -> Int32 {
  let status: Status = Status::Warning(field0 = 42);
  let code: Int32 = match status {
    Status::Success -> 0,
    Status::Warning(x: Int32) -> x + 100,
    Status::Error(x: Int32) -> x + 200
  };
  return code;
}
```

### Generic Function
```plat
fn identity<T>(value: T) -> T {
  return value;
}

fn main() -> Int32 {
  let x: Int32 = identity(value = 10);
  let name: String = identity(value = "hello");
  return x;
}
```

### Module System
```plat
// math.plat
mod math;

fn add(a: Int32, b: Int32) -> Int32 {
  return a + b;
}

// main.plat
use math;

fn main() -> Int32 {
  return math::add(a = 5, b = 10);
}
```

### Newtypes
```plat
// Type aliases: interchangeable with underlying type
type Username = String;

// Newtypes: distinct type at compile-time, zero runtime overhead
newtype DocumentID = String;
newtype UserID = String;

fn process_user(id: UserID) -> Int32 {
  return 42;
}

fn main() -> Int32 {
  // ✅ Type alias works with raw string
  let name: Username = "john";

  // ❌ Newtype ERROR: cannot assign String to UserID
  // let user: UserID = "user123";

  // ❌ Newtype ERROR: DocumentID != UserID
  // let doc: DocumentID = user;

  return 0;
}
```

### Testing
```plat
class Point {
  let x: Int32;
  let y: Int32;

  fn add(other: Point) -> Point {
    return Point.init(x = self.x + other.x, y = self.y + other.y);
  }

  fn magnitude() -> Int32 {
    return (self.x * self.x) + (self.y * self.y);
  }
}

test "point operations" {
  fn test_addition() {
    let p1: Point = Point.init(x = 1, y = 2);
    let p2: Point = Point.init(x = 2, y = 4);
    let p3: Point = p1.add(other = p2);
    assert(condition = p3.x == 3, message = "X coordinate should be 3");
    assert(condition = p3.y == 6, message = "Y coordinate should be 6");
  }

  fn test_magnitude() {
    let p: Point = Point.init(x = 3, y = 4);
    assert(condition = p.magnitude() == 25, message = "3² + 4² = 25");
  }

  // Helper function (not a test, doesn't start with test_)
  fn create_origin() -> Point {
    return Point.init(x = 0, y = 0);
  }

  fn test_origin_magnitude() {
    let origin: Point = create_origin();
    assert(condition = origin.magnitude() == 0);
  }
}

fn main() -> Int32 {
  let p: Point = Point.init(x = 5, y = 10);
  print(value = "Point created!");
  return 0;
}
```

**Running tests:**
```bash
$ plat test point.plat
Running tests...
✓ point operations::test_addition
✓ point operations::test_magnitude
✓ point operations::test_origin_magnitude

3 tests, 3 passed, 0 failed
```

### Benchmarking
```plat
class Point {
  let x: Int32;
  let y: Int32;

  fn add(other: Point) -> Point {
    return Point.init(x = self.x + other.x, y = self.y + other.y);
  }

  fn magnitude() -> Int32 {
    return (self.x * self.x) + (self.y * self.y);
  }
}

bench "point operations" {
  // Helper function (not a benchmark, doesn't start with bench_)
  fn create_test_point() -> Point {
    return Point.init(x = 42, y = 84);
  }

  fn bench_point_creation() {
    let p: Point = Point.init(x = 10, y = 20);
  }

  fn bench_point_addition() {
    let p1: Point = Point.init(x = 1, y = 2);
    let p2: Point = Point.init(x = 3, y = 4);
    let p3: Point = p1.add(other = p2);
  }

  fn bench_magnitude_calculation() {
    let p: Point = create_test_point();
    let mag: Int32 = p.magnitude();
  }
}

fn main() -> Int32 {
  let p: Point = Point.init(x = 5, y = 10);
  return 0;
}
```

**Running benchmarks:**
```bash
$ plat bench point.plat
Running benchmarks...

point operations::bench_point_creation
  Iterations: 10,000,000
  Mean: 125ns
  Median: 120ns
  Std Dev: 15ns

point operations::bench_point_addition
  Iterations: 10,000,000
  Mean: 245ns
  Median: 240ns
  Std Dev: 22ns

point operations::bench_magnitude_calculation
  Iterations: 10,000,000
  Mean: 180ns
  Median: 175ns
  Std Dev: 18ns

3 benchmarks completed
```

---

## 🔧 Development Principles

1. **TDD First**: Write failing tests, make them pass, refactor
2. **No Fake Wiring**: Never hard-code outputs to look correct
3. **Frequent Commits**: Commit after each green step
4. **Plan Hygiene**: Update TODO.md for work tracking

---

For detailed implementation history and examples, see `CLAUDE_ARCHIVE.md`.
