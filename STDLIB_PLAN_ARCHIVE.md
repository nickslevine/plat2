# Plat Standard Library - Archived Documentation

This file contains detailed implementation notes, code examples, and design documents that have been archived from the main STDLIB_PLAN.md for reference purposes.

---

## Table of Contents

1. [std::io Implementation Pattern](#stdio-implementation-pattern)
2. [std::json Implementation Details](#stdjson-implementation-details)
3. [Future Module Designs](#future-module-designs)
4. [Custom Error Types](#custom-error-types)
5. [Detailed Testing Strategy](#detailed-testing-strategy)
6. [Performance Considerations](#performance-considerations)
7. [Security Considerations](#security-considerations)
8. [Historical Fixes](#historical-fixes)

---

## std::io Implementation Pattern

### Implementation with Match Expression Workaround

Due to Plat's limitation (no multi-statement blocks in match arms), we use this pattern:

```plat
pub fn read_file(path: String) -> Result<String, String> {
  let fd_result: Result<Int32, String> = file_open(path = path, mode = "r");

  // Pattern: Check for error first
  let has_error: Bool = match fd_result {
    Result::Ok(fd: Int32) -> false,
    Result::Err(err: String) -> true
  };

  // Then handle error case with early return
  if (has_error) {
    return match fd_result {
      Result::Ok(fd: Int32) -> Result::Err(field0 = "impossible"),
      Result::Err(err: String) -> Result::Err(field0 = err)
    };
  }

  // Extract value in separate match
  let fd: Int32 = match fd_result {
    Result::Ok(descriptor: Int32) -> descriptor,
    Result::Err(err: String) -> -1
  };

  let content: Result<String, String> = file_read(fd = fd, max_bytes = 10485760);
  let close_result: Result<Bool, String> = file_close(fd = fd);
  return content;
}
```

---

## std::json Implementation Details

### Language Workarounds Applied

**No `||` operator:**
```plat
// Instead of: if (ch == "0" || ch == "1" || ch == "2")
if (ch == "0") {
  // handle digit
} else if (ch == "1") {
  // handle digit
} else if (ch == "2") {
  // handle digit
}
```

**No `&&` operator:**
```plat
// Instead of: if (index < length && ch == "\"")
if (index < length) {
  if (ch == "\"") {
    // handle quote
  }
}
```

**No `!` operator:**
```plat
// Instead of: if (!is_valid)
if (is_valid == false) {
  // handle invalid
}
```

**No `break` statement:**
```plat
// Instead of: while (true) { if (done) break; }
var continue_loop: Bool = true;
while (continue_loop) {
  if (done) {
    continue_loop = false;
  } else {
    // continue processing
  }
}
```

### Clean else-if Syntax Example

```plat
if (ch == "n") {
  return self.parse_null();
} else if (ch == "t") {
  return self.parse_bool();
} else if (ch == "f") {
  return self.parse_bool();
} else if (ch == "\"") {
  return self.parse_string();
} else if (ch == "[") {
  return self.parse_array();
} else if (ch == "{") {
  return self.parse_object();
} else {
  return Result::Err(field0 = "Unexpected character: ${ch}");
}
```

---

## Future Module Designs

### std::fs (File System Utilities)

Complete pathlib-style Path class with methods:

**Path Manipulation (Pure - No I/O):**
- `new(path: String) -> Path` - Constructor
- `join(other: String) -> Path` - Join paths
- `parent() -> Option<Path>` - Get parent directory
- `name() -> Option<String>` - Get filename
- `stem() -> Option<String>` - Filename without extension
- `suffix() -> Option<String>` - File extension
- `with_name(name: String) -> Path` - Replace filename
- `with_suffix(suffix: String) -> Path` - Replace extension
- `is_absolute() -> Bool` - Check if path is absolute
- `parts() -> List[String]` - Split path into components

**File System Queries (Read-Only I/O):**
- `exists() -> Bool` - Check if path exists
- `is_file() -> Bool` - Check if path is a file
- `is_dir() -> Bool` - Check if path is a directory
- `is_symlink() -> Bool` - Check if path is a symlink
- `size() -> Result<Int64, String>` - Get file size
- `permissions() -> Result<Int32, String>` - Get file permissions
- `modified() -> Result<Int64, String>` - Get last modified time
- `created() -> Result<Int64, String>` - Get creation time

**File Operations (Mutating I/O):**
- `read_text() -> Result<String, String>` - Read entire file as string
- `read_bytes() -> Result<List[Int8], String>` - Read entire file as binary
- `write_text(content: String) -> Result<Bool, String>` - Write string to file
- `write_bytes(content: List[Int8]) -> Result<Bool, String>` - Write binary to file
- `append_text(content: String) -> Result<Bool, String>` - Append string
- `unlink() -> Result<Bool, String>` - Delete file
- `rename(new_path: Path) -> Result<Bool, String>` - Rename/move file
- `chmod(mode: Int32) -> Result<Bool, String>` - Change permissions

**Directory Operations:**
- `mkdir() -> Result<Bool, String>` - Create directory
- `mkdir_parents() -> Result<Bool, String>` - Create with parents
- `rmdir() -> Result<Bool, String>` - Remove empty directory
- `list_dir() -> Result<List[Path], String>` - List directory contents
- `iterdir() -> Result<List[Path], String>` - Iterate over entries

**Symlink Operations:**
- `symlink_to(target: Path) -> Result<Bool, String>` - Create symlink
- `readlink() -> Result<String, String>` - Read symlink target
- `resolve() -> Result<Path, String>` - Resolve symlink to actual path

**Standalone Utility Functions:**
- `copy_tree(src: Path, dst: Path) -> Result<Bool, String>` - Recursive copy
- `remove_tree(path: Path) -> Result<Bool, String>` - Recursive delete
- `cwd() -> Result<Path, String>` - Get current working directory
- `home() -> Result<Path, String>` - Get home directory
- `temp_dir() -> Result<Path, String>` - Get temporary directory

### std::net (High-Level Networking)

```plat
pub class TcpListener {
  let socket: Int32;

  pub fn bind(host: String, port: Int32) -> Result<TcpListener, String>
  pub fn accept() -> Result<TcpStream, String>
}

pub class TcpStream {
  let socket: Int32;

  pub fn connect(host: String, port: Int32) -> Result<TcpStream, String>
  pub fn read(max_bytes: Int32) -> Result<String, String>
  pub fn write(data: String) -> Result<Int32, String>
  pub fn close() -> Result<Bool, String>
}
```

### std::http (HTTP Client/Server)

```plat
pub class Request {
  pub let method: String;
  pub let path: String;
  pub let headers: Dict[String, String];
  pub let body: String;
}

pub class Response {
  pub let status: Int32;
  pub let headers: Dict[String, String];
  pub let body: String;
}

pub fn get(url: String) -> Result<Response, String>
pub fn post(url: String, body: String) -> Result<Response, String>

pub class Server {
  pub fn listen(port: Int32, handler: fn(Request) -> Response) -> Result<Bool, String>
}
```

### std::collections (Additional Data Structures)

```plat
pub class Queue<T> {
  var items: List[T];

  pub fn enqueue(item: T)
  pub fn dequeue() -> Option<T>
}

pub class Stack<T> {
  var items: List[T];

  pub fn push(item: T)
  pub fn pop() -> Option<T>
}
```

### std::math (Mathematical Functions)

```plat
pub fn sqrt(x: Float64) -> Float64
pub fn pow(base: Float64, exp: Float64) -> Float64
pub fn sin(x: Float64) -> Float64
pub fn cos(x: Float64) -> Float64
pub fn abs(x: Float64) -> Float64

pub let PI: Float64 = 3.141592653589793;
pub let E: Float64 = 2.718281828459045;
```

### std::time (Time/Date Utilities)

```plat
pub class Duration {
  let milliseconds: Int64;

  pub fn from_seconds(seconds: Int64) -> Duration
  pub fn from_minutes(minutes: Int64) -> Duration
  pub fn to_seconds() -> Int64
}

pub fn now() -> Int64
pub fn sleep(duration: Duration)
```

### std::string (Extended String Utilities)

```plat
pub fn join(strings: List[String], separator: String) -> String
pub fn repeat(s: String, count: Int32) -> String
pub fn reverse(s: String) -> String
pub fn is_numeric(s: String) -> Bool
pub fn is_alpha(s: String) -> Bool
```

---

## Custom Error Types

### Why Structured Errors?

Currently stdlib uses `Result<T, String>` everywhere, which has issues:
- No way to programmatically distinguish error types
- Can't pattern match on error categories
- No context (error code, location, etc.)
- Poor error composition

### std::io Error Types

```plat
pub enum IoError {
  NotFound(String),           // path
  PermissionDenied(String),   // path
  AlreadyExists(String),      // path
  InvalidInput(String),       // message
  UnexpectedEof,
  Other(String)
}

impl IoError {
  pub fn to_string() -> String { /* ... */ }
  pub fn code() -> Int32 { /* ... */ }
}

pub type IoResult<T> = Result<T, IoError>;
```

### std::json Error Types

```plat
pub enum JsonError {
  UnexpectedChar(Int32, String),      // position, character
  UnexpectedEof,
  InvalidNumber(String),               // value
  InvalidEscape(String),               // sequence
  ExpectedToken(String, String),       // expected, found
  TrailingChars(Int32),                // position
}

impl JsonError {
  pub fn to_string() -> String { /* ... */ }
}

pub type JsonResult<T> = Result<T, JsonError>;
```

### Pattern Matching on Error Types

```plat
use std::io;

fn main() -> Int32 {
  let result: io::IoResult<String> = io::read_file(path = "config.json");

  match result {
    Result::Ok(content: String) -> {
      print(value = "Read ${content.length()} bytes");
    },
    Result::Err(err: io::IoError) -> {
      match err {
        io::IoError::NotFound(path: String) -> {
          print(value = "Creating default config at ${path}");
          return 0;
        },
        io::IoError::PermissionDenied(path: String) -> {
          print(value = "Permission denied: ${path}");
          return 13;
        },
        _ -> {
          print(value = "Error: ${err.to_string()}");
          return 1;
        }
      }
    }
  };

  return 0;
}
```

---

## Detailed Testing Strategy

### Comprehensive Unit Test Example

```plat
test json_parser {
  // ============================================================================
  // Happy Path Tests
  // ============================================================================

  fn test_parse_null() {
    let result: JsonResult<JsonValue> = parse(input = "null");
    match result {
      Result::Ok(value: JsonValue) -> {
        match value {
          JsonValue::Null -> assert(condition = true),
          _ -> assert(condition = false, message = "Expected null")
        }
      },
      Result::Err(err: JsonError) -> {
        assert(condition = false, message = "Parse failed: ${err.to_string()}");
      }
    };
  }

  fn test_parse_number_integer() {
    let result: JsonResult<JsonValue> = parse(input = "42");
    // assertions...
  }

  fn test_parse_string_with_escapes() {
    let result: JsonResult<JsonValue> = parse(input = "\"hello\\nworld\\t!\"");
    // assertions...
  }

  fn test_parse_array_mixed() {
    let result: JsonResult<JsonValue> = parse(input = "[1, \"two\", true, null]");
    // assertions...
  }

  fn test_parse_object_nested() {
    let input: String = "{\"person\": {\"name\": \"Bob\", \"address\": {\"city\": \"NYC\"}}}";
    let result: JsonResult<JsonValue> = parse(input = input);
    // assertions...
  }

  // ============================================================================
  // Edge Cases
  // ============================================================================

  fn test_parse_empty_string() {
    let result: JsonResult<JsonValue> = parse(input = "");
    match result {
      Result::Err(err: JsonError) -> {
        match err {
          JsonError::UnexpectedEof -> assert(condition = true),
          _ -> assert(condition = false, message = "Expected UnexpectedEof error")
        }
      },
      Result::Ok(_) -> {
        assert(condition = false, message = "Should fail on empty input");
      }
    };
  }

  fn test_parse_deeply_nested() {
    let input: String = "[[[[[[[[[[\"deep\"]]]]]]]]]]";
    let result: JsonResult<JsonValue> = parse(input = input);
    // assertions...
  }

  // ============================================================================
  // Error Cases
  // ============================================================================

  fn test_parse_invalid_character() {
    let result: JsonResult<JsonValue> = parse(input = "{invalid}");
    match result {
      Result::Err(err: JsonError) -> {
        match err {
          JsonError::UnexpectedChar(pos: Int32, ch: String) -> {
            assert(condition = pos >= 0, message = "Position should be non-negative");
          },
          _ -> assert(condition = false, message = "Expected UnexpectedChar error")
        }
      },
      Result::Ok(_) -> {
        assert(condition = false, message = "Should fail on invalid input");
      }
    };
  }

  fn test_parse_trailing_chars() {
    let result: JsonResult<JsonValue> = parse(input = "null garbage");
    match result {
      Result::Err(err: JsonError) -> {
        match err {
          JsonError::TrailingChars(pos: Int32) -> assert(condition = true),
          _ -> assert(condition = false, message = "Expected TrailingChars error")
        }
      },
      Result::Ok(_) -> assert(condition = false)
    };
  }

  // ============================================================================
  // Round-Trip Tests
  // ============================================================================

  fn test_roundtrip_object() {
    // Create object, stringify, parse back, verify equality
  }
}
```

### Benchmark Example

```plat
bench json_performance {
  fn create_large_object() -> JsonValue {
    // Helper to create test data
    return JsonValue::Null;
  }

  fn bench_parse_small() {
    let result: JsonResult<JsonValue> = parse(input = "{\"key\": \"value\"}");
  }

  fn bench_parse_large() {
    let large_json: String = "...";
    let result: JsonResult<JsonValue> = parse(input = large_json);
  }

  fn bench_stringify_small() {
    let value: JsonValue = JsonValue::Null;
    let result: String = stringify(value = value);
  }

  fn bench_stringify_large() {
    let value: JsonValue = create_large_object();
    let result: String = stringify(value = value);
  }
}
```

### Test Coverage Requirements

**Per Module Coverage:**
- std::io: >95% (critical I/O operations)
- std::json: >90% (parser must handle all edge cases)
- std::fs: >90% (file system operations)
- std::net: >85% (network I/O)
- std::http: >85% (HTTP protocol handling)

---

## Performance Considerations

### Selective Linking

**Problem**: Don't want to link entire stdlib if user only imports one module

**Solution**: Incremental compilation and selective linking
- Each stdlib module compiles to separate object file
- Linker only includes object files for imported modules
- Dead code elimination removes unused functions

**Example:**
```plat
use std::json;  // Only links std/json.o, not std/http.o
```

### Caching Strategy

**First Compilation:**
```
stdlib/std/json.plat → parse → HIR → cache to target/stdlib-cache/std-json.o
```

**Subsequent Compilations:**
```
Check cache for std-json.o → if fresh, load from cache
                          ↓
                       Skip parsing & type checking & codegen
                          ↓
                       Link directly
```

**Speedup**: ~10x faster compilation for projects using stdlib

### Inlining Opportunities

Some stdlib functions are small enough to inline:

```plat
pub fn abs(x: Int32) -> Int32 {
  if (x < 0) {
    return -x;
  } else {
    return x;
  }
}
```

Future optimization: `#[inline]` attribute for hot stdlib functions

---

## Security Considerations

### Trusted Stdlib Path

- Stdlib path is baked into compiler binary (not user-configurable)
- Prevents malicious code injection via fake stdlib
- Users cannot override `std::` namespace

### Sandboxing (Future)

For untrusted Plat code execution:
- Disable file system access (stub out `file_*` functions)
- Disable network access (stub out `tcp_*` functions)
- Limited execution time (wrap main with timeout)

---

## Historical Fixes

### Cross-Module Function Call Codegen Fix

**Issue**: Cross-module function calls failed because codegen incorrectly handled function signatures

**Problem Analysis**:
1. **Implicit Self Parameter Bug** (Fixed)
   - Root Cause: Codegen used `name.contains("::")` to detect methods
   - Solution: Added `method_names: HashSet<String>` to track actual methods

2. **Function Signature Resolution** (Fixed in commit a819495)
   - Root Cause: Cross-module functions assumed all parameters/returns are `i64`
   - Solution: Threaded global symbol table through codegen
   - Implementation: Added `symbol_table` parameter to 162+ call sites

**Current Status**: All cross-module function calls work with correct types

### Type Checker Limitation Discovery

**Issue**: Generic enum constructor type inference didn't respect function return types

**Problem**:
```plat
fn make_string_error(msg: String) -> Result<String, String> {
  // Type checker inferred Result<Int32, String> from context
  return Result::Err(field0 = msg);  // ❌ Type error!
}
```

**Solution** (Implemented):
- Added `expected_type: Option<&HirType>` parameter to `check_expression()`
- Pass expected return type down in return statements
- Updated enum constructor inference to use expected type when available

**Now Works**:
```plat
fn make_error(msg: String) -> Result<String, String> {
  return Result::Err(field0 = msg);  // ✅ Correctly infers Result<String, String>
}
```

### Recursive Enum Type Support

**Issue**: Forward reference problem with recursive enums

**Problem**:
```plat
pub enum JsonValue {
  Array(List[JsonValue]),    // Error: Unknown type 'JsonValue'
}
```

**Solution**: Two-phase registration
1. Phase 1: Register enum names with empty variants
2. Phase 2: Resolve variant field types (enum names now available)

**Implementation**:
- Updated `setup_global_symbols()` for global registration
- Split `collect_enum_info()` into `register_enum_name()` and `collect_enum_variants()`
- Updated `check_program()` to call both phases

**Now Works**:
```plat
pub enum JsonValue {
  Array(List[JsonValue]),              // ✅ Recursive reference works!
  Object(Dict[String, JsonValue])
}
```

---

**End of Archive**

For current stdlib status and next steps, see `STDLIB_PLAN.md`.
