# Plat Standard Library Architecture Plan

> **📝 IMPORTANT**: After completing any chunk of work on the stdlib, always:
> 1. Update this plan with progress details
> 2. Commit the changes to git
> 3. Keep the status section at the bottom current

## Overview

This document outlines the design and implementation plan for the Plat standard library (`std`). The stdlib will be written entirely in Plat (dogfooding!), providing high-level abstractions over the low-level Rust FFI primitives.

## Directory Structure

```
plat2/
├── stdlib/              # NEW: Standard library in Plat
│   ├── std/
│   │   ├── json.plat         # JSON parsing/serialization
│   │   ├── io.plat           # High-level I/O wrappers
│   │   ├── fs.plat           # File system utilities
│   │   ├── net.plat          # Networking utilities
│   │   ├── http.plat         # HTTP client/server
│   │   ├── collections.plat  # Additional collections (Queue, Stack)
│   │   ├── math.plat         # Math functions
│   │   ├── time.plat         # Time/date handling
│   │   └── string.plat       # Extended string utilities
│   └── README.md
├── crates/
│   ├── plat-modules/    # EXTEND: Recognize std:: prefix
│   ├── plat-cli/        # EXTEND: Pass stdlib path to compiler
│   └── plat-runtime/    # ADD: New primitives (time, env, random)
```

## Current Primitive Layer (Already Implemented)

The following are implemented as Rust FFI functions in `plat-runtime/src/ffi/`:

### Networking (net.rs)
- `tcp_listen(host: String, port: Int32) -> Result<Int32, String>`
- `tcp_accept(listener: Int32) -> Result<Int32, String>`
- `tcp_connect(host: String, port: Int32) -> Result<Int32, String>`
- `tcp_read(socket: Int32, max_bytes: Int32) -> Result<String, String>`
- `tcp_write(socket: Int32, data: String) -> Result<Int32, String>`
- `tcp_close(socket: Int32) -> Result<Bool, String>`

### File System (fs.rs)
- `file_open(path: String, mode: String) -> Result<Int32, String>`
- `file_read(fd: Int32, max_bytes: Int32) -> Result<String, String>`
- `file_write(fd: Int32, data: String) -> Result<Int32, String>`
- `file_close(fd: Int32) -> Result<Bool, String>`
- `file_read_binary(fd: Int32, max_bytes: Int32) -> Result<List[Int8], String>`
- `file_write_binary(fd: Int32, data: List[Int8]) -> Result<Int32, String>`
- `file_exists(path: String) -> Bool`
- `file_size(path: String) -> Result<Int64, String>`
- `file_is_dir(path: String) -> Bool`
- `file_is_symlink(path: String) -> Bool`
- `file_delete(path: String) -> Result<Bool, String>`
- `file_rename(old_path: String, new_path: String) -> Result<Bool, String>`
- `file_seek(fd: Int32, offset: Int64, whence: Int32) -> Result<Int64, String>`
- `file_tell(fd: Int32) -> Result<Int64, String>`
- `file_rewind(fd: Int32) -> Result<Bool, String>`
- `file_chmod(path: String, mode: Int32) -> Result<Bool, String>`
- `file_get_permissions(path: String) -> Result<Int32, String>`
- `file_modified_time(path: String) -> Result<Int64, String>`
- `file_created_time(path: String) -> Result<Int64, String>`
- `dir_create(path: String) -> Result<Bool, String>`
- `dir_create_all(path: String) -> Result<Bool, String>`
- `dir_remove(path: String) -> Result<Bool, String>`
- `dir_list(path: String) -> Result<String, String>`
- `symlink_create(target: String, link: String) -> Result<Bool, String>`
- `symlink_read(path: String) -> Result<String, String>`
- `symlink_delete(path: String) -> Result<Bool, String>`

### String Operations (string.rs)
- 17 built-in methods: `length()`, `substring()`, `concat()`, `split()`, `trim()`, `to_upper()`, `to_lower()`, `contains()`, `starts_with()`, `ends_with()`, `replace()`, `parse_int()`, `parse_int64()`, `parse_float()`, `parse_bool()`, `char_at()`, `index_of()`

### Collections
- List[T]: indexing, `length()`, `push()`, `pop()`, etc.
- Dict[K, V]: 11 built-in operations
- Set[T]: 11 built-in operations

### Concurrency (green_runtime/)
- Green thread runtime with work-stealing scheduler
- `concurrent {}` blocks with structured concurrency
- `spawn { ... }` for task creation
- `.await()` for blocking on task completion

## Module Resolution Strategy

### Special `std::` Handling

1. **Reserved Namespace**: The `std::` prefix is reserved for standard library modules
2. **Search Path Priority**:
   - User modules: Current project directory
   - Standard library: `stdlib/` directory (for `std::*` imports only)
3. **Module Path Mapping**:
   - `use std::json;` → `stdlib/std/json.plat`
   - `use std::io;` → `stdlib/std/io.plat`
4. **Validation**: User modules cannot use `std::` prefix (compile error)

### Compilation Flow

```
User Code (main.plat)
  ↓
use std::json;
  ↓
Module Resolver discovers stdlib/std/json.plat
  ↓
Check cache for compiled HIR (target/stdlib-cache/std-json.hir)
  ↓
If cached and not stale: load cached HIR
If not cached: compile stdlib/std/json.plat → HIR
  ↓
Type check user code with stdlib symbols
  ↓
Codegen: only link imported stdlib modules
  ↓
Final binary (minimal size, only includes what's used)
```

### Caching Strategy (Phase 2)

**Why Cache?**
- Stdlib modules rarely change
- Compiling from scratch every time is slow
- HIR is platform-independent (can be serialized)

**Cache Location**: `target/stdlib-cache/`

**Cache Key**: `{module_name}-{file_hash}.hir`

**Cache Invalidation**:
- Compare file modification time
- If source newer than cache: recompile
- If cache missing: compile and cache

**What Gets Cached?**
- HIR (type-checked intermediate representation)
- Type signatures (for cross-module resolution)
- Metadata (module dependencies, public symbols)

## Implementation Phases

### Phase 1: Infrastructure (Core) ✅ COMPLETED

**Goal**: Make `use std::*` work

**Status**: Completed on 2025-01-XX (Commit: 07106ee)

**What Was Implemented**:
1. ✅ Created `stdlib/std/` directory structure
2. ✅ `ModuleResolver` already had `stdlib_dir` field and `std::` handling
3. ✅ CLI already had `get_stdlib_root()` helper
4. ✅ **Parser Fix**: Added `consume_module_name()` to accept keywords in module paths
   - Allows `std::test` where `test` is a keyword
   - Updated `parse_use_decl()`, `parse_module_decl()`, and qualified path parsing
5. ✅ Created test module: `stdlib/std/test.plat` with public functions
6. ✅ Verified: `use std::test;` compiles and runs successfully

**Key Achievement**: Parser now accepts keywords (test, mod, type, bench, etc.) as valid module path components, enabling stdlib modules to use any name.

**Success Criteria Met**:
- ✅ `use std::test;` compiles without errors
- ✅ Module system discovers stdlib modules from `stdlib/` directory
- ✅ Module not found error for non-existent stdlib modules

**Note**: There's an existing codegen issue with cross-module function calls (affects all modules, not stdlib-specific). This will be addressed separately.

---

### Phase 2: Module Caching (Performance)

**Goal**: Cache compiled stdlib modules for fast rebuilds

**Tasks**:
1. Implement HIR serialization with serde
2. Add `target/stdlib-cache/` directory
3. Create `StdlibCache` struct in `plat-modules`:
   - `fn get(module: &str) -> Option<CachedModule>`
   - `fn put(module: &str, hir: &HirModule)`
   - `fn invalidate(module: &str)`
4. Check cache before compiling stdlib modules
5. Store file hash with cached HIR for invalidation
6. Benchmark: measure compilation speedup

**Success Criteria**:
- First compilation: full stdlib compile
- Second compilation: instant (loaded from cache)
- Modifying stdlib module: only that module recompiles

---

### Phase 3: std::io (First Stdlib Module)

**Goal**: High-level I/O wrappers with ergonomic API

**Module**: `stdlib/std/io.plat`

```plat
mod std::io;

// Simple file reading
pub fn read_file(path: String) -> Result<String, String> {
  let fd_result: Result<Int32, String> = file_open(path = path, mode = "r");

  let fd: Int32 = match fd_result {
    Result::Ok(descriptor: Int32) -> descriptor,
    Result::Err(err: String) -> {
      return Result::Err(field0 = err);
    }
  };

  let content: Result<String, String> = file_read(fd = fd, max_bytes = 1048576);
  let close_result: Result<Bool, String> = file_close(fd = fd);

  return content;
}

// Simple file writing
pub fn write_file(path: String, content: String) -> Result<Bool, String> {
  let fd_result: Result<Int32, String> = file_open(path = path, mode = "w");

  let fd: Int32 = match fd_result {
    Result::Ok(descriptor: Int32) -> descriptor,
    Result::Err(err: String) -> {
      return Result::Err(field0 = err);
    }
  };

  let write_result: Result<Int32, String> = file_write(fd = fd, data = content);
  let close_result: Result<Bool, String> = file_close(fd = fd);

  match write_result {
    Result::Ok(bytes: Int32) -> Result::Ok(field0 = true),
    Result::Err(err: String) -> Result::Err(field0 = err)
  }
}

// Buffered reader (performance optimization)
pub class Reader {
  let fd: Int32;
  var buffer: String;
  var position: Int32;
  let buffer_size: Int32;

  pub fn read_line() -> Result<String, String> {
    // TODO: Implement buffered line reading
    return Result::Err(field0 = "Not implemented");
  }
}

// Buffered writer
pub class Writer {
  let fd: Int32;
  var buffer: String;
  let buffer_size: Int32;

  pub fn write(data: String) -> Result<Bool, String> {
    // TODO: Implement buffered writing
    return Result::Err(field0 = "Not implemented");
  }

  pub fn flush() -> Result<Bool, String> {
    // TODO: Flush buffer to disk
    return Result::Err(field0 = "Not implemented");
  }
}
```

**Tests**: Create `test` block in `std::io.plat`

**Success Criteria**:
- User can `use std::io;`
- `io::read_file()` and `io::write_file()` work
- Tests pass

---

### Phase 4: std::json (Pure Plat Implementation!)

**Goal**: JSON parser written entirely in Plat (no Rust!)

**Module**: `stdlib/std/json.plat`

```plat
mod std::json;

// JSON value representation
pub enum JsonValue {
  Null,
  Bool(Bool),
  Number(Float64),
  String(String),
  Array(List[JsonValue]),
  Object(Dict[String, JsonValue])
}

// Parse JSON string into JsonValue
pub fn parse(input: String) -> Result<JsonValue, String> {
  let parser: Parser = Parser.init(input = input);
  return parser.parse_value();
}

// Stringify JsonValue into JSON string
pub fn stringify(value: JsonValue) -> String {
  match value {
    JsonValue::Null -> "null",
    JsonValue::Bool(b: Bool) -> {
      if (b) {
        return "true";
      } else {
        return "false";
      }
    },
    JsonValue::Number(n: Float64) -> {
      // TODO: Convert Float64 to String (need stdlib function)
      return "0.0";
    },
    JsonValue::String(s: String) -> {
      // TODO: Escape special characters
      return "\"${s}\"";
    },
    JsonValue::Array(arr: List[JsonValue]) -> {
      // TODO: Stringify array elements
      return "[]";
    },
    JsonValue::Object(obj: Dict[String, JsonValue]) -> {
      // TODO: Stringify object key-value pairs
      return "{}";
    }
  }
}

// Internal parser class
class Parser {
  let input: String;
  var position: Int32;

  fn parse_value() -> Result<JsonValue, String> {
    // TODO: Implement recursive descent parser
    return Result::Err(field0 = "Not implemented");
  }

  fn parse_object() -> Result<JsonValue, String> {
    // TODO: Parse { "key": value, ... }
    return Result::Err(field0 = "Not implemented");
  }

  fn parse_array() -> Result<JsonValue, String> {
    // TODO: Parse [ value, value, ... ]
    return Result::Err(field0 = "Not implemented");
  }

  fn parse_string() -> Result<String, String> {
    // TODO: Parse quoted string with escape sequences
    return Result::Err(field0 = "Not implemented");
  }

  fn parse_number() -> Result<Float64, String> {
    // TODO: Parse numeric literal
    return Result::Err(field0 = "Not implemented");
  }

  fn skip_whitespace() {
    // TODO: Skip spaces, tabs, newlines
  }
}
```

**Tests**: Comprehensive JSON test suite

**Success Criteria**:
- Parse valid JSON (objects, arrays, primitives)
- Reject invalid JSON with error messages
- Round-trip: `stringify(parse(json)) == json` (modulo formatting)

---

### Phase 5: Additional Primitives (Expand Runtime)

**Goal**: Add missing primitives needed by stdlib

**New FFI Functions in plat-runtime**:

#### Time (ffi/time.rs)
```rust
#[no_mangle]
pub extern "C" fn plat_time_now() -> i64 {
    // Unix timestamp in milliseconds
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

#[no_mangle]
pub extern "C" fn plat_time_sleep(milliseconds: i64) {
    std::thread::sleep(std::time::Duration::from_millis(milliseconds as u64));
}
```

#### Environment (ffi/env.rs)
```rust
// Returns pointer to String (or NULL if not found)
#[no_mangle]
pub extern "C" fn plat_env_get(key: *const RuntimeString) -> *mut RuntimeString {
    // TODO: Implement with Result<String, String> return type
}

#[no_mangle]
pub extern "C" fn plat_env_set(key: *const RuntimeString, value: *const RuntimeString) -> bool {
    // TODO: Implement
}
```

#### Random (ffi/random.rs)
```rust
#[no_mangle]
pub extern "C" fn plat_random_int(min: i32, max: i32) -> i32 {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    rng.gen_range(min..=max)
}

#[no_mangle]
pub extern "C" fn plat_random_float() -> f64 {
    use rand::Rng;
    rand::thread_rng().gen()
}
```

#### Process (ffi/process.rs)
```rust
#[no_mangle]
pub extern "C" fn plat_process_exit(code: i32) -> ! {
    std::process::exit(code)
}

#[no_mangle]
pub extern "C" fn plat_process_args() -> *mut RuntimeArray {
    // TODO: Return command-line arguments as List[String]
}
```

**Plat Signatures** (add to type checker):
```plat
// Time
fn time_now() -> Int64;
fn time_sleep(milliseconds: Int64);

// Environment
fn env_get(key: String) -> Option<String>;
fn env_set(key: String, value: String) -> Bool;

// Random
fn random_int(min: Int32, max: Int32) -> Int32;
fn random_float() -> Float64;

// Process
fn process_exit(code: Int32) -> !;  // never returns
fn process_args() -> List[String];
```

---

### Phase 6: More Stdlib Modules

#### std::fs (File System Utilities with pathlib-style Path)

**Inspiration**: Python's `pathlib.Path` - object-oriented file system path manipulation

```plat
mod std::fs;

/// Object-oriented path manipulation (like Python's pathlib.Path)
pub class Path {
  let path: String;

  // ============================================================================
  // Constructors and Conversions
  // ============================================================================

  pub fn new(path: String) -> Path {
    return Path.init(path = path);
  }

  pub fn from_parts(parts: List[String]) -> Path {
    // TODO: Join parts with platform separator
    return Path.init(path = "");
  }

  pub fn to_string() -> String {
    return self.path;
  }

  // ============================================================================
  // Path Manipulation (Pure - No I/O)
  // ============================================================================

  /// Join paths: Path("foo") / "bar" / "baz" -> Path("foo/bar/baz")
  pub fn join(other: String) -> Path {
    // Platform-aware path joining (/ on Unix, \ on Windows)
    // Handle edge cases: trailing slashes, absolute paths, etc.
    var result: String = self.path;

    if (!result.ends_with(substring = "/") && !result.ends_with(substring = "\\")) {
      result = result.concat(other = "/");  // TODO: Use platform separator
    }

    result = result.concat(other = other);
    return Path.init(path = result);
  }

  /// Get parent directory: Path("foo/bar/baz.txt").parent() -> Path("foo/bar")
  pub fn parent() -> Option<Path> {
    // TODO: Find last separator, return substring before it
    return Option::None;
  }

  /// Get filename: Path("foo/bar/baz.txt").name() -> "baz.txt"
  pub fn name() -> Option<String> {
    // TODO: Return part after last separator
    return Option::None;
  }

  /// Get filename without extension: Path("baz.txt").stem() -> "baz"
  pub fn stem() -> Option<String> {
    // TODO: Return filename without extension
    return Option::None;
  }

  /// Get file extension: Path("baz.txt").suffix() -> ".txt"
  pub fn suffix() -> Option<String> {
    // TODO: Return extension including dot
    return Option::None;
  }

  /// Get all extensions: Path("archive.tar.gz").suffixes() -> [".tar", ".gz"]
  pub fn suffixes() -> List[String] {
    // TODO: Return all extensions
    return [];
  }

  /// Replace filename: Path("foo/bar.txt").with_name("baz.txt") -> Path("foo/baz.txt")
  pub fn with_name(name: String) -> Path {
    // TODO: Replace filename, keep directory
    return self;
  }

  /// Replace extension: Path("foo.txt").with_suffix(".md") -> Path("foo.md")
  pub fn with_suffix(suffix: String) -> Path {
    // TODO: Replace extension
    return self;
  }

  /// Check if path is absolute: Path("/usr/bin").is_absolute() -> true
  pub fn is_absolute() -> Bool {
    // TODO: Check if starts with / (Unix) or C:\ (Windows)
    return self.path.starts_with(substring = "/");
  }

  /// Check if path is relative: Path("foo/bar").is_relative() -> true
  pub fn is_relative() -> Bool {
    return !self.is_absolute();
  }

  /// Split path into parts: Path("foo/bar/baz").parts() -> ["foo", "bar", "baz"]
  pub fn parts() -> List[String] {
    // TODO: Split on platform separator
    return self.path.split(separator = "/");
  }

  // ============================================================================
  // File System Queries (Read-Only I/O)
  // ============================================================================

  /// Check if path exists (file or directory)
  pub fn exists() -> Bool {
    return file_exists(path = self.path);
  }

  /// Check if path is a file
  pub fn is_file() -> Bool {
    return self.exists() && !file_is_dir(path = self.path);
  }

  /// Check if path is a directory
  pub fn is_dir() -> Bool {
    return file_is_dir(path = self.path);
  }

  /// Check if path is a symbolic link
  pub fn is_symlink() -> Bool {
    return file_is_symlink(path = self.path);
  }

  /// Get file size in bytes
  pub fn size() -> Result<Int64, String> {
    return file_size(path = self.path);
  }

  /// Get file permissions (Unix mode bits)
  pub fn permissions() -> Result<Int32, String> {
    return file_get_permissions(path = self.path);
  }

  /// Get last modified time (Unix timestamp)
  pub fn modified() -> Result<Int64, String> {
    return file_modified_time(path = self.path);
  }

  /// Get creation time (Unix timestamp)
  pub fn created() -> Result<Int64, String> {
    return file_created_time(path = self.path);
  }

  // ============================================================================
  // File Operations (Mutating I/O)
  // ============================================================================

  /// Read entire file as string
  pub fn read_text() -> Result<String, String> {
    let fd_result: Result<Int32, String> = file_open(path = self.path, mode = "r");

    let fd: Int32 = match fd_result {
      Result::Ok(descriptor: Int32) -> descriptor,
      Result::Err(err: String) -> {
        return Result::Err(field0 = err);
      }
    };

    let content: Result<String, String> = file_read(fd = fd, max_bytes = 10485760);  // 10MB max
    let close_result: Result<Bool, String> = file_close(fd = fd);

    return content;
  }

  /// Read entire file as binary (List[Int8])
  pub fn read_bytes() -> Result<List[Int8], String> {
    let fd_result: Result<Int32, String> = file_open(path = self.path, mode = "r");

    let fd: Int32 = match fd_result {
      Result::Ok(descriptor: Int32) -> descriptor,
      Result::Err(err: String) -> {
        return Result::Err(field0 = err);
      }
    };

    let content: Result<List[Int8], String> = file_read_binary(fd = fd, max_bytes = 10485760);
    let close_result: Result<Bool, String> = file_close(fd = fd);

    return content;
  }

  /// Write string to file (creates or overwrites)
  pub fn write_text(content: String) -> Result<Bool, String> {
    let fd_result: Result<Int32, String> = file_open(path = self.path, mode = "w");

    let fd: Int32 = match fd_result {
      Result::Ok(descriptor: Int32) -> descriptor,
      Result::Err(err: String) -> {
        return Result::Err(field0 = err);
      }
    };

    let write_result: Result<Int32, String> = file_write(fd = fd, data = content);
    let close_result: Result<Bool, String> = file_close(fd = fd);

    match write_result {
      Result::Ok(bytes: Int32) -> Result::Ok(field0 = true),
      Result::Err(err: String) -> Result::Err(field0 = err)
    }
  }

  /// Write binary data to file (creates or overwrites)
  pub fn write_bytes(content: List[Int8]) -> Result<Bool, String> {
    let fd_result: Result<Int32, String> = file_open(path = self.path, mode = "w");

    let fd: Int32 = match fd_result {
      Result::Ok(descriptor: Int32) -> descriptor,
      Result::Err(err: String) -> {
        return Result::Err(field0 = err);
      }
    };

    let write_result: Result<Int32, String> = file_write_binary(fd = fd, data = content);
    let close_result: Result<Bool, String> = file_close(fd = fd);

    match write_result {
      Result::Ok(bytes: Int32) -> Result::Ok(field0 = true),
      Result::Err(err: String) -> Result::Err(field0 = err)
    }
  }

  /// Append string to file
  pub fn append_text(content: String) -> Result<Bool, String> {
    let fd_result: Result<Int32, String> = file_open(path = self.path, mode = "a");

    let fd: Int32 = match fd_result {
      Result::Ok(descriptor: Int32) -> descriptor,
      Result::Err(err: String) -> {
        return Result::Err(field0 = err);
      }
    };

    let write_result: Result<Int32, String> = file_write(fd = fd, data = content);
    let close_result: Result<Bool, String> = file_close(fd = fd);

    match write_result {
      Result::Ok(bytes: Int32) -> Result::Ok(field0 = true),
      Result::Err(err: String) -> Result::Err(field0 = err)
    }
  }

  /// Delete file
  pub fn unlink() -> Result<Bool, String> {
    return file_delete(path = self.path);
  }

  /// Rename/move file
  pub fn rename(new_path: Path) -> Result<Bool, String> {
    return file_rename(old_path = self.path, new_path = new_path.path);
  }

  /// Change file permissions
  pub fn chmod(mode: Int32) -> Result<Bool, String> {
    return file_chmod(path = self.path, mode = mode);
  }

  // ============================================================================
  // Directory Operations
  // ============================================================================

  /// Create directory (parent must exist)
  pub fn mkdir() -> Result<Bool, String> {
    return dir_create(path = self.path);
  }

  /// Create directory and all parent directories
  pub fn mkdir_parents() -> Result<Bool, String> {
    return dir_create_all(path = self.path);
  }

  /// Remove empty directory
  pub fn rmdir() -> Result<Bool, String> {
    return dir_remove(path = self.path);
  }

  /// List directory contents (returns list of Path objects)
  pub fn list_dir() -> Result<List[Path], String> {
    let entries_result: Result<String, String> = dir_list(path = self.path);

    let entries_str: String = match entries_result {
      Result::Ok(content: String) -> content,
      Result::Err(err: String) -> {
        return Result::Err(field0 = err);
      }
    };

    let entries: List[String] = entries_str.split(separator = "\n");
    var paths: List[Path] = [];

    // TODO: Convert each entry to Path and add to list
    // Need for-each loop support first

    return Result::Ok(field0 = paths);
  }

  /// Iterate over directory entries (returns iterator-like object)
  pub fn iterdir() -> Result<List[Path], String> {
    // Alias for list_dir
    return self.list_dir();
  }

  /// Recursively list all files in directory tree
  pub fn glob(pattern: String) -> Result<List[Path], String> {
    // TODO: Implement glob pattern matching
    // Example: Path("src").glob("**/*.plat") finds all .plat files
    return Result::Err(field0 = "Not implemented");
  }

  /// Walk directory tree (like os.walk in Python)
  pub fn walk() -> Result<List[PathWalkEntry], String> {
    // TODO: Implement recursive directory traversal
    return Result::Err(field0 = "Not implemented");
  }

  // ============================================================================
  // Symlink Operations
  // ============================================================================

  /// Create symbolic link
  pub fn symlink_to(target: Path) -> Result<Bool, String> {
    return symlink_create(target = target.path, link = self.path);
  }

  /// Read symbolic link target
  pub fn readlink() -> Result<String, String> {
    return symlink_read(path = self.path);
  }

  /// Resolve symlink to actual path
  pub fn resolve() -> Result<Path, String> {
    if (self.is_symlink()) {
      let target_result: Result<String, String> = symlink_read(path = self.path);

      match target_result {
        Result::Ok(target: String) -> Result::Ok(field0 = Path.init(path = target)),
        Result::Err(err: String) -> Result::Err(field0 = err)
      }
    } else {
      return Result::Ok(field0 = self);
    }
  }
}

/// Helper class for directory tree walking
pub class PathWalkEntry {
  pub let path: Path;
  pub let is_dir: Bool;
  pub let is_file: Bool;
}

// ============================================================================
// Standalone Utility Functions
// ============================================================================

/// Recursively copy directory tree
pub fn copy_tree(src: Path, dst: Path) -> Result<Bool, String> {
  // TODO: Recursively copy all files and directories
  return Result::Err(field0 = "Not implemented");
}

/// Recursively delete directory tree
pub fn remove_tree(path: Path) -> Result<Bool, String> {
  // TODO: Recursively delete all files and directories
  return Result::Err(field0 = "Not implemented");
}

/// Get current working directory
pub fn cwd() -> Result<Path, String> {
  // TODO: Need primitive for getcwd()
  return Result::Err(field0 = "Not implemented");
}

/// Get home directory
pub fn home() -> Result<Path, String> {
  // TODO: Use env_get("HOME") or env_get("USERPROFILE") on Windows
  return Result::Err(field0 = "Not implemented");
}

/// Get temporary directory
pub fn temp_dir() -> Result<Path, String> {
  // TODO: Use env_get("TMPDIR") or platform-specific default
  return Result::Err(field0 = "Not implemented");
}
```

**Usage Examples**:

```plat
use std::fs;

fn main() -> Int32 {
  // Create Path object
  let config_path: fs::Path = fs::Path.new(path = "config.json");

  // Check if exists
  if (config_path.exists()) {
    print(value = "Config file found!");
  }

  // Read file
  let content_result: Result<String, String> = config_path.read_text();
  match content_result {
    Result::Ok(content: String) -> {
      print(value = "Config: ${content}");
    },
    Result::Err(err: String) -> {
      print(value = "Error reading file: ${err}");
    }
  };

  // Path manipulation (no I/O)
  let data_dir: fs::Path = fs::Path.new(path = "data");
  let log_file: fs::Path = data_dir.join(other = "app.log");
  print(value = "Log file path: ${log_file.to_string()}");  // "data/app.log"

  // Create directory with parents
  let deep_path: fs::Path = fs::Path.new(path = "foo/bar/baz");
  let mkdir_result: Result<Bool, String> = deep_path.mkdir_parents();

  // List directory
  let src_path: fs::Path = fs::Path.new(path = "src");
  let files_result: Result<List[fs::Path], String> = src_path.list_dir();

  match files_result {
    Result::Ok(files: List[fs::Path]) -> {
      // TODO: Iterate over files when for-each is ready
      print(value = "Found files in src/");
    },
    Result::Err(err: String) -> {
      print(value = "Error listing directory: ${err}");
    }
  };

  // Get file metadata
  let file_path: fs::Path = fs::Path.new(path = "data.txt");

  if (file_path.is_file()) {
    let size_result: Result<Int64, String> = file_path.size();
    match size_result {
      Result::Ok(bytes: Int64) -> {
        print(value = "File size: ${bytes} bytes");
      },
      Result::Err(err: String) -> {
        print(value = "Error: ${err}");
      }
    };
  }

  // Path parts
  let full_path: fs::Path = fs::Path.new(path = "/usr/local/bin/plat");
  let name_opt: Option<String> = full_path.name();

  match name_opt {
    Option::Some(name: String) -> {
      print(value = "Filename: ${name}");  // "plat"
    },
    Option::None -> {
      print(value = "No filename");
    }
  };

  // Copy file using Path API
  let src: fs::Path = fs::Path.new(path = "original.txt");
  let dst: fs::Path = fs::Path.new(path = "backup.txt");

  let content_result: Result<String, String> = src.read_text();
  match content_result {
    Result::Ok(content: String) -> {
      let write_result: Result<Bool, String> = dst.write_text(content = content);
      // File copied!
    },
    Result::Err(err: String) -> {
      print(value = "Copy failed: ${err}");
    }
  };

  return 0;
}
```

#### std::net (High-Level Networking)
```plat
mod std::net;

pub class TcpListener {
  let socket: Int32;

  pub fn bind(host: String, port: Int32) -> Result<TcpListener, String> {
    // TODO: Wrap tcp_listen
  }

  pub fn accept() -> Result<TcpStream, String> {
    // TODO: Wrap tcp_accept
  }
}

pub class TcpStream {
  let socket: Int32;

  pub fn connect(host: String, port: Int32) -> Result<TcpStream, String> {
    // TODO: Wrap tcp_connect
  }

  pub fn read(max_bytes: Int32) -> Result<String, String> {
    return tcp_read(socket = self.socket, max_bytes = max_bytes);
  }

  pub fn write(data: String) -> Result<Int32, String> {
    return tcp_write(socket = self.socket, data = data);
  }

  pub fn close() -> Result<Bool, String> {
    return tcp_close(socket = self.socket);
  }
}
```

#### std::http (HTTP Client/Server)
```plat
mod std::http;
use std::net;

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

pub fn get(url: String) -> Result<Response, String> {
  // TODO: Parse URL, connect, send HTTP GET request
}

pub fn post(url: String, body: String) -> Result<Response, String> {
  // TODO: Send HTTP POST request
}

pub class Server {
  let listener: net::TcpListener;

  pub fn listen(port: Int32, handler: fn(Request) -> Response) -> Result<Bool, String> {
    // TODO: Accept connections, parse HTTP, call handler
  }
}
```

#### std::collections (Additional Data Structures)
```plat
mod std::collections;

pub class Queue<T> {
  var items: List[T];

  pub fn enqueue(item: T) {
    self.items.push(value = item);
  }

  pub fn dequeue() -> Option<T> {
    // TODO: Remove first element (need List.remove_at method)
  }
}

pub class Stack<T> {
  var items: List[T];

  pub fn push(item: T) {
    self.items.push(value = item);
  }

  pub fn pop() -> Option<T> {
    return self.items.pop();
  }
}
```

#### std::math (Mathematical Functions)
```plat
mod std::math;

pub fn sqrt(x: Float64) -> Float64 {
  // TODO: Newton's method or call C math lib
}

pub fn pow(base: Float64, exp: Float64) -> Float64 {
  // TODO: Implement exponentiation
}

pub fn sin(x: Float64) -> Float64 {
  // TODO: Taylor series or call C math lib
}

pub fn cos(x: Float64) -> Float64 {
  // TODO: Taylor series or call C math lib
}

pub fn abs(x: Float64) -> Float64 {
  if (x < 0.0) {
    return -x;
  } else {
    return x;
  }
}

pub let PI: Float64 = 3.141592653589793;
pub let E: Float64 = 2.718281828459045;
```

#### std::time (Time/Date Utilities)
```plat
mod std::time;

pub class Duration {
  let milliseconds: Int64;

  pub fn from_seconds(seconds: Int64) -> Duration {
    return Duration.init(milliseconds = seconds * 1000);
  }

  pub fn from_minutes(minutes: Int64) -> Duration {
    return Duration.init(milliseconds = minutes * 60 * 1000);
  }

  pub fn to_seconds() -> Int64 {
    return self.milliseconds / 1000;
  }
}

pub fn now() -> Int64 {
  return time_now();
}

pub fn sleep(duration: Duration) {
  time_sleep(milliseconds = duration.milliseconds);
}
```

#### std::string (Extended String Utilities)
```plat
mod std::string;

pub fn join(strings: List[String], separator: String) -> String {
  // TODO: Concatenate with separator between elements
}

pub fn repeat(s: String, count: Int32) -> String {
  // TODO: Repeat string N times
}

pub fn reverse(s: String) -> String {
  // TODO: Reverse string character-by-character
}

pub fn is_numeric(s: String) -> Bool {
  // TODO: Check if all characters are digits
}

pub fn is_alpha(s: String) -> Bool {
  // TODO: Check if all characters are letters
}
```

---

## Example User Code

```plat
use std::io;
use std::json;
use std::http;
use std::time;

fn main() -> Int32 {
  // Read configuration file
  let config_result: Result<String, String> = io::read_file(path = "config.json");

  let config_str: String = match config_result {
    Result::Ok(content: String) -> content,
    Result::Err(err: String) -> {
      print(value = "Failed to read config: ${err}");
      return 1;
    }
  };

  // Parse JSON configuration
  let json_result: Result<json::JsonValue, String> = json::parse(input = config_str);

  let config: json::JsonValue = match json_result {
    Result::Ok(value: json::JsonValue) -> value,
    Result::Err(err: String) -> {
      print(value = "Failed to parse JSON: ${err}");
      return 1;
    }
  };

  // Make HTTP request
  let start_time: Int64 = time::now();
  let response_result: Result<http::Response, String> = http::get(url = "https://api.example.com/data");
  let end_time: Int64 = time::now();

  match response_result {
    Result::Ok(response: http::Response) -> {
      print(value = "Status: ${response.status}");
      print(value = "Body: ${response.body}");
      print(value = "Time: ${end_time - start_time}ms");
    },
    Result::Err(err: String) -> {
      print(value = "HTTP request failed: ${err}");
      return 1;
    }
  };

  return 0;
}
```

---

## Custom Error Types

### Problem with String Errors

Currently, the stdlib uses `Result<T, String>` everywhere:

```plat
pub fn read_file(path: String) -> Result<String, String> {
  // Error is just a string - no structure!
}
```

**Issues**:
- No way to programmatically distinguish error types
- Can't pattern match on error categories
- No context (error code, location, etc.)
- Poor error composition

### Solution: Structured Error Enums

Define custom error types per module using enums:

#### std::io Error Types

```plat
mod std::io;

/// I/O error types
pub enum IoError {
  /// File or directory not found
  NotFound(String),  // path

  /// Permission denied
  PermissionDenied(String),  // path

  /// File or directory already exists
  AlreadyExists(String),  // path

  /// Invalid input (e.g., invalid mode string)
  InvalidInput(String),  // message

  /// Unexpected end of file
  UnexpectedEof,

  /// Generic I/O error with message
  Other(String)
}

impl IoError {
  /// Convert error to human-readable message
  pub fn to_string() -> String {
    match self {
      IoError::NotFound(path: String) -> {
        return "File not found: ${path}";
      },
      IoError::PermissionDenied(path: String) -> {
        return "Permission denied: ${path}";
      },
      IoError::AlreadyExists(path: String) -> {
        return "File already exists: ${path}";
      },
      IoError::InvalidInput(msg: String) -> {
        return "Invalid input: ${msg}";
      },
      IoError::UnexpectedEof -> {
        return "Unexpected end of file";
      },
      IoError::Other(msg: String) -> {
        return "I/O error: ${msg}";
      }
    }
  }

  /// Get error code for programmatic handling
  pub fn code() -> Int32 {
    match self {
      IoError::NotFound(_) -> 2,  // ENOENT
      IoError::PermissionDenied(_) -> 13,  // EACCES
      IoError::AlreadyExists(_) -> 17,  // EEXIST
      IoError::InvalidInput(_) -> 22,  // EINVAL
      IoError::UnexpectedEof -> 0,
      IoError::Other(_) -> 1
    }
  }
}

/// Type alias for I/O results
pub type IoResult<T> = Result<T, IoError>;

/// Read file with structured error
pub fn read_file(path: String) -> IoResult<String> {
  let fd_result: Result<Int32, String> = file_open(path = path, mode = "r");

  let fd: Int32 = match fd_result {
    Result::Ok(descriptor: Int32) -> descriptor,
    Result::Err(err: String) -> {
      // Parse error string to determine error type
      if (err.contains(substring = "not found") || err.contains(substring = "No such file")) {
        return Result::Err(field0 = IoError::NotFound(field0 = path));
      } else if (err.contains(substring = "Permission denied")) {
        return Result::Err(field0 = IoError::PermissionDenied(field0 = path));
      } else {
        return Result::Err(field0 = IoError::Other(field0 = err));
      }
    }
  };

  let content: Result<String, String> = file_read(fd = fd, max_bytes = 1048576);
  let close_result: Result<Bool, String> = file_close(fd = fd);

  match content {
    Result::Ok(data: String) -> Result::Ok(field0 = data),
    Result::Err(err: String) -> Result::Err(field0 = IoError::Other(field0 = err))
  }
}
```

#### std::json Error Types

```plat
mod std::json;

/// JSON parsing errors
pub enum JsonError {
  /// Unexpected character at position
  UnexpectedChar(Int32, String),  // position, character

  /// Unexpected end of input
  UnexpectedEof,

  /// Invalid number format
  InvalidNumber(String),  // value

  /// Invalid escape sequence
  InvalidEscape(String),  // sequence

  /// Expected token not found
  ExpectedToken(String, String),  // expected, found

  /// Trailing characters after valid JSON
  TrailingChars(Int32),  // position
}

impl JsonError {
  pub fn to_string() -> String {
    match self {
      JsonError::UnexpectedChar(pos: Int32, ch: String) -> {
        return "Unexpected character '${ch}' at position ${pos}";
      },
      JsonError::UnexpectedEof -> {
        return "Unexpected end of input";
      },
      JsonError::InvalidNumber(val: String) -> {
        return "Invalid number: ${val}";
      },
      JsonError::InvalidEscape(seq: String) -> {
        return "Invalid escape sequence: ${seq}";
      },
      JsonError::ExpectedToken(expected: String, found: String) -> {
        return "Expected ${expected}, found ${found}";
      },
      JsonError::TrailingChars(pos: Int32) -> {
        return "Trailing characters after position ${pos}";
      }
    }
  }
}

pub type JsonResult<T> = Result<T, JsonError>;

pub fn parse(input: String) -> JsonResult<JsonValue> {
  // Parser returns structured errors
}
```

#### std::http Error Types

```plat
mod std::http;
use std::io;

/// HTTP error types
pub enum HttpError {
  /// Network error (wraps IoError)
  Network(io::IoError),

  /// Invalid URL format
  InvalidUrl(String),  // url

  /// HTTP status error (4xx, 5xx)
  StatusError(Int32, String),  // status code, message

  /// Timeout
  Timeout,

  /// Invalid response
  InvalidResponse(String),  // message
}

impl HttpError {
  pub fn to_string() -> String {
    match self {
      HttpError::Network(err: io::IoError) -> {
        return "Network error: ${err.to_string()}";
      },
      HttpError::InvalidUrl(url: String) -> {
        return "Invalid URL: ${url}";
      },
      HttpError::StatusError(code: Int32, msg: String) -> {
        return "HTTP ${code}: ${msg}";
      },
      HttpError::Timeout -> {
        return "Request timeout";
      },
      HttpError::InvalidResponse(msg: String) -> {
        return "Invalid response: ${msg}";
      }
    }
  }
}

pub type HttpResult<T> = Result<T, HttpError>;
```

#### std::fs Error Types

```plat
mod std::fs;
use std::io;

/// File system error (wraps IoError with path context)
pub enum FsError {
  /// I/O error with path context
  Io(io::IoError, String),  // error, path

  /// Invalid path format
  InvalidPath(String),  // path

  /// Path traversal outside allowed directory
  PathTraversal(String),  // path
}

impl FsError {
  pub fn to_string() -> String {
    match self {
      FsError::Io(err: io::IoError, path: String) -> {
        return "${err.to_string()} (path: ${path})";
      },
      FsError::InvalidPath(path: String) -> {
        return "Invalid path: ${path}";
      },
      FsError::PathTraversal(path: String) -> {
        return "Path traversal detected: ${path}";
      }
    }
  }
}

pub type FsResult<T> = Result<T, FsError>;
```

### Usage Examples

**Pattern Matching on Error Types**:

```plat
use std::io;

fn main() -> Int32 {
  let result: io::IoResult<String> = io::read_file(path = "config.json");

  match result {
    Result::Ok(content: String) -> {
      print(value = "Read ${content.length()} bytes");
    },
    Result::Err(err: io::IoError) -> {
      // Pattern match on specific error types!
      match err {
        io::IoError::NotFound(path: String) -> {
          print(value = "Creating default config at ${path}");
          // Create default config
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

**Error Context Propagation**:

```plat
use std::fs;
use std::io;
use std::json;

fn load_config(path: String) -> Result<json::JsonValue, String> {
  // Read file - returns IoResult
  let content_result: io::IoResult<String> = io::read_file(path = path);

  let content: String = match content_result {
    Result::Ok(data: String) -> data,
    Result::Err(err: io::IoError) -> {
      // Convert IoError to String with context
      return Result::Err(field0 = "Failed to read config: ${err.to_string()}");
    }
  };

  // Parse JSON - returns JsonResult
  let json_result: json::JsonResult<json::JsonValue> = json::parse(input = content);

  match json_result {
    Result::Ok(value: json::JsonValue) -> Result::Ok(field0 = value),
    Result::Err(err: json::JsonError) -> {
      // Convert JsonError to String with context
      Result::Err(field0 = "Failed to parse config: ${err.to_string()}")
    }
  }
}
```

### Benefits

1. **Structured Errors**: Errors are data, not just strings
2. **Pattern Matching**: Can handle specific error cases differently
3. **Error Codes**: Programmatic access to error codes
4. **Context**: Errors carry relevant information (path, position, etc.)
5. **Composability**: Errors can wrap other errors (e.g., `HttpError::Network(IoError)`)
6. **Type Safety**: Compiler ensures all error cases are handled

### Migration Path

**Phase 1**: Start with String errors (current approach)
**Phase 2**: Introduce error enums in new stdlib modules
**Phase 3**: Migrate existing modules to use error enums
**Phase 4**: Add `?` operator support for automatic error conversion

---

## Testing Strategy

### Comprehensive Testing Requirements

**Coverage Goals**:
- **Unit Tests**: >90% code coverage for all stdlib modules
- **Integration Tests**: End-to-end workflows across multiple modules
- **Edge Cases**: Boundary conditions, empty inputs, large inputs
- **Error Paths**: All error types must have test coverage
- **Cross-Platform**: Tests run on Linux, macOS, Windows
- **Performance**: Benchmarks for hot paths

### Unit Tests (Per Module)

Each stdlib module has a `test` block with comprehensive coverage:

```plat
mod std::json;

// ... implementation ...

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

  fn test_parse_bool_true() {
    let result: JsonResult<JsonValue> = parse(input = "true");
    // ... assertions ...
  }

  fn test_parse_bool_false() {
    let result: JsonResult<JsonValue> = parse(input = "false");
    // ... assertions ...
  }

  fn test_parse_number_integer() {
    let result: JsonResult<JsonValue> = parse(input = "42");
    // ... assertions ...
  }

  fn test_parse_number_float() {
    let result: JsonResult<JsonValue> = parse(input = "3.14159");
    // ... assertions ...
  }

  fn test_parse_number_negative() {
    let result: JsonResult<JsonValue> = parse(input = "-273.15");
    // ... assertions ...
  }

  fn test_parse_string_simple() {
    let result: JsonResult<JsonValue> = parse(input = "\"hello\"");
    // ... assertions ...
  }

  fn test_parse_string_with_escapes() {
    let result: JsonResult<JsonValue> = parse(input = "\"hello\\nworld\\t!\"");
    // ... assertions ...
  }

  fn test_parse_array_empty() {
    let result: JsonResult<JsonValue> = parse(input = "[]");
    // ... assertions ...
  }

  fn test_parse_array_mixed() {
    let result: JsonResult<JsonValue> = parse(input = "[1, \"two\", true, null]");
    // ... assertions ...
  }

  fn test_parse_object_empty() {
    let result: JsonResult<JsonValue> = parse(input = "{}");
    // ... assertions ...
  }

  fn test_parse_object_simple() {
    let result: JsonResult<JsonValue> = parse(input = "{\"name\": \"Alice\", \"age\": 30}");
    // ... assertions ...
  }

  fn test_parse_object_nested() {
    let input: String = "{\"person\": {\"name\": \"Bob\", \"address\": {\"city\": \"NYC\"}}}";
    let result: JsonResult<JsonValue> = parse(input = input);
    // ... assertions ...
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

  fn test_parse_whitespace_only() {
    let result: JsonResult<JsonValue> = parse(input = "   \n\t  ");
    // Should fail with UnexpectedEof
  }

  fn test_parse_large_number() {
    let result: JsonResult<JsonValue> = parse(input = "999999999999999999");
    // ... assertions ...
  }

  fn test_parse_deeply_nested() {
    let input: String = "[[[[[[[[[[\"deep\"]]]]]]]]]]";
    let result: JsonResult<JsonValue> = parse(input = input);
    // ... assertions ...
  }

  fn test_parse_unicode() {
    let result: JsonResult<JsonValue> = parse(input = "\"Hello 世界 🌍\"");
    // ... assertions ...
  }

  // ============================================================================
  // Error Cases (Test Error Types)
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

  fn test_parse_trailing_comma() {
    let result: JsonResult<JsonValue> = parse(input = "[1, 2, 3,]");
    // Should fail with appropriate error
  }

  fn test_parse_missing_quote() {
    let result: JsonResult<JsonValue> = parse(input = "{\"name: \"Alice\"}");
    // Should fail with InvalidEscape or UnexpectedChar
  }

  fn test_parse_invalid_escape() {
    let result: JsonResult<JsonValue> = parse(input = "\"hello\\x world\"");
    match result {
      Result::Err(err: JsonError) -> {
        match err {
          JsonError::InvalidEscape(seq: String) -> assert(condition = true),
          _ -> assert(condition = false, message = "Expected InvalidEscape error")
        }
      },
      Result::Ok(_) -> assert(condition = false)
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
  // Round-Trip Tests (Stringify → Parse)
  // ============================================================================

  fn test_roundtrip_null() {
    let original: JsonValue = JsonValue::Null;
    let json_str: String = stringify(value = original);
    let parsed_result: JsonResult<JsonValue> = parse(input = json_str);
    // Assert parsed equals original
  }

  fn test_roundtrip_object() {
    // Create object, stringify, parse back, verify equality
  }

  // ============================================================================
  // Performance Tests (should be in bench block)
  // ============================================================================
}

test json_stringify {
  fn test_stringify_null() {
    let value: JsonValue = JsonValue::Null;
    let result: String = stringify(value = value);
    assert(condition = result == "null", message = "Expected 'null'");
  }

  fn test_stringify_object() {
    // ... more stringify tests ...
  }
}
```

**Test Organization Best Practices**:
1. Group tests by category (happy path, edge cases, errors)
2. Test every error variant explicitly
3. Use descriptive test names (`test_parse_X_Y`)
4. Include comments explaining what's being tested
5. Test both success and failure paths

Run tests:
```bash
plat test stdlib/std/json.plat          # Single module
plat test stdlib/std/json.plat -f parse # Filter tests
plat test stdlib/                       # All stdlib modules
```

### Integration Tests

Create `stdlib/tests/` directory with end-to-end tests:

```plat
// stdlib/tests/http_server_test.plat
use std::http;
use std::net;
use std::time;

test http_integration {
  fn test_http_get_request() {
    // Start a simple HTTP server
    // Make a GET request
    // Verify response
    assert(condition = true);
  }

  fn test_http_post_with_json() {
    // POST JSON data
    // Verify server receives it correctly
    assert(condition = true);
  }

  fn test_http_timeout() {
    // Test timeout handling
    assert(condition = true);
  }
}
```

```plat
// stdlib/tests/file_operations_test.plat
use std::fs;
use std::io;

test file_integration {
  fn test_create_read_delete_file() {
    let path: fs::Path = fs::Path.new(path = "test_file.txt");

    // Write file
    let write_result: Result<Bool, String> = path.write_text(content = "Hello!");
    assert(condition = write_result.is_ok());

    // Read file
    let read_result: Result<String, String> = path.read_text();
    match read_result {
      Result::Ok(content: String) -> {
        assert(condition = content == "Hello!", message = "Content mismatch");
      },
      Result::Err(err: String) -> {
        assert(condition = false, message = err);
      }
    };

    // Delete file
    let delete_result: Result<Bool, String> = path.unlink();
    assert(condition = delete_result.is_ok());
  }

  fn test_directory_operations() {
    let dir: fs::Path = fs::Path.new(path = "test_dir");

    // Create directory
    let mkdir_result: Result<Bool, String> = dir.mkdir();
    assert(condition = mkdir_result.is_ok());

    // List directory
    let list_result: Result<List[fs::Path], String> = dir.list_dir();
    // ... assertions ...

    // Remove directory
    let rmdir_result: Result<Bool, String> = dir.rmdir();
    assert(condition = rmdir_result.is_ok());
  }
}
```

### Property-Based Testing (Future)

Once the test framework supports it:

```plat
test json_properties {
  fn property_roundtrip() {
    // For any valid JSON value, parse(stringify(x)) == x
    // Generate random JSON values and test
  }

  fn property_parse_never_panics() {
    // For any string input, parse should return Result (not panic)
    // Fuzz testing
  }
}
```

### Benchmarking

Each stdlib module should have a `bench` block for performance testing:

```plat
mod std::json;

// ... implementation ...

bench json_performance {
  // Helper to create test data
  fn create_large_object() -> JsonValue {
    // Create a large JSON object for benchmarking
    return JsonValue::Null;  // TODO: Real implementation
  }

  fn bench_parse_small() {
    let result: JsonResult<JsonValue> = parse(input = "{\"key\": \"value\"}");
  }

  fn bench_parse_large() {
    let large_json: String = "...";  // Large JSON string
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

Run benchmarks:
```bash
plat bench stdlib/std/json.plat    # Single module benchmarks
plat bench stdlib/                 # All stdlib benchmarks
```

### CI Pipeline

Complete CI pipeline with testing, benchmarking, and coverage:

```yaml
# .github/workflows/stdlib-tests.yml
name: Stdlib Tests

on: [push, pull_request]

jobs:
  test-linux:
    name: Test on Linux
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2

      - name: Cache Rust dependencies
        uses: actions/cache@v2
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            target
          key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}

      - name: Build compiler
        run: cargo build --release

      - name: Test std::io
        run: ./target/release/plat test stdlib/std/io.plat

      - name: Test std::json
        run: ./target/release/plat test stdlib/std/json.plat

      - name: Test std::fs
        run: ./target/release/plat test stdlib/std/fs.plat

      - name: Test std::net
        run: ./target/release/plat test stdlib/std/net.plat

      - name: Test std::http
        run: ./target/release/plat test stdlib/std/http.plat

      - name: Test all stdlib modules
        run: ./target/release/plat test stdlib/

      - name: Run integration tests
        run: ./target/release/plat test stdlib/tests/

      - name: Generate test report
        run: |
          echo "Test Summary:" >> $GITHUB_STEP_SUMMARY
          echo "- All stdlib tests passed ✅" >> $GITHUB_STEP_SUMMARY

  test-macos:
    name: Test on macOS
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v2
      - name: Build compiler
        run: cargo build --release
      - name: Test all stdlib
        run: ./target/release/plat test stdlib/

  test-windows:
    name: Test on Windows
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v2
      - name: Build compiler
        run: cargo build --release
      - name: Test all stdlib
        run: ./target/release/plat test stdlib/

  benchmark:
    name: Performance Benchmarks
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - name: Build compiler
        run: cargo build --release

      - name: Benchmark std::json
        run: ./target/release/plat bench stdlib/std/json.plat

      - name: Benchmark std::io
        run: ./target/release/plat bench stdlib/std/io.plat

      - name: Store benchmark results
        uses: benchmark-action/github-action-benchmark@v1
        with:
          tool: 'customBenchmark'
          output-file-path: bench_results.json

  coverage:
    name: Code Coverage
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2

      - name: Build compiler with coverage
        run: cargo build --release

      - name: Run tests with coverage tracking
        run: |
          # TODO: Implement coverage tracking for Plat code
          ./target/release/plat test stdlib/

      - name: Upload coverage to Codecov
        uses: codecov/codecov-action@v2
        with:
          files: ./coverage.json

  lint:
    name: Code Quality
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2

      - name: Build compiler
        run: cargo build --release

      - name: Format check
        run: |
          # Check all stdlib files are formatted
          for file in stdlib/std/*.plat; do
            ./target/release/plat fmt "$file"
            if git diff --exit-code "$file"; then
              echo "✅ $file is formatted"
            else
              echo "❌ $file is not formatted"
              exit 1
            fi
          done

      - name: Naming convention check
        run: |
          # Compiler enforces naming conventions at compile time
          ./target/release/plat build stdlib/
```

### Test Coverage Requirements

**Per Module Coverage**:
- **std::io**: >95% (critical I/O operations)
- **std::json**: >90% (parser must handle all edge cases)
- **std::fs**: >90% (file system operations)
- **std::net**: >85% (network I/O)
- **std::http**: >85% (HTTP protocol handling)
- **std::collections**: >90% (data structures)
- **std::math**: >95% (mathematical functions)

**Coverage Reports**:
```bash
# Generate coverage report
plat test --coverage stdlib/

# Output:
# std::io:      97.3% (123/127 lines)
# std::json:    94.1% (456/485 lines)
# std::fs:      91.8% (234/255 lines)
# ...
# Total:        93.2% (1234/1324 lines)
```

### Test Data and Fixtures

Create `stdlib/tests/fixtures/` for test data:

```
stdlib/
├── tests/
│   ├── fixtures/
│   │   ├── valid_json/
│   │   │   ├── simple.json
│   │   │   ├── nested.json
│   │   │   └── large.json
│   │   ├── invalid_json/
│   │   │   ├── trailing_comma.json
│   │   │   ├── missing_quote.json
│   │   │   └── invalid_escape.json
│   │   └── test_files/
│   │       ├── sample.txt
│   │       └── binary.dat
│   ├── http_server_test.plat
│   └── file_operations_test.plat
└── std/
    ├── io.plat
    ├── json.plat
    └── ...
```

Load fixtures in tests:

```plat
use std::fs;
use std::json;

test json_fixtures {
  fn test_valid_json_files() {
    let fixtures_dir: fs::Path = fs::Path.new(path = "stdlib/tests/fixtures/valid_json");
    let files_result: Result<List[fs::Path], String> = fixtures_dir.list_dir();

    match files_result {
      Result::Ok(files: List[fs::Path]) -> {
        // Test each fixture file
        // TODO: Iterate when for-each is ready
      },
      Result::Err(err: String) -> {
        assert(condition = false, message = err);
      }
    };
  }
}
```

---

## Performance Considerations

### Selective Linking

**Problem**: Don't want to link entire stdlib if user only imports one module

**Solution**: Incremental compilation and selective linking
- Each stdlib module compiles to separate object file
- Linker only includes object files for imported modules
- Dead code elimination removes unused functions

**Example**:
```plat
use std::json;  // Only links std/json.o, not std/http.o
```

### Caching Strategy

**First Compilation**:
```
stdlib/std/json.plat → parse → HIR → cache to target/stdlib-cache/std-json.hir
                                  ↓
                              codegen → std-json.o
```

**Subsequent Compilations**:
```
Check cache for std-json.hir → if fresh, load from cache
                            ↓
                         Skip parsing & type checking
                            ↓
                         codegen → std-json.o
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

## Migration Path

### Phase 1: MVP (Minimal Viable Product)
- Basic infrastructure (stdlib path resolution)
- `std::io` (file reading/writing)
- `std::json` (pure Plat implementation)

### Phase 2: Networking
- `std::net` (high-level TCP wrappers)
- `std::http` (HTTP client)

### Phase 3: Utilities
- `std::fs` (file system utilities)
- `std::collections` (Queue, Stack)
- `std::time` (Duration, sleep)
- `std::math` (mathematical functions)

### Phase 4: Advanced
- `std::crypto` (hashing, encryption)
- `std::regex` (regular expressions)
- `std::async` (async/await on top of green threads)

---

## Open Questions

1. **Versioning**: How do we version stdlib separately from compiler?
   - Option A: Stdlib version tied to compiler version
   - Option B: Separate semantic versioning for stdlib
   - **Recommendation**: Option A for now (simplicity)

2. **Breaking Changes**: How do we handle breaking changes in stdlib API?
   - Follow semantic versioning
   - Major version bumps for breaking changes
   - Deprecation warnings before removal

3. **Third-Party Libraries**: Should we support external package manager?
   - Future: `plat add github.com/user/repo`
   - For now: focus on stdlib only

4. **Documentation**: How do we document stdlib API?
   - Generate docs from source code comments
   - Host on docs.platlang.org/std
   - CLI: `plat doc std::json`

---

## Success Metrics

- ✅ **Usability**: Users can `use std::*` and it just works
- ✅ **Performance**: Stdlib caching speeds up compilation by 10x
- ✅ **Completeness**: Cover 80% of common use cases (I/O, JSON, HTTP)
- ✅ **Dogfooding**: Stdlib written 100% in Plat (no Rust escape hatches)
- ✅ **Testing**: >90% test coverage for all stdlib modules
- ✅ **Documentation**: Every public function/class has doc comments

---

## Cross-Module Function Call Codegen Fix (In Progress)

**Issue**: Cross-module function calls (e.g., `std::test::add()`) fail because codegen incorrectly handles function signatures.

### Problem Analysis

There are two distinct issues:

1. **✅ FIXED: Implicit Self Parameter Bug**
   - **Root Cause**: Codegen used `name.contains("::")` to detect methods needing implicit `self` parameter
   - **Impact**: Cross-module functions like `std::test::hello` were treated as enum methods like `Option::Some`
   - **Solution**: Added `method_names: HashSet<String>` to track actual enum/class methods
   - **Status**: Fixed in commit XXX

2. **✅ FIXED: Function Signature Resolution**
   - **Root Cause**: When calling cross-module functions not in the current module's function map, codegen dynamically creates signatures
   - **Old Behavior**: Assumed all parameters and returns are `i64`
   - **Impact**: Functions returning `i32` (or other types) caused Cranelift type mismatches
   - **Example**: `std::test::add` returns `i32` but was declared as returning `i64`
   - **Solution**: Threaded global symbol table through codegen to look up actual signatures
   - **Implementation**: Added symbol_table parameter to all 162+ expression/statement helper call sites
   - **Status**: Fixed in commit a819495

### Current Status

**What Works**:
- ✅ Cross-module imports: `use std::test;` works
- ✅ Enum/class methods correctly get implicit `self` parameter
- ✅ Cross-module functions no longer get incorrect `self` parameter
- ✅ Cross-module function signatures looked up from symbol table
- ✅ Functions with Int32/Int8/Int16/Float32 return types now generate correct Cranelift IR

**What's Working**:
- ✅ `std::test::add(x = 5, y = 10)` generates correct signature (`i32, i32 -> i32`)
- ✅ Variable type inference for all numeric types (Int8, Int16, Int32, Int64, Float32, Float64)
- ✅ String interpolation correctly handles all numeric types with automatic conversion

### Implementation Plan

**Phase 1** (✅ Complete):
- Add `method_names: HashSet<String>` to `CodeGenerator`
- Track enum/class method names during declaration
- Use set instead of string matching for method detection

**Phase 2** (✅ Complete):
1. ✅ Added `symbol_table: Option<plat_hir::ModuleSymbolTable>` to `CodeGenerator`
2. ✅ Added `with_symbol_table()` method to set it
3. ✅ Pass symbol table in CLI when creating `CodeGenerator`
4. ✅ Thread symbol table through to `generate_expression_helper` and all helper functions
5. ✅ Look up function signatures from symbol table for cross-module calls
6. ✅ Convert HIR types to Cranelift types correctly with `hir_type_to_cranelift()`

**Phase 2 Implementation Details**:
- Updated `generate_expression_helper` signature to accept `Option<&plat_hir::ModuleSymbolTable>`
- Updated 162+ call sites across `generate_expression_helper`, `generate_literal`, `generate_statement_helper`, etc.
- Cross-module function calls now check symbol table first, fall back to i64 assumption if not found
- Used automated sed/perl scripts to update all call sites consistently

### ~~Workaround~~ (No longer needed - Phase 2 complete!)

~~Until Phase 2 is complete, stdlib functions should:~~
- ~~Return `Int64`, `Float64`, `String`, or object types (all i64-compatible)~~
- ~~Avoid returning `Int32`, `Int8`, `Int16`, `Float32` which require exact type matching~~

**UPDATE**: Phase 2 is now complete! Stdlib functions can use any return type (`Int32`, `Int8`, `Float32`, etc.) and signatures will be correctly resolved from the symbol table.

**UPDATE 2**: String interpolation type conversion issue fixed! All numeric types (Int8, Int16, Int32, Int64, Float32, Float64) now correctly convert to strings in print statements and string interpolation. The issue was that Int8/Int16 values weren't being sign-extended to Int32 before calling the i32_to_string conversion function.

---

## Next Steps

1. ✅ ~~**Create Directory Structure**: `mkdir -p stdlib/std`~~ (Completed)
2. ✅ ~~**Implement Phase 1**: Module resolution for `std::*`~~ (Completed)
3. ✅ ~~**Fix Cross-Module Codegen**: Phase 1 & Phase 2 complete~~ (Completed - commit a819495)
4. **Write std::io**: First real stdlib module (Phase 3) - ready to implement with full type support!
5. **Write std::json**: Showcase pure Plat implementation (Phase 4)
6. **Add Caching**: Optimize compilation performance (Module caching phase)
7. **Expand**: Add more modules based on user feedback

---

**Status**: ✅ Phase 1 Complete - All Codegen Issues Resolved!
**Start Date**: 2025-01-XX
**Last Updated**: 2025-10-06
**Current Phase**: Phase 1 (Complete) → Codegen Fix (Complete) → String Interpolation Fix (Complete) → Ready for Phase 3 (std::io)
**Maintainer**: Plat Core Team

## Progress Summary

- ✅ **Phase 1**: Infrastructure complete - stdlib modules can be imported with `use std::*`
- ✅ **Codegen Fix Phase 1**: Method detection fixed - no more incorrect self parameters (commit 1ec9636)
- ✅ **Codegen Fix Phase 2**: Signature resolution complete - symbol table threaded through codegen (commit a819495)
- ⏸️ **Module Caching Phase**: Not started - optimization for future
- 📝 **Phase 3 (std::io)**: Ready to implement - no blockers, full type support available!
- 📝 **Phase 4 (std::json)**: Ready to implement - no blockers

**Blockers**: ~~Cross-module function calls with non-i64 return types~~ **RESOLVED!** ~~Variable type inference for Int8/Int16~~ **RESOLVED!** No current blockers for stdlib development.
