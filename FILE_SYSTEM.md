# File System Primitives Implementation

## Overview
Implement low-level file system primitives following the same pattern as TCP networking functions. All functions return `Result<T, String>` for error handling and use Int32 file descriptors.

## ✅ Fixed Issues

### Result Enum Extraction Bug (FIXED)
**Status**: ✅ RESOLVED

**Issue**: FFI functions that return `Result<T, E>` couldn't be properly pattern matched - extracted values were always 0/empty instead of the actual value.

**Root Cause**: Built-in functions return i64 pointers to heap-allocated enums, but match expressions didn't properly detect and dereference them. The codegen assumed all enums were packed (discriminant in high 32 bits), but FFI functions use heap format.

**Solution**: Added runtime format detection in match expression codegen:
- Heap pointers: range [0x1000, 0x7FFFFFFFFFFF] → load discriminant and values from memory
- Packed enums: outside pointer range → extract discriminant from high 32 bits, value from low 32 bits
- Applied to: match discriminant extraction, pattern bindings, and `?` operator

**Testing** (all passing ✓):
- `examples/test_result.plat` - parse_int returns 42 ✓
- `examples/test_tcp_simple.plat` - tcp_listen returns valid fd ✓
- `examples/test_enum_simple.plat` - Regular enums work (returns 42) ✓
- `examples/test_enum_function.plat` - Functions returning enums work (returns 99) ✓

**Files Modified**:
- `crates/plat-codegen/src/lib.rs` - Added runtime format detection for match expressions, pattern bindings, and Try expressions

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
- [x] **BUG FIX #1**: Fixed Result enum extraction from FFI functions (runtime format detection)
- [x] **BUG FIX #2**: Fixed String extraction from Result enums in match expressions (was hardcoded to Int32)
- [x] Test all file modes (r, w, a, r+, w+, a+) - **PASSING**
- [x] Test error conditions (file not found, permission denied) - **PASSING**

**Phase 1 Status**: ✅ **COMPLETE** - All core file I/O functions working correctly!

---

## Phase 2: File Metadata & Checks
**Priority: HIGH** - Common operations for file handling

### Functions to Implement

#### `file_exists(path: String) -> Bool`
- [x] Runtime FFI implementation in `fs.rs` (simple boolean check, no Result needed)
- [x] Type checking in `plat-hir/src/lib.rs`
- [x] Code generation in `plat-codegen/src/lib.rs`
- [x] Test in `examples/test_file_metadata.plat`

#### `file_size(path: String) -> Result<Int64, String>`
- [x] Runtime FFI implementation in `fs.rs` (returns file size in bytes, Int64 for large files)
- [x] Type checking in `plat-hir/src/lib.rs`
- [x] Code generation in `plat-codegen/src/lib.rs`
- [x] Test in `examples/test_file_metadata.plat`

#### `file_is_dir(path: String) -> Bool`
- [x] Runtime FFI implementation in `fs.rs` (check if path is a directory)
- [x] Type checking in `plat-hir/src/lib.rs`
- [x] Code generation in `plat-codegen/src/lib.rs`
- [x] Test in `examples/test_file_metadata.plat`

### Phase 2 Testing
- [x] Test file existence checks
- [x] Test file size for various file sizes
- [x] Test directory vs file detection

**Phase 2 Status**: ✅ **COMPLETE** - All file metadata functions working correctly!

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
