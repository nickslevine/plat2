# 📜 Plat Language Compiler

**A modern compiled language with:**
- Native code generation (Rust + Cranelift)
- Strong static typing with type inference
- Object-oriented programming (classes, inheritance, polymorphism)
- Algebraic data types (enums, pattern matching)
- Generic types and functions
- Module system with dependency resolution
- GC-managed memory

---

## 🎯 Core Language Features

### Type System
- **Primitives**: `bool`, `i32`, `i64`, `f32`, `f64`, `string`
- **Collections**: `List[T]`, `Dict[K, V]`, `Set[T]`
- **Built-in Enums**: `Option<T>`, `Result<T, E>`
- **Type Aliases**: `type UserID = string;`

### Object-Oriented Programming
- **Classes**: Field declarations with `let`/`var` mutability
- **Inheritance**: `class Dog : Animal` with virtual methods
- **Polymorphism**: Safe upcasting, vtable-based dynamic dispatch
- **Generics**: `class Container<T>`, `fn identity<T>(value: T) -> T`

### Pattern Matching
- **Enums**: Unit variants, data variants, multi-field variants
- **Match Expressions**: Exhaustiveness checking, pattern binding
- **Example**: `match status { Status::Success -> 1, Status::Error(code) -> code }`

### Control Flow
- **If-Expressions**: `let max = if (x > y) { x } else { y }`
- **Range Loops**: `for (i in 0..10)` (exclusive), `for (i in 0..=10)` (inclusive)
- **For-Each**: `for (item in array)` works with arrays and custom classes

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
- Float support (f32/f64)
- String methods (13 built-in functions)
- Set methods (11 built-in operations)
- Dict methods (11 built-in operations)

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
class Point {
  let x: i32;
  var name: string;

  init(x: i32, name: string) -> Point {
    self.x = x;
    self.name = name;
    return self;
  }

  fn get_x() -> i32 {
    return self.x;
  }
}
```

### Enum with Pattern Matching
```plat
enum Status {
  Success,
  Warning(i32),
  Error(i32)
}

fn main() -> i32 {
  let status = Status::Warning(42);
  let code = match status {
    Status::Success -> 0,
    Status::Warning(x) -> x + 100,
    Status::Error(x) -> x + 200
  };
  return code;
}
```

### Generic Function
```plat
fn identity<T>(value: T) -> T {
  return value;
}

fn main() -> i32 {
  let x = identity(10);
  let name = identity("hello");
  return x;
}
```

### Module System
```plat
// math.plat
mod math;

fn add(a: i32, b: i32) -> i32 {
  return a + b;
}

// main.plat
use math;

fn main() -> i32 {
  return math::add(5, 10);
}
```

---

## 🔧 Development Principles

1. **TDD First**: Write failing tests, make them pass, refactor
2. **No Fake Wiring**: Never hard-code outputs to look correct
3. **Frequent Commits**: Commit after each green step
4. **Plan Hygiene**: Update TODO.md for work tracking

---

For detailed implementation history and examples, see `CLAUDE_ARCHIVE.md`.
