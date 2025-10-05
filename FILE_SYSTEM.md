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
- [x] Runtime FFI implementation in `fs.rs` (delete file, not directory)
- [x] Type checking in `plat-hir/src/lib.rs`
- [x] Code generation in `plat-codegen/src/lib.rs`
- [x] Test in `examples/test_delete_copy.plat`

#### `file_rename(old_path: String, new_path: String) -> Result<Bool, String>`
- [x] Runtime FFI implementation in `fs.rs` (rename/move file)
- [x] Type checking in `plat-hir/src/lib.rs`
- [x] Code generation in `plat-codegen/src/lib.rs`
- [x] Test in `examples/test_delete_copy.plat`

### Phase 3 Testing
- [x] Test file deletion (file exists and doesn't exist cases)
- [x] Test file rename (same directory and move cases)
- [x] Test error cases (permission denied, etc.)

**Phase 3 Status**: ✅ **COMPLETE** - All file operation functions working correctly!

---

## Phase 4: Directory Operations
**Priority: MEDIUM** - Complete file system support

### Functions to Implement

#### `dir_create(path: String) -> Result<Bool, String>`
- [x] Runtime FFI implementation in `fs.rs` (create single directory, parent must exist)
- [x] Type checking in `plat-hir/src/lib.rs`
- [x] Code generation in `plat-codegen/src/lib.rs`
- [x] Test in `examples/file_test.plat`

#### `dir_create_all(path: String) -> Result<Bool, String>`
- [x] Runtime FFI implementation in `fs.rs` (create directory with all parent directories)
- [x] Type checking in `plat-hir/src/lib.rs`
- [x] Code generation in `plat-codegen/src/lib.rs`
- [x] Test in `examples/file_test.plat`

#### `dir_remove(path: String) -> Result<Bool, String>`
- [x] Runtime FFI implementation in `fs.rs` (remove empty directory only)
- [x] Type checking in `plat-hir/src/lib.rs`
- [x] Code generation in `plat-codegen/src/lib.rs`
- [x] Test in `examples/file_test.plat`

#### `dir_list(path: String) -> Result<String, String>`
- [x] Runtime FFI implementation in `fs.rs` (return newline-separated list of file/directory names)
- [x] Type checking in `plat-hir/src/lib.rs`
- [x] Code generation in `plat-codegen/src/lib.rs`
- [x] Test in `examples/file_test.plat`

### Phase 4 Testing
- [x] Test directory creation (single and recursive)
- [x] Test directory removal (empty and non-empty error cases)
- [x] Test directory listing

**Phase 4 Status**: ✅ **COMPLETE** - All directory operation functions working correctly!

---

## Phase 5: Binary File Support
**Priority: MEDIUM** - For non-text file operations
**Status**: ⚠️ **BLOCKED** - Requires Int8 literal support or array element casting

### Functions to Implement

#### `file_read_binary(fd: Int32, max_bytes: Int32) -> Result<List[Int8], String>`
- [x] Runtime FFI implementation in `fs.rs` (read bytes as List[Int8], no UTF-8 validation)
- [x] Type checking in `plat-hir/src/lib.rs`
- [x] Code generation in `plat-codegen/src/lib.rs`
- [ ] Test in `examples/file_binary_test.plat` - BLOCKED

#### `file_write_binary(fd: Int32, data: List[Int8]) -> Result<Int32, String>`
- [x] Runtime FFI implementation in `fs.rs` (write raw bytes from List[Int8])
- [x] Type checking in `plat-hir/src/lib.rs`
- [x] Code generation in `plat-codegen/src/lib.rs`
- [ ] Test in `examples/file_binary_test.plat` - BLOCKED

### Phase 5 Implementation Notes
**✅ Completed:**
- Added `ARRAY_TYPE_I8` constant to `plat-runtime/src/ffi/array.rs`
- Implemented `plat_array_create_i8` function for Int8 array creation
- Implemented `plat_file_read_binary` FFI function with proper Result<List[Int8], String> return
- Implemented `plat_file_write_binary` FFI function with List[Int8] parameter validation
- Added type checking for both functions in `plat-hir`
- Added code generation for both functions in `plat-codegen`

**⚠️ Blocking Issue:**
- Numeric literals in Plat default to Int32, cannot create List[Int8] literals directly
- Example: `let data: List[Int8] = [0, 1, 2]` fails with "expected List(Int8), found List(Int32)"
- **Workaround needed**: Int8 literal syntax (e.g., `0i8`) or cast expressions for array elements
- Functions are fully implemented and ready to use once language supports Int8 array creation

### Phase 5 Testing
- [ ] Test reading binary files (images, executables) - BLOCKED
- [ ] Test writing binary data - BLOCKED
- [ ] Test round-trip binary operations (write then read) - BLOCKED
- [ ] Verify no UTF-8 corruption on binary data - BLOCKED

---

## Phase 6: File Seeking/Positioning
**Priority: MEDIUM** - For random access file operations

### Functions to Implement

#### `file_seek(fd: Int32, offset: Int64, whence: Int32) -> Result<Int64, String>`
- [ ] Runtime FFI implementation in `fs.rs` (seek to position, whence: 0=start, 1=current, 2=end)
- [ ] Type checking in `plat-hir/src/lib.rs`
- [ ] Code generation in `plat-codegen/src/lib.rs`
- [ ] Test in `examples/file_seek_test.plat`

#### `file_tell(fd: Int32) -> Result<Int64, String>`
- [ ] Runtime FFI implementation in `fs.rs` (get current position in file)
- [ ] Type checking in `plat-hir/src/lib.rs`
- [ ] Code generation in `plat-codegen/src/lib.rs`
- [ ] Test in `examples/file_seek_test.plat`

#### `file_rewind(fd: Int32) -> Result<Bool, String>`
- [ ] Runtime FFI implementation in `fs.rs` (reset position to start of file)
- [ ] Type checking in `plat-hir/src/lib.rs`
- [ ] Code generation in `plat-codegen/src/lib.rs`
- [ ] Test in `examples/file_seek_test.plat`

### Phase 6 Testing
- [ ] Test seeking from start, current, and end positions
- [ ] Test tell() returns correct position after seeks
- [ ] Test rewind() resets to beginning
- [ ] Test seek beyond file boundaries (error cases)

---

## Phase 7: File Permissions/Attributes
**Priority: MEDIUM** - For file metadata and permission management

### Functions to Implement

#### `file_chmod(path: String, mode: Int32) -> Result<Bool, String>`
- [ ] Runtime FFI implementation in `fs.rs` (change file permissions, Unix mode bits)
- [ ] Type checking in `plat-hir/src/lib.rs`
- [ ] Code generation in `plat-codegen/src/lib.rs`
- [ ] Test in `examples/file_permissions_test.plat`

#### `file_get_permissions(path: String) -> Result<Int32, String>`
- [ ] Runtime FFI implementation in `fs.rs` (get permission bits as Int32)
- [ ] Type checking in `plat-hir/src/lib.rs`
- [ ] Code generation in `plat-codegen/src/lib.rs`
- [ ] Test in `examples/file_permissions_test.plat`

#### `file_modified_time(path: String) -> Result<Int64, String>`
- [ ] Runtime FFI implementation in `fs.rs` (get last modified timestamp, Unix epoch seconds)
- [ ] Type checking in `plat-hir/src/lib.rs`
- [ ] Code generation in `plat-codegen/src/lib.rs`
- [ ] Test in `examples/file_permissions_test.plat`

#### `file_created_time(path: String) -> Result<Int64, String>`
- [ ] Runtime FFI implementation in `fs.rs` (get creation timestamp, Unix epoch seconds)
- [ ] Type checking in `plat-hir/src/lib.rs`
- [ ] Code generation in `plat-codegen/src/lib.rs`
- [ ] Test in `examples/file_permissions_test.plat`

### Phase 7 Testing
- [ ] Test chmod with various permission modes (0644, 0755, etc.)
- [ ] Test reading permissions and verify correctness
- [ ] Test modified time changes after file writes
- [ ] Test created time remains stable

---

## Phase 8: Symlink Operations
**Priority: LOW** - For symbolic link management

### Functions to Implement

#### `symlink_create(target: String, link: String) -> Result<Bool, String>`
- [ ] Runtime FFI implementation in `fs.rs` (create symbolic link)
- [ ] Type checking in `plat-hir/src/lib.rs`
- [ ] Code generation in `plat-codegen/src/lib.rs`
- [ ] Test in `examples/file_symlink_test.plat`

#### `symlink_read(path: String) -> Result<String, String>`
- [ ] Runtime FFI implementation in `fs.rs` (read symlink target path)
- [ ] Type checking in `plat-hir/src/lib.rs`
- [ ] Code generation in `plat-codegen/src/lib.rs`
- [ ] Test in `examples/file_symlink_test.plat`

#### `file_is_symlink(path: String) -> Bool`
- [ ] Runtime FFI implementation in `fs.rs` (check if path is a symbolic link)
- [ ] Type checking in `plat-hir/src/lib.rs`
- [ ] Code generation in `plat-codegen/src/lib.rs`
- [ ] Test in `examples/file_symlink_test.plat`

#### `symlink_delete(path: String) -> Result<Bool, String>`
- [ ] Runtime FFI implementation in `fs.rs` (delete symbolic link without following)
- [ ] Type checking in `plat-hir/src/lib.rs`
- [ ] Code generation in `plat-codegen/src/lib.rs`
- [ ] Test in `examples/file_symlink_test.plat`

### Phase 8 Testing
- [ ] Test creating symlinks to files and directories
- [ ] Test reading symlink targets
- [ ] Test symlink detection (is_symlink vs is_dir/exists)
- [ ] Test deleting symlinks without affecting targets

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
