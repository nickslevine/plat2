# Plat Standard Library Plan

> **📝 IMPORTANT**: After completing any chunk of work on the stdlib, always:
> 1. Update this plan with progress details
> 2. Commit the changes to git
> 3. Keep the status section current

## Overview

The Plat standard library (`std`) is written entirely in Plat, providing high-level abstractions over low-level Rust FFI primitives. Users import stdlib modules with `use std::module_name;`.

**Directory Structure:**
```
plat2/
├── stdlib/
│   └── std/
│       ├── json.plat         # JSON parsing/serialization
│       ├── io.plat           # High-level I/O wrappers
│       ├── test.plat         # Test module
│       └── [future modules]
└── target/
    └── stdlib-cache/         # Cached compiled stdlib modules
```

---

## Module Resolution Strategy

### Special `std::` Handling

1. **Reserved Namespace**: The `std::` prefix is reserved for standard library
2. **Search Path**: User modules: current directory; stdlib: `stdlib/` directory
3. **Path Mapping**: `use std::json;` → `stdlib/std/json.plat`
4. **Validation**: User code cannot use `std::` prefix (compile error)

### Compilation Flow
```
User Code (main.plat)
  ↓ use std::json;
  ↓ Module Resolver discovers stdlib/std/json.plat
  ↓ Check cache (target/stdlib-cache/std-json.o)
  ↓ If cached and fresh: use cached object file
  ↓ If not cached: compile and cache
  ↓ Type check user code with stdlib symbols
  ↓ Codegen and link (only imported modules)
  ↓ Final binary
```

---

## Current Primitive Layer (Already Implemented)

The following are implemented as Rust FFI in `plat-runtime/src/ffi/`:

- **Networking** (net.rs): `tcp_listen`, `tcp_accept`, `tcp_connect`, `tcp_read`, `tcp_write`, `tcp_close`
- **File System** (fs.rs): `file_open`, `file_read`, `file_write`, `file_close`, `file_read_binary`, `file_write_binary`, `file_exists`, `file_size`, `file_is_dir`, `file_delete`, `file_rename`, `dir_create`, `dir_create_all`, `dir_remove`, `dir_list`, symlink operations, permissions, seeking
- **String Operations** (string.rs): 17 built-in methods including `substring`, `char_at`, `split`, `parse_int`, `parse_float`, etc.
- **Collections**: List[T], Dict[K,V], Set[T] with methods (`push`, `pop`, `insert`, `length`, etc.)
- **Concurrency** (green_runtime/): Green thread runtime with `concurrent {}` blocks, `spawn`, `.await()`

---

## Implementation Phases

### Phase 1: Infrastructure (Core) ✅ COMPLETED

**Goal**: Make `use std::*` work

**What Was Implemented**:
- ✅ Created `stdlib/std/` directory structure
- ✅ ModuleResolver already had `stdlib_dir` field and `std::` handling
- ✅ Parser fix: `consume_module_name()` accepts keywords in module paths (e.g., `std::test`)
- ✅ Test module verification: `use std::test;` compiles and runs

**Success**: Module system discovers stdlib modules from `stdlib/` directory

---

### Phase 2: Module Caching (Performance) ✅ COMPLETED

**Goal**: Cache compiled stdlib modules for fast rebuilds

**Status**: Completed on 2025-10-07

**What Was Implemented**:
- ✅ Object file caching (`.o` files instead of HIR)
- ✅ `StdlibCache` struct in `plat-modules/src/lib.rs`
- ✅ Integrated into CLI compilation flow
- ✅ Cache invalidation based on file modification timestamps
- ✅ Cache location: `target/stdlib-cache/`

**Cache Behavior**:
- First build: compiles stdlib, caches object files
- Subsequent builds: instant cache hit, no recompilation
- Modified stdlib: cache invalidated, module recompiles

**Performance**: Significant speedup for projects using multiple stdlib modules

---

### Phase 3: std::io (First Stdlib Module) ✅ COMPLETED

**Status**: Completed on 2025-10-07

**Module**: `stdlib/std/io.plat`

**What Was Implemented**:
1. ✅ `pub fn read_file(path: String) -> Result<String, String>` - Read entire file (up to 10MB)
2. ✅ `pub fn write_file(path: String, content: String) -> Result<Bool, String>` - Write/overwrite file
3. ✅ `pub fn append_file(path: String, content: String) -> Result<Bool, String>` - Append to file
4. ✅ Comprehensive test suite with 4 test functions

**Key Findings**:
- ✅ Type checker fix works - direct `Result::Err(field0 = msg)` construction works
- ⚠️ **Match Expression Limitation**: Plat doesn't support blocks with multiple statements in match arms
  - Workaround: Use pattern (check error → early return → extract value)

---

### Phase 4: std::json (Pure Plat Implementation!) ✅ COMPLETED

**Goal**: JSON parser written entirely in Plat (no Rust!)

**Status**: Completed on 2025-10-08 - Full parser and stringify working!

**Module**: `stdlib/std/json.plat`

**What's Been Implemented**:
1. ✅ Complete JsonValue enum with all variants (Null, Bool, Number, String, Array, Object)
2. ✅ Full Parser class with recursive descent parser
3. ✅ All parse methods: `parse_null()`, `parse_bool()`, `parse_number()`, `parse_string()`, `parse_array()`, `parse_object()`, `parse_value()`, `parse()`
4. ✅ Complete stringify implementation: `stringify()`, `stringify_string()`, `stringify_array()`, `stringify_object()`, `float_to_string()`
5. ✅ Error handling with Result types

**Major Fixes Completed (2025-10-07)**:
1. ✅ Parser fix - both `let` and `var` support generic types with angle brackets (commit 4c3df07)
2. ✅ Recursive enum support - two-phase registration (commit c38e1f4)
3. ✅ Match arm type error fix - workaround pattern applied
4. ✅ Missing string methods - `substring()`, `char_at()` implemented
5. ✅ Empty collection type inference - explicit type annotations work
6. ✅ Qualified type names in generics - parser extended
7. ✅ Empty braces `{}` default to Dict literals
8. ✅ Missing List methods - `push()`, `pop()` added
9. ✅ Missing Dict method - `insert()` added
10. ✅ Qualified type resolution - `json::JsonValue` resolves to `std::json::JsonValue`
11. ✅ Symbol loading issue RESOLVED - stdlib modules discovered and loaded
12. ✅ Same-module function calls - codegen fallback lookup added (commit b897c2b)
13. ✅ Enum variant extraction - fixed for complex types (List/Dict/Set/Named)
14. ✅ Duplicate module loading prevented
15. ✅ Cross-module function resolution fixed
16. ✅ Fully qualified type names for enums/classes
17. ✅ Class method collection in symbol phase
18. ✅ Stale unqualified class entries fixed
19. ✅ Enum constructor canonical names
20. ✅ Pattern matching with qualified enum names
21. ✅ **MAJOR MILESTONE**: Type checking PASSES! 🎉

**Verification**:
1. ✅ Module loads successfully with `use std::json;`
2. ✅ Compiles without errors (type checking and codegen both pass)
3. ✅ `json::parse()` function accessible from user code
4. ✅ `json::stringify()` function accessible from user code
5. ✅ Returns proper Result types for error handling
6. ⏸️ **Known Limitation**: Direct enum variant construction (`json::JsonValue::Null`) not yet supported in user code
   - Workaround: Use `json::parse()` to create JsonValue instances

**Language Workarounds Applied**:
- No `||`, `&&`, `!` operators: Replaced with separate/nested `if` statements and `== false`
- No `break` statement: Rewrote loops with boolean continuation flags
- No multi-statement match arms: Use pattern (check error → early return → extract value)

---

### Phase 5: Additional Primitives (Expand Runtime) ✅ COMPLETED

**Status**: Completed on 2025-10-08

**Goal**: Add missing primitives needed by stdlib

**What Was Implemented**:
1. ✅ **Time** (ffi/time.rs):
   - `time_now() -> Int64` - Get current Unix timestamp in milliseconds
   - `time_sleep(millis: Int64) -> Bool` - Sleep for specified milliseconds
2. ✅ **Environment** (ffi/env.rs):
   - `env_get(name: String) -> Option<String>` - Get environment variable (returns Option)
   - `env_set(name: String, value: String) -> Bool` - Set environment variable
   - `env_vars() -> String` - Get all environment variables as newline-separated string
3. ✅ **Random** (ffi/random.rs):
   - `random_int(min: Int64, max: Int64) -> Int64` - Generate random integer in range [min, max]
   - `random_float() -> Float64` - Generate random float in range [0.0, 1.0)
4. ✅ **Process** (ffi/process.rs):
   - `process_exit(code: Int32) -> Never` - Exit the process with exit code
   - `process_args() -> String` - Get command-line arguments as newline-separated string

**Integration**:
- ✅ FFI modules added to `plat-runtime/src/ffi/mod.rs`
- ✅ Type checking added to `plat-hir/src/lib.rs`
- ✅ Codegen added to `plat-codegen/src/lib.rs`
- ✅ Dependency added: `rand = "0.8"` for random number generation

**Testing**:
- ✅ `examples/test_time.plat` - Time functions work correctly
- ✅ `examples/test_random.plat` - Random number generation works
- ✅ `examples/test_process.plat` - Process arguments retrieval works
- ✅ `examples/test_env_simple.plat` - Environment variable operations work

**Key Findings**:
- Bool is represented as I32 in Cranelift, not I8
- Option enums use discriminant hashing with GC-allocated memory layout: [discriminant:i32][padding:i32][value:i64]
- All new primitives integrate seamlessly with existing type system

---

### Phase 6: More Stdlib Modules (Future)

**Planned Modules** (outline only):
- **std::fs**: File system utilities with pathlib-style `Path` class
- **std::net**: High-level TCP networking (`TcpListener`, `TcpStream`)
- **std::http**: HTTP client/server (`Request`, `Response`, `get()`, `post()`)
- **std::collections**: Additional data structures (`Queue`, `Stack`)
- **std::math**: Mathematical functions (`sqrt()`, `pow()`, `sin()`, `cos()`, `abs()`)
- **std::time**: Time/date handling (`Duration`, `now()`, `sleep()`)
- **std::string**: Extended string utilities (`join()`, `repeat()`, `reverse()`)

See `STDLIB_PLAN_ARCHIVE.md` for detailed module designs.

---

## Known Issues & Workarounds

### Type Checker Fix (2025-10-07) ✅ RESOLVED

**Issue**: Generic enum constructor type inference didn't respect function return types

**Solution**: Added `expected_type: Option<&HirType>` parameter to `check_expression()`, pass expected return type down in return statements

**Now Works**:
```plat
fn make_error(msg: String) -> Result<String, String> {
  return Result::Err(field0 = msg);  // ✅ Correctly infers Result<String, String>
}
```

### Recursive Enum Support (2025-10-07) ✅ COMPLETE

**Issue**: Enums couldn't reference themselves (e.g., JSON tree structures)

**Solution**: Two-phase registration at global symbol table and type checker levels

**Now Works**:
```plat
pub enum JsonValue {
  Array(List[JsonValue]),              // ✅ Recursive reference works!
  Object(Dict[String, JsonValue])
}
```

### Cross-Module Codegen Fix ✅ RESOLVED

**Issue**: Cross-module function calls failed due to incorrect signature handling

**Solution**: Threaded global symbol table through codegen to look up actual signatures (commit a819495)

**Now Works**: All cross-module function calls with correct types (Int8, Int16, Int32, Int64, Float32, Float64)

---

## Testing Strategy

### Unit Tests (Per Module)

Each stdlib module has a `test` block with comprehensive coverage:
- Happy path tests
- Edge cases (empty inputs, large inputs, boundary conditions)
- Error path tests (all error types)
- Round-trip tests (for parsers/serializers)

### Integration Tests

`stdlib/tests/` directory with end-to-end tests combining multiple modules

### Benchmarks

Each module has a `bench` block for performance testing

---

## Success Metrics

- ✅ **Usability**: Users can `use std::*` and it just works
- ✅ **Performance**: Stdlib caching speeds up compilation significantly
- ⏸️ **Completeness**: Cover 80% of common use cases (I/O, JSON, HTTP)
- ✅ **Dogfooding**: Stdlib written 100% in Plat (no Rust escape hatches)
- ⏸️ **Testing**: >90% test coverage for all stdlib modules
- ⏸️ **Documentation**: Every public function/class has doc comments

---

## Next Steps

1. ✅ ~~Create directory structure~~ (Completed)
2. ✅ ~~Implement Phase 1: Module resolution~~ (Completed)
3. ✅ ~~Fix cross-module codegen~~ (Completed - commit a819495)
4. ✅ ~~Fix type checker: respect return types~~ (Completed - 2025-10-07)
5. ✅ ~~Write std::io~~ (Completed - 2025-10-07)
6. ✅ ~~Fix parser: support `var x: List<T>`~~ (Completed - 2025-10-07, commit 4c3df07)
7. ✅ ~~Implement Phase 2: Module caching~~ (Completed - 2025-10-07)
8. ✅ ~~Complete std::json implementation~~ (Completed - 2025-10-08)
9. **Future**: Fix qualified enum variant construction in user code (nice-to-have)
10. **Expand**: Add more modules (std::fs, std::net, std::http)

---

## Status Summary

**Start Date**: 2025-01-XX
**Last Updated**: 2025-10-08
**Current Phase**: Phase 5 (Additional Primitives) - COMPLETE!

### Progress by Phase

- ✅ **Phase 1** (Infrastructure): COMPLETE
- ✅ **Phase 2** (Caching): COMPLETE
- ✅ **Phase 3** (std::io): COMPLETE
- ✅ **Phase 4** (std::json): COMPLETE - Full JSON parser and stringify in pure Plat!
- ✅ **Phase 5** (Additional Primitives): COMPLETE - Time, Environment, Random, and Process primitives!
- ⏸️ **Phase 6** (More Modules): Not started

### Compiler Fixes Completed

- ✅ Parser: Keywords in module paths (commit 07106ee)
- ✅ Parser: `var` with generic types (commit 4c3df07)
- ✅ Type checker: Generic enum constructor inference (2025-10-07)
- ✅ Type checker: Recursive enum support (commit c38e1f4)
- ✅ Codegen: Cross-module function signatures (commit a819495)
- ✅ Codegen: Same-module function calls in stdlib (commit b897c2b)
- ✅ Codegen: Enum variant extraction for complex types (2025-10-07)
- ✅ HIR: Symbol loading for stdlib modules (2025-10-07)
- ✅ HIR: Qualified type resolution (2025-10-07)
- ✅ HIR: Cross-module function/type resolution (2025-10-07)

**Maintainer**: Plat Core Team

---

For detailed examples, module designs, testing strategies, and archived implementation notes, see `STDLIB_PLAN_ARCHIVE.md`.
