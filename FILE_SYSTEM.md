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
**Status**: ✅ **COMPLETE** - Int8 literal syntax implemented (e.g., `0i8`, `127i8`)

### Functions to Implement

#### `file_read_binary(fd: Int32, max_bytes: Int32) -> Result<List[Int8], String>`
- [x] Runtime FFI implementation in `fs.rs` (read bytes as List[Int8], no UTF-8 validation)
- [x] Type checking in `plat-hir/src/lib.rs`
- [x] Code generation in `plat-codegen/src/lib.rs`
- [x] Test with Int8 literals (e.g., `[0i8, 1i8, 2i8]`)

#### `file_write_binary(fd: Int32, data: List[Int8]) -> Result<Int32, String>`
- [x] Runtime FFI implementation in `fs.rs` (write raw bytes from List[Int8])
- [x] Type checking in `plat-hir/src/lib.rs`
- [x] Code generation in `plat-codegen/src/lib.rs`
- [x] Test with Int8 literals (e.g., `[0i8, 1i8, 2i8]`)

### Phase 5 Implementation Notes
**✅ Completed:**
- Added `ARRAY_TYPE_I8` constant to `plat-runtime/src/ffi/array.rs`
- Implemented `plat_array_create_i8` function for Int8 array creation
- Implemented `plat_file_read_binary` FFI function with proper Result<List[Int8], String> return
- Implemented `plat_file_write_binary` FFI function with List[Int8] parameter validation
- Added type checking for both functions in `plat-hir`
- Added code generation for both functions in `plat-codegen`
- **RESOLVED**: Added typed numeric literal support to the language
  - Extended lexer to recognize suffixes: `i8`, `i16`, `i32`, `i64`, `f8`, `f16`, `f32`, `f64`
  - Updated parser, HIR type checking, and codegen to handle all numeric types
  - Example: `let data: List[Int8] = [0i8, 1i8, 127i8]` now works correctly
  - Successfully tested with `examples/test_int8_literal.plat`

### Phase 5 Testing
- [x] Test Int8 literal syntax works correctly
- [x] Test creating List[Int8] arrays with typed literals
- [x] Test writing binary data (`examples/test_binary_simple.plat`, `examples/test_binary_files.plat`)
- [x] Test reading binary data (both test files validate successful reads)
- [x] Test round-trip binary operations (write then read - validated in both test files)
- [x] Verify no UTF-8 corruption on binary data (binary functions bypass UTF-8 validation)

**Phase 5 Status**: ✅ **COMPLETE** - All functions implemented and fully tested!

---

## Phase 6: File Seeking/Positioning
**Priority: MEDIUM** - For random access file operations
**Status**: ✅ **COMPLETE** - All file seeking functions working correctly!

### Functions to Implement

#### `file_seek(fd: Int32, offset: Int64, whence: Int32) -> Result<Int64, String>`
- [x] Runtime FFI implementation in `fs.rs` (seek to position, whence: 0=start, 1=current, 2=end)
- [x] Type checking in `plat-hir/src/lib.rs`
- [x] Code generation in `plat-codegen/src/lib.rs`
- [x] Test in `examples/test_file_seek.plat`

#### `file_tell(fd: Int32) -> Result<Int64, String>`
- [x] Runtime FFI implementation in `fs.rs` (get current position in file)
- [x] Type checking in `plat-hir/src/lib.rs`
- [x] Code generation in `plat-codegen/src/lib.rs`
- [x] Test in `examples/test_file_seek.plat`

#### `file_rewind(fd: Int32) -> Result<Bool, String>`
- [x] Runtime FFI implementation in `fs.rs` (reset position to start of file)
- [x] Type checking in `plat-hir/src/lib.rs`
- [x] Code generation in `plat-codegen/src/lib.rs`
- [x] Test in `examples/test_file_seek.plat`

### Phase 6 Implementation Notes
**✅ Completed:**
- Implemented `plat_file_seek` FFI function with SeekFrom support (Start, Current, End)
- Implemented `plat_file_tell` FFI function using stream_position()
- Implemented `plat_file_rewind` FFI function (seek to start)
- Added type checking for all three functions in `plat-hir`
- Added code generation for all three functions in `plat-codegen`
- **BUG FIX**: Fixed Int64 handling in Result enum extraction
  - Int64 and Float64 values are 8 bytes, not 4 bytes
  - Updated heap format offset calculations for 8-byte types
  - Fixed match expression return type detection for Int64 pattern bindings
  - Added Int64/Float64 to continuation block parameter type mapping

### Phase 6 Testing
- [x] Test seeking from start, current, and end positions
- [x] Test tell() returns correct position after seeks
- [x] Test rewind() resets to beginning
- [x] Test seek beyond file boundaries (error cases)

---

## Phase 7: File Permissions/Attributes
**Priority: MEDIUM** - For file metadata and permission management
**Status**: ✅ **COMPLETE** - All file permissions and timestamp functions working correctly!

### Functions to Implement

#### `file_chmod(path: String, mode: Int32) -> Result<Bool, String>`
- [x] Runtime FFI implementation in `fs.rs` (change file permissions, Unix mode bits)
- [x] Type checking in `plat-hir/src/lib.rs`
- [x] Code generation in `plat-codegen/src/lib.rs`
- [x] Test in `examples/test_phase7.plat`

#### `file_get_permissions(path: String) -> Result<Int32, String>`
- [x] Runtime FFI implementation in `fs.rs` (get permission bits as Int32)
- [x] Type checking in `plat-hir/src/lib.rs`
- [x] Code generation in `plat-codegen/src/lib.rs`
- [x] Test in `examples/test_phase7.plat`

#### `file_modified_time(path: String) -> Result<Int64, String>`
- [x] Runtime FFI implementation in `fs.rs` (get last modified timestamp, Unix epoch seconds)
- [x] Type checking in `plat-hir/src/lib.rs`
- [x] Code generation in `plat-codegen/src/lib.rs`
- [x] Test in `examples/test_phase7.plat`

#### `file_created_time(path: String) -> Result<Int64, String>`
- [x] Runtime FFI implementation in `fs.rs` (get creation timestamp, Unix epoch seconds)
- [x] Type checking in `plat-hir/src/lib.rs`
- [x] Code generation in `plat-codegen/src/lib.rs`
- [x] Test in `examples/test_phase7.plat`

### Phase 7 Implementation Notes
**✅ Completed:**
- Implemented `plat_file_chmod` FFI function with platform-specific handling (Unix vs Windows)
- Implemented `plat_file_get_permissions` FFI function returning Unix mode bits
- Implemented `plat_file_modified_time` FFI function returning Unix epoch seconds
- Implemented `plat_file_created_time` FFI function returning Unix epoch seconds
- Added type checking for all four functions in `plat-hir`
- Added code generation for all four functions in `plat-codegen`
- **Platform Notes**:
  - On Unix: Full chmod support with all permission bits
  - On Windows: Limited to read-only attribute (best-effort compatibility)
  - Permissions include file type bits (e.g., 0o100644 for regular file)

### Phase 7 Testing
- [x] Test chmod with various permission modes (0444, 0644, etc.)
- [x] Test reading permissions and verify correctness
- [x] Test modified time returns valid Unix timestamps
- [x] Test created time returns valid Unix timestamps

---

## Phase 8: Symlink Operations
**Priority: LOW** - For symbolic link management
**Status**: ✅ **COMPLETE** - All symlink operation functions working correctly!

### Functions to Implement

#### `symlink_create(target: String, link: String) -> Result<Bool, String>`
- [x] Runtime FFI implementation in `fs.rs` (create symbolic link)
- [x] Type checking in `plat-hir/src/lib.rs`
- [x] Code generation in `plat-codegen/src/lib.rs`
- [x] Test in `examples/test_symlink.plat`

#### `symlink_read(path: String) -> Result<String, String>`
- [x] Runtime FFI implementation in `fs.rs` (read symlink target path)
- [x] Type checking in `plat-hir/src/lib.rs`
- [x] Code generation in `plat-codegen/src/lib.rs`
- [x] Test in `examples/test_symlink.plat`

#### `file_is_symlink(path: String) -> Bool`
- [x] Runtime FFI implementation in `fs.rs` (check if path is a symbolic link)
- [x] Type checking in `plat-hir/src/lib.rs`
- [x] Code generation in `plat-codegen/src/lib.rs`
- [x] Test in `examples/test_symlink.plat`

#### `symlink_delete(path: String) -> Result<Bool, String>`
- [x] Runtime FFI implementation in `fs.rs` (delete symbolic link without following)
- [x] Type checking in `plat-hir/src/lib.rs`
- [x] Code generation in `plat-codegen/src/lib.rs`
- [x] Test in `examples/test_symlink.plat`

### Phase 8 Implementation Notes
**✅ Completed:**
- Implemented `plat_symlink_create` FFI function with platform-specific handling (Unix vs Windows)
- Implemented `plat_symlink_read` FFI function using std::fs::read_link
- Implemented `plat_file_is_symlink` FFI function using symlink_metadata
- Implemented `plat_symlink_delete` FFI function with safety checks (verifies path is actually a symlink)
- Added type checking for all four functions in `plat-hir`
- Added code generation for all four functions in `plat-codegen`
- **Platform Notes**:
  - On Unix: Uses std::os::unix::fs::symlink
  - On Windows: Automatically detects if target is directory or file and uses appropriate symlink function
  - symlink_delete verifies the path is actually a symlink before deletion to prevent accidental file removal

### Phase 8 Testing
- [x] Test creating symlinks to files and directories
- [x] Test reading symlink targets
- [x] Test symlink detection (is_symlink vs is_dir/exists)
- [x] Test deleting symlinks without affecting targets

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
- [x] Update `CLAUDE.md` with file system functions documentation
- [x] Add examples to Quick Reference section
- [x] Update Production Ready checklist

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
