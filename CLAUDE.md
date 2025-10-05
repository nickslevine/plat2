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

### Visibility System
- **Private by Default**: All class members and module items are private unless explicitly marked `pub`
- **Class Members**: Fields and methods are private to the class by default
- **Module Exports**: Functions, classes, enums, types are private to the module by default
- **Explicit Public**: Use `pub` keyword to make items accessible from outside
- **Compile-Time Enforcement**: Visibility violations are caught during type checking

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
- **Default Arguments**: Parameters can have default values: `fn add(x: Int32, y: Int32 = 10) -> Int32`
- **Omitting Defaults**: Call with fewer arguments: `add(x = 5)` uses default for `y`
- **Works With**: Functions, methods, and constructors all support default arguments

### Control Flow
- **If-Expressions**: `let max: Int32 = if (x > y) { x } else { y }`
- **Range Loops**: `for (i: Int32 in 0..10)` (exclusive), `for (i: Int32 in 0..=10)` (inclusive)
- **For-Each**: `for (item: Type in array)` works with arrays and custom classes (type annotation required)

### Type Casting
- **Numeric Casting**: `cast(value = expr, target = Type)` converts between numeric types
- **Float to Int**: Truncates towards zero (e.g., `cast(value = 3.7, target = Int32)` → `3`)
- **Int to Float**: Converts with appropriate precision for target type
- **Int to Int**: Wrapping behavior on overflow (two's complement)
- **Example**: `let z: Float32 = x + cast(value = y, target = Float32)`

### Testing
- **Test Blocks**: `test test_block_name { ... }` groups related tests (snake_case identifier required)
- **Test Functions**: Functions starting with `test_` are automatically discovered and run
- **Assertions**: `assert(condition = expr)` or `assert(condition = expr, message = "...")`
- **Helper Functions**: Non-test functions in test blocks provide shared setup/fixtures
- **Lifecycle Hooks**: `before_each()` and `after_each()` for setup/teardown
  - `before_each()` returns a context value injected into each test
  - `after_each(ctx)` receives the context for cleanup
  - Both hooks are optional
- **Test Execution**: `plat test` compiles and runs all tests, reports results
- **Test Filtering**: Filter tests with `-f`/`--filter` flag (supports glob patterns, can be repeated)
  - `plat test -f query_tests` - Run all tests in `query_tests` block
  - `plat test -f "query_tests.test_select_*"` - Run specific tests with wildcards
  - `plat test -f database.* -f auth.*` - Multiple filters (runs tests matching any filter)
  - `plat test -f "*.test_insert"` - Match test name across all blocks
  - Three-level filtering: `module.test_block.test_function`
- **Fail-Fast**: Assertion failures immediately stop the test and report the failure

### Benchmarking
- **Bench Blocks**: `bench bench_block_name { ... }` groups related benchmarks (snake_case identifier required)
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
plat run <file.plat>              # Compile and run a single file
plat run                          # Run main.plat in current directory
plat build <file.plat>            # Compile to executable
plat build                        # Compile all .plat files in project
plat test <file.plat>             # Run tests in a single file
plat test                         # Run all tests in project
plat test -f <pattern>            # Filter tests by pattern (glob syntax, repeatable)
plat bench <file.plat>            # Run benchmarks in a single file
plat bench                        # Run all benchmarks in project
plat fmt <file.plat>              # Format code with 2-space indentation
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
- String methods (17 built-in functions including parsing)
- Set methods (11 built-in operations)
- Dict methods (11 built-in operations)
- Naming convention enforcement (compile-time validation)
- Default constructors (auto-generated init methods)
- Named arguments (required for all function/method/constructor/print calls)
- Default arguments for functions, methods, and constructors
- Built-in test framework with automatic test discovery, assertions, runner, and filtering (glob patterns)
- Numeric type casting with cast() function (wrapping overflow, truncating float→int)
- **Result & Option integration:**
  - Collection indexing returns `Option<T>` for safe access
  - String parsing methods return `Result<T, String>` (parse_int, parse_int64, parse_float, parse_bool)
  - `?` operator for error propagation (basic support)
- **Visibility enforcement:**
  - Compile-time checking for field access (private by default)
  - Compile-time checking for method calls (private by default)
  - Compile-time checking for cross-module symbol access (functions, classes, enums)
  - Clear error messages for visibility violations
- **Beautiful error messages with Ariadne:**
  - Rich diagnostics with syntax highlighting and code snippets
  - Error codes (E001-E004) for common syntax errors
  - Helpful suggestions and "did you mean" for undefined symbols
  - Multi-label support showing related locations
  - Contextual help messages for fixing errors

**📋 TODO (Stretch Goals):**
- [ ] Generic constraints (`T: Display`)
- [ ] Complete `?` operator implementation with proper early returns
- [ ] `if let` pattern matching
- [ ] Main function Result/Option return types (codegen support)
- [ ] unwrap(), unwrap_or(), expect() methods for Result/Option

---

## 📝 Quick Reference

### Visibility Examples

**Class with Public and Private Members:**
```plat
class BankAccount {
  // Private fields (default)
  let account_number: String;
  let balance: Int32;

  // Public field
  pub let owner_name: String;

  // Private helper method
  fn validate_transaction(amount: Int32) -> Bool {
    return amount <= self.balance;
  }

  // Public methods
  pub fn get_balance() -> Int32 {
    return self.balance;
  }

  pub fn deposit(amount: Int32) -> Bool {
    if (self.validate_transaction(amount = amount)) {
      return true;
    } else {
      return false;
    }
  }
}

fn main() -> Int32 {
  let account: BankAccount = BankAccount.init(
    account_number = "12345",
    balance = 1000,
    owner_name = "Alice"
  );

  // ✅ OK: owner_name is public
  print(value = account.owner_name);

  // ✅ OK: get_balance is public
  let bal: Int32 = account.get_balance();

  // ❌ ERROR: balance is private
  // print(value = account.balance);

  // ❌ ERROR: validate_transaction is private
  // let valid: Bool = account.validate_transaction(amount = 50);

  return 0;
}
```

**Module with Public API:**
```plat
// database.plat
mod database;

// Private helper function
fn validate_connection_string(conn: String) -> Bool {
  return conn.length() > 0;
}

// Public API
pub fn connect(conn_string: String) -> Bool {
  return validate_connection_string(conn = conn_string);
}

pub class Connection {
  // Private internal state
  let socket_fd: Int32;

  // Public status field
  pub let is_connected: Bool;

  // Public method
  pub fn close() -> Bool {
    return true;
  }
}

// main.plat
use database;

fn main() -> Int32 {
  // ✅ OK: connect is public
  let connected: Bool = database::connect(conn_string = "localhost");

  // ✅ OK: Connection is public
  let conn: database::Connection = database::Connection.init(socket_fd = 42, is_connected = true);

  // ✅ OK: is_connected is public
  print(value = "Connected: ${conn.is_connected}");

  // ❌ ERROR: validate_connection_string is private
  // let valid: Bool = database::validate_connection_string(conn = "test");

  // ❌ ERROR: socket_fd is private
  // print(value = conn.socket_fd);

  return 0;
}
```

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

### Default Arguments
```plat
// Function with defaults
fn add(x: Int32, y: Int32 = 5, z: Int32 = 10) -> Int32 {
  return x + y + z;
}

// Class with default constructor and method
class Point {
  let x: Int32;
  let y: Int32;

  init(x: Int32 = 0, y: Int32 = 0) -> Point {
    self.x = x;
    self.y = y;
    return self;
  }

  fn distance(other_x: Int32 = 0, other_y: Int32 = 0) -> Int32 {
    let dx: Int32 = self.x - other_x;
    let dy: Int32 = self.y - other_y;
    return (dx * dx) + (dy * dy);
  }
}

fn main() -> Int32 {
  // Call with all arguments
  let sum1: Int32 = add(x = 1, y = 2, z = 3);  // 6

  // Call with some defaults
  let sum2: Int32 = add(x = 1, y = 2);  // 1 + 2 + 10 = 13

  // Call with all defaults
  let sum3: Int32 = add(x = 1);  // 1 + 5 + 10 = 16

  // Constructor with defaults
  let p1: Point = Point.init(x = 3, y = 4);
  let p2: Point = Point.init(x = 3);  // y defaults to 0
  let p3: Point = Point.init();  // Both default to 0

  // Method with defaults
  let dist: Int32 = p1.distance();  // Distance from origin

  return 0;
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

test point_operations {
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

// Testing with setup/teardown
class Connection {
  var is_open: Bool;

  fn close() {
    self.is_open = false;
  }
}

test database_operations {
  // Lifecycle hook: runs before each test, returns context
  fn before_each() -> Connection {
    let conn: Connection = Connection.init(is_open = true);
    return conn;
  }

  // Lifecycle hook: runs after each test, receives context
  fn after_each(ctx: Connection) {
    ctx.close();
  }

  // Context is automatically injected into test functions
  fn test_connection_starts_open(ctx: Connection) {
    assert(condition = ctx.is_open == true);
  }

  fn test_can_close_connection(ctx: Connection) {
    ctx.close();
    assert(condition = ctx.is_open == false);
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
✓ database operations::test_connection_starts_open
✓ database operations::test_can_close_connection

5 tests, 5 passed, 0 failed
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

bench point_operations {
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

point_operations::bench_point_creation
  Iterations: 10,000,000
  Mean: 125ns
  Median: 120ns
  Std Dev: 15ns

point_operations::bench_point_addition
  Iterations: 10,000,000
  Mean: 245ns
  Median: 240ns
  Std Dev: 22ns

point_operations::bench_magnitude_calculation
  Iterations: 10,000,000
  Mean: 180ns
  Median: 175ns
  Std Dev: 18ns

3 benchmarks completed
```

### Result & Option for Safe Error Handling

**String Parsing:**
```plat
fn main() -> Int32 {
  let input: String = "42";
  let result: Result<Int32, String> = input.parse_int();

  let value: Int32 = match result {
    Result::Ok(num: Int32) -> num,
    Result::Err(msg: String) -> {
      print(value = msg);
      return 1;
    }
  };

  print(value = "Parsed number: ${value}");
  return 0;
}
```

**Safe Collection Indexing:**
```plat
fn main() -> Int32 {
  let numbers: List[Int32] = [10, 20, 30];
  let maybe_value: Option<Int32> = numbers[5];  // Returns Option, not panic!

  let result: Int32 = match maybe_value {
    Option::Some(val: Int32) -> val,
    Option::None -> {
      print(value = "Index out of bounds!");
      return 1;
    }
  };

  print(value = "Value: ${result}");
  return 0;
}
```

**Error Propagation with ? Operator (Basic):**
```plat
fn parse_and_double(s: String) -> Result<Int32, String> {
  let num: Int32 = s.parse_int()?;  // Propagates error if parsing fails
  return Result::Ok(field0 = num * 2);
}

fn main() -> Int32 {
  let result: Result<Int32, String> = parse_and_double(s = "21");

  match result {
    Result::Ok(val: Int32) -> print(value = "Result: ${val}"),
    Result::Err(err: String) -> print(value = "Error: ${err}")
  };

  return 0;
}
```

**All Parsing Methods:**
- `parse_int() -> Result<Int32, String>` - Parse to 32-bit integer
- `parse_int64() -> Result<Int64, String>` - Parse to 64-bit integer
- `parse_float() -> Result<Float64, String>` - Parse to 64-bit float
- `parse_bool() -> Result<Bool, String>` - Parse "true" or "false"

---

## 🔧 Development Principles

1. **TDD First**: Write failing tests, make them pass, refactor
2. **No Fake Wiring**: Never hard-code outputs to look correct
3. **Frequent Commits**: Commit after each green step
4. Update CLAUDE.md to maintain working knowledge of project. 
5. As you add features and make changes, make sure to always add helpful compiler error messages as we already have for existing functionality.  

---

For detailed implementation history and examples, see `CLAUDE_ARCHIVE.md`.
