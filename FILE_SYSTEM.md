# File System Primitives Implementation

## Overview
Implement low-level file system primitives following the same pattern as TCP networking functions. All functions return `Result<T, String>` for error handling and use Int32 file descriptors.

## ⚠️ Known Issues

### Result Enum Extraction Bug (Pre-existing)
**Status**: Affects ALL built-in functions returning Result enums (parse_int, TCP functions, file I/O)

**Symptoms**:
- FFI functions that return `Result<T, E>` cannot be properly pattern matched
- Extracted values are always 0/empty instead of the actual value
- Example: `file_open()` returns Result but match extracts fd=0 instead of actual fd (e.g., 2000)

**Root Cause**:
- Built-in functions return i64 pointers to heap-allocated enums
- Match expressions on these Results don't properly dereference and extract values
- Regular Plat functions returning enums work correctly
- Only affects FFI/built-in functions

**Workaround**: None currently - Result-based error handling from built-ins is non-functional

**Testing**:
- `examples/test_result.plat` - parse_int returns 0 instead of 42
- `examples/test_tcp_simple.plat` - tcp_listen returns fd=0 instead of actual fd
- `examples/fs_test_final.plat` - file_open returns fd=0 instead of actual fd
- `examples/test_enum_simple.plat` - Regular enums work correctly (returns 42) ✓
- `examples/test_enum_function.plat` - Functions returning enums work (returns 99) ✓

**Next Steps**: Fix enum dereference in match codegen for FFI function returns

## Architecture Pattern (Based on Networking Implementation)
- [x] Study existing TCP networking implementation in `plat-runtime/src/ffi/net.rs`
- [x] Understand type checking pattern in `plat-hir/src/lib.rs`
- [x] Understand code generation pattern in `plat-codegen/src/lib.rs`

---

## Phase 1: Core File I/O (Essential)
**Priority: HIGHEST** - These are the minimum viable primitives

### Setup
- [x] Create `crates/plat-runtime/src/ffi/fs.rs`
- [x] Add file descriptor management (HashMap<i32, File> with Mutex)
- [x] Add FD counter starting at 2000 (avoid conflicts with network FDs)
- [x] Reuse helper functions: `alloc_c_string`, `create_result_enum_*`, `variant_hash`

### Functions to Implement

#### `file_open(path: String, mode: String) -> Result<Int32, String>`
- [x] Runtime FFI implementation in `fs.rs`
  - Modes: "r" (read), "w" (write/truncate), "a" (append), "r+" (read/write), "w+" (read/write/truncate), "a+" (read/append)
  - Returns file descriptor on success
- [x] Type checking in `plat-hir/src/lib.rs`
- [x] Code generation in `plat-codegen/src/lib.rs`
- [x] Test in `examples/test_open_only.plat` (function compiles and runs)

#### `file_read(fd: Int32, max_bytes: Int32) -> Result<String, String>`
- [x] Runtime FFI implementation in `fs.rs`
  - Read up to max_bytes from file descriptor
  - Returns string with actual bytes read (UTF-8 validated)
- [x] Type checking in `plat-hir/src/lib.rs`
- [x] Code generation in `plat-codegen/src/lib.rs`
- [x] Test in examples (function compiles)

#### `file_write(fd: Int32, data: String) -> Result<Int32, String>`
- [x] Runtime FFI implementation in `fs.rs`
  - Write string to file descriptor
  - Returns bytes written on success
- [x] Type checking in `plat-hir/src/lib.rs`
- [x] Code generation in `plat-codegen/src/lib.rs`
- [x] Test in examples (function compiles)

#### `file_close(fd: Int32) -> Result<Bool, String>`
- [x] Runtime FFI implementation in `fs.rs`
  - Close file descriptor
  - Returns true on success
- [x] Type checking in `plat-hir/src/lib.rs`
- [x] Code generation in `plat-codegen/src/lib.rs`
- [x] Test in examples (function compiles)

### Phase 1 Integration
- [x] Export fs module in `crates/plat-runtime/src/ffi/mod.rs`
- [x] Create test files for basic file operations
- [x] Verify compilation and type checking works correctly
- [ ] ~~Test all file modes (r, w, a, r+, w+, a+)~~ (blocked by Result extraction bug)
- [ ] ~~Test error conditions (file not found, permission denied)~~ (blocked by Result extraction bug)

---

## Phase 2: File Metadata & Checks
**Priority: HIGH** - Common operations for file handling

### Functions to Implement

#### `file_exists(path: String) -> Bool`
- [ ] Runtime FFI implementation in `fs.rs` (simple boolean check, no Result needed)
- [ ] Type checking in `plat-hir/src/lib.rs`
- [ ] Code generation in `plat-codegen/src/lib.rs`
- [ ] Test in `examples/file_test.plat`

#### `file_size(path: String) -> Result<Int64, String>`
- [ ] Runtime FFI implementation in `fs.rs` (returns file size in bytes, Int64 for large files)
- [ ] Type checking in `plat-hir/src/lib.rs`
- [ ] Code generation in `plat-codegen/src/lib.rs`
- [ ] Test in `examples/file_test.plat`

#### `file_is_dir(path: String) -> Bool`
- [ ] Runtime FFI implementation in `fs.rs` (check if path is a directory)
- [ ] Type checking in `plat-hir/src/lib.rs`
- [ ] Code generation in `plat-codegen/src/lib.rs`
- [ ] Test in `examples/file_test.plat`

### Phase 2 Testing
- [ ] Test file existence checks
- [ ] Test file size for various file sizes
- [ ] Test directory vs file detection

---

## Phase 3: File Operations
**Priority: MEDIUM** - Useful for file manipulation

### Functions to Implement

#### `file_delete(path: String) -> Result<Bool, String>`
- [ ] Runtime FFI implementation in `fs.rs` (delete file, not directory)
- [ ] Type checking in `plat-hir/src/lib.rs`
- [ ] Code generation in `plat-codegen/src/lib.rs`
- [ ] Test in `examples/file_test.plat`

#### `file_rename(old_path: String, new_path: String) -> Result<Bool, String>`
- [ ] Runtime FFI implementation in `fs.rs` (rename/move file)
- [ ] Type checking in `plat-hir/src/lib.rs`
- [ ] Code generation in `plat-codegen/src/lib.rs`
- [ ] Test in `examples/file_test.plat`

### Phase 3 Testing
- [ ] Test file deletion (file exists and doesn't exist cases)
- [ ] Test file rename (same directory and move cases)
- [ ] Test error cases (permission denied, etc.)

---

## Phase 4: Directory Operations
**Priority: MEDIUM** - Complete file system support

### Functions to Implement

#### `dir_create(path: String) -> Result<Bool, String>`
- [ ] Runtime FFI implementation in `fs.rs` (create single directory, parent must exist)
- [ ] Type checking in `plat-hir/src/lib.rs`
- [ ] Code generation in `plat-codegen/src/lib.rs`
- [ ] Test in `examples/file_test.plat`

#### `dir_create_all(path: String) -> Result<Bool, String>`
- [ ] Runtime FFI implementation in `fs.rs` (create directory with all parent directories)
- [ ] Type checking in `plat-hir/src/lib.rs`
- [ ] Code generation in `plat-codegen/src/lib.rs`
- [ ] Test in `examples/file_test.plat`

#### `dir_remove(path: String) -> Result<Bool, String>`
- [ ] Runtime FFI implementation in `fs.rs` (remove empty directory only)
- [ ] Type checking in `plat-hir/src/lib.rs`
- [ ] Code generation in `plat-codegen/src/lib.rs`
- [ ] Test in `examples/file_test.plat`

#### `dir_list(path: String) -> Result<String, String>`
- [ ] Runtime FFI implementation in `fs.rs` (return newline-separated list of file/directory names)
- [ ] Type checking in `plat-hir/src/lib.rs`
- [ ] Code generation in `plat-codegen/src/lib.rs`
- [ ] Test in `examples/file_test.plat`

### Phase 4 Testing
- [ ] Test directory creation (single and recursive)
- [ ] Test directory removal (empty and non-empty error cases)
- [ ] Test directory listing

---

## Implementation Checklist for Each Function

For each function, follow this pattern:

### Runtime FFI (`plat-runtime/src/ffi/fs.rs`)
1. [ ] Add `#[no_mangle] pub extern "C"` function
2. [ ] Validate input pointers (null checks)
3. [ ] Convert C strings to Rust strings
4. [ ] Perform file operation with proper error handling
5. [ ] Return appropriate Result enum variant

### Type Checking (`plat-hir/src/lib.rs`)
1. [ ] Add `if function == "function_name"` block
2. [ ] Validate argument count and names
3. [ ] Type-check each argument
4. [ ] Return correct HirType (usually wrapped in Result enum)

### Code Generation (`plat-codegen/src/lib.rs`)
1. [ ] Add `if function == "function_name"` block
2. [ ] Extract named arguments
3. [ ] Generate expressions for arguments
4. [ ] Declare function signature (parameters + return type)
5. [ ] Declare function with `Linkage::Import`
6. [ ] Generate call instruction

---

## Documentation Updates
- [ ] Update `CLAUDE.md` with file system functions documentation
- [ ] Add examples to Quick Reference section
- [ ] Update Production Ready checklist

---

## Future Enhancements (Not in This Plan)
- Binary file support (separate from String-based I/O)
- File seeking/positioning
- File permissions/attributes
- Symlink operations
- Path manipulation helpers (can be implemented in Plat stdlib)
- Async/non-blocking file I/O

---

## Files Summary

### Files to CREATE
1. `crates/plat-runtime/src/ffi/fs.rs` (~400-500 lines)
2. `examples/file_test.plat` (comprehensive test file)

### Files to EDIT
1. `crates/plat-runtime/src/ffi/mod.rs` (add `pub mod fs;`)
2. `crates/plat-hir/src/lib.rs` (add type checking for all functions)
3. `crates/plat-codegen/src/lib.rs` (add codegen for all functions)
4. `CLAUDE.md` (documentation updates)

---

## Notes
- Follow the exact same patterns as TCP networking implementation
- Use proper error messages for common failures (not found, permission denied, etc.)
- All file operations use Int32 file descriptors (FDs start at 2000)
- All functions that can fail return `Result<T, String>`
- Simple predicates (exists, is_dir) return Bool directly
