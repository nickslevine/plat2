use std::ffi::CStr;
use std::os::raw::c_char;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write, Seek, SeekFrom};
use std::sync::Mutex;
use std::collections::HashMap;
use super::core::{plat_gc_alloc, plat_gc_alloc_atomic};
use super::array::{RuntimeArray, plat_array_create_i8};

// Global file storage
// Maps file descriptor (i32) to File handle
lazy_static::lazy_static! {
    static ref FILES: Mutex<HashMap<i32, File>> = Mutex::new(HashMap::new());
    static ref NEXT_FD: Mutex<i32> = Mutex::new(2000); // Start at 2000 to avoid conflicts with network FDs
}

/// Get next available file descriptor
fn next_fd() -> i32 {
    let mut fd = NEXT_FD.lock().unwrap();
    let result = *fd;
    *fd += 1;
    result
}

/// Compute variant discriminant using same hash as codegen
fn variant_hash(name: &str) -> u32 {
    let mut hash = 0u32;
    for byte in name.bytes() {
        hash = hash.wrapping_mul(31).wrapping_add(byte as u32);
    }
    hash
}

/// Create Result::Ok(i32) enum value
unsafe fn create_result_enum_ok_i32(value: i32) -> i64 {
    let ok_disc = variant_hash("Ok");
    // Heap-allocated: [discriminant:i32][value:i32]
    let ptr = plat_gc_alloc(8) as *mut i32;
    *ptr = ok_disc as i32;
    *ptr.add(1) = value;
    ptr as i64
}

/// Create Result::Ok(bool) enum value
unsafe fn create_result_enum_ok_bool(value: bool) -> i64 {
    let ok_disc = variant_hash("Ok");
    // Heap-allocated: [discriminant:i32][value:i32]
    let ptr = plat_gc_alloc(8) as *mut i32;
    *ptr = ok_disc as i32;
    *ptr.add(1) = if value { 1 } else { 0 };
    ptr as i64
}

/// Create Result::Ok(i64) enum value
unsafe fn create_result_enum_ok_i64(value: i64) -> i64 {
    let ok_disc = variant_hash("Ok");
    // Heap-allocated: [discriminant:i32][padding:i32][value:i64]
    let ptr = plat_gc_alloc(16) as *mut i32;
    *ptr = ok_disc as i32;
    let val_ptr = ptr.add(2) as *mut i64;
    *val_ptr = value;
    ptr as i64
}

/// Create Result::Ok(String) enum value
unsafe fn create_result_enum_ok_string(value: *const c_char) -> i64 {
    let ok_disc = variant_hash("Ok");
    // Heap-allocated: [discriminant:i32][padding:i32][string_ptr:i64]
    let ptr = plat_gc_alloc(16) as *mut i32;
    *ptr = ok_disc as i32;
    let str_ptr = ptr.add(2) as *mut i64;
    *str_ptr = value as i64;
    ptr as i64
}

/// Create Result::Err(String) enum value
unsafe fn create_result_enum_err_string(error_msg: *const c_char) -> i64 {
    let err_disc = variant_hash("Err");
    // Heap-allocated: [discriminant:i32][padding:i32][error_ptr:i64]
    let ptr = plat_gc_alloc(16) as *mut i32;
    *ptr = err_disc as i32;
    let msg_ptr = ptr.add(2) as *mut i64;
    *msg_ptr = error_msg as i64;
    ptr as i64
}

/// Create Result::Ok(List[Int8]) enum value
unsafe fn create_result_enum_ok_list_i8(array_ptr: *mut RuntimeArray) -> i64 {
    let ok_disc = variant_hash("Ok");
    // Heap-allocated: [discriminant:i32][padding:i32][array_ptr:i64]
    let ptr = plat_gc_alloc(16) as *mut i32;
    *ptr = ok_disc as i32;
    let arr_ptr = ptr.add(2) as *mut i64;
    *arr_ptr = array_ptr as i64;
    ptr as i64
}

/// Helper to allocate a C string in GC memory
unsafe fn alloc_c_string(s: &str) -> *const c_char {
    let mut bytes = s.as_bytes().to_vec();
    bytes.push(0); // null terminator
    let size = bytes.len();
    let gc_ptr = plat_gc_alloc_atomic(size);
    if gc_ptr.is_null() {
        return std::ptr::null();
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), gc_ptr, size);
    gc_ptr as *const c_char
}

/// Open a file with specified mode
/// Returns Result<Int32, String> where Int32 is the file descriptor
///
/// Modes:
/// - "r"  = read only
/// - "w"  = write (create/truncate)
/// - "a"  = append (create if doesn't exist)
/// - "r+" = read/write (file must exist)
/// - "w+" = read/write (create/truncate)
/// - "a+" = read/append (create if doesn't exist)
#[no_mangle]
pub extern "C" fn plat_file_open(path_ptr: *const c_char, mode_ptr: *const c_char) -> i64 {
    unsafe {
        if path_ptr.is_null() {
            let err_msg = alloc_c_string("file_open: path is null");
            return create_result_enum_err_string(err_msg);
        }

        if mode_ptr.is_null() {
            let err_msg = alloc_c_string("file_open: mode is null");
            return create_result_enum_err_string(err_msg);
        }

        let path = match CStr::from_ptr(path_ptr).to_str() {
            Ok(s) => s,
            Err(_) => {
                let err_msg = alloc_c_string("file_open: invalid path string");
                return create_result_enum_err_string(err_msg);
            }
        };

        let mode = match CStr::from_ptr(mode_ptr).to_str() {
            Ok(s) => s,
            Err(_) => {
                let err_msg = alloc_c_string("file_open: invalid mode string");
                return create_result_enum_err_string(err_msg);
            }
        };

        let file_result = match mode {
            "r" => OpenOptions::new()
                .read(true)
                .open(path),
            "w" => OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(path),
            "a" => OpenOptions::new()
                .write(true)
                .create(true)
                .append(true)
                .open(path),
            "r+" => OpenOptions::new()
                .read(true)
                .write(true)
                .open(path),
            "w+" => OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(true)
                .open(path),
            "a+" => OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .append(true)
                .open(path),
            _ => {
                let err_msg = alloc_c_string(&format!("file_open: invalid mode '{}' (use r, w, a, r+, w+, or a+)", mode));
                return create_result_enum_err_string(err_msg);
            }
        };

        match file_result {
            Ok(file) => {
                let fd = next_fd();
                FILES.lock().unwrap().insert(fd, file);
                create_result_enum_ok_i32(fd)
            }
            Err(e) => {
                let err_msg = alloc_c_string(&format!("file_open failed: {}", e));
                create_result_enum_err_string(err_msg)
            }
        }
    }
}

/// Read up to max_bytes from file
/// Returns Result<String, String>
#[no_mangle]
pub extern "C" fn plat_file_read(fd: i32, max_bytes: i32) -> i64 {
    unsafe {
        let mut files = FILES.lock().unwrap();

        if let Some(file) = files.get_mut(&fd) {
            let mut buffer = vec![0u8; max_bytes as usize];

            match file.read(&mut buffer) {
                Ok(bytes_read) => {
                    buffer.truncate(bytes_read);

                    // Convert bytes to string (handle UTF-8 properly)
                    match String::from_utf8(buffer) {
                        Ok(s) => {
                            let c_str = alloc_c_string(&s);
                            create_result_enum_ok_string(c_str)
                        }
                        Err(_) => {
                            let err_msg = alloc_c_string("file_read: file contains invalid UTF-8 data");
                            create_result_enum_err_string(err_msg)
                        }
                    }
                }
                Err(e) => {
                    let err_msg = alloc_c_string(&format!("file_read failed: {}", e));
                    create_result_enum_err_string(err_msg)
                }
            }
        } else {
            let err_msg = alloc_c_string("file_read: invalid file descriptor");
            create_result_enum_err_string(err_msg)
        }
    }
}

/// Write data to file
/// Returns Result<Int32, String> where Int32 is the number of bytes written
#[no_mangle]
pub extern "C" fn plat_file_write(fd: i32, data_ptr: *const c_char) -> i64 {
    unsafe {
        if data_ptr.is_null() {
            let err_msg = alloc_c_string("file_write: data is null");
            return create_result_enum_err_string(err_msg);
        }

        let data = match CStr::from_ptr(data_ptr).to_str() {
            Ok(s) => s,
            Err(_) => {
                let err_msg = alloc_c_string("file_write: invalid data string");
                return create_result_enum_err_string(err_msg);
            }
        };

        let mut files = FILES.lock().unwrap();

        if let Some(file) = files.get_mut(&fd) {
            match file.write(data.as_bytes()) {
                Ok(bytes_written) => {
                    // Ensure data is flushed to disk
                    if let Err(e) = file.flush() {
                        let err_msg = alloc_c_string(&format!("file_write: failed to flush: {}", e));
                        return create_result_enum_err_string(err_msg);
                    }
                    create_result_enum_ok_i32(bytes_written as i32)
                }
                Err(e) => {
                    let err_msg = alloc_c_string(&format!("file_write failed: {}", e));
                    create_result_enum_err_string(err_msg)
                }
            }
        } else {
            let err_msg = alloc_c_string("file_write: invalid file descriptor");
            create_result_enum_err_string(err_msg)
        }
    }
}

/// Close file
/// Returns Result<Bool, String>
#[no_mangle]
pub extern "C" fn plat_file_close(fd: i32) -> i64 {
    unsafe {
        // Try to remove from files
        if FILES.lock().unwrap().remove(&fd).is_some() {
            return create_result_enum_ok_bool(true);
        }

        // File descriptor not found
        let err_msg = alloc_c_string("file_close: invalid file descriptor");
        create_result_enum_err_string(err_msg)
    }
}

/// Check if file exists
/// Returns Bool (1 = true, 0 = false)
#[no_mangle]
pub extern "C" fn plat_file_exists(path_ptr: *const c_char) -> i32 {
    unsafe {
        if path_ptr.is_null() {
            return 0; // false
        }

        let path = match CStr::from_ptr(path_ptr).to_str() {
            Ok(s) => s,
            Err(_) => return 0, // false for invalid path
        };

        if std::path::Path::new(path).exists() {
            1 // true
        } else {
            0 // false
        }
    }
}

/// Get file size in bytes
/// Returns Result<Int64, String>
#[no_mangle]
pub extern "C" fn plat_file_size(path_ptr: *const c_char) -> i64 {
    unsafe {
        if path_ptr.is_null() {
            let err_msg = alloc_c_string("file_size: path is null");
            return create_result_enum_err_string(err_msg);
        }

        let path = match CStr::from_ptr(path_ptr).to_str() {
            Ok(s) => s,
            Err(_) => {
                let err_msg = alloc_c_string("file_size: invalid path string");
                return create_result_enum_err_string(err_msg);
            }
        };

        match std::fs::metadata(path) {
            Ok(metadata) => {
                let size = metadata.len() as i64;
                create_result_enum_ok_i64(size)
            }
            Err(e) => {
                let err_msg = alloc_c_string(&format!("file_size failed: {}", e));
                create_result_enum_err_string(err_msg)
            }
        }
    }
}

/// Check if path is a directory
/// Returns Bool (1 = true, 0 = false)
#[no_mangle]
pub extern "C" fn plat_file_is_dir(path_ptr: *const c_char) -> i32 {
    unsafe {
        if path_ptr.is_null() {
            return 0; // false
        }

        let path = match CStr::from_ptr(path_ptr).to_str() {
            Ok(s) => s,
            Err(_) => return 0, // false for invalid path
        };

        match std::fs::metadata(path) {
            Ok(metadata) => {
                if metadata.is_dir() {
                    1 // true
                } else {
                    0 // false
                }
            }
            Err(_) => 0, // false if path doesn't exist or other error
        }
    }
}

/// Delete a file (not directory)
/// Returns Result<Bool, String>
#[no_mangle]
pub extern "C" fn plat_file_delete(path_ptr: *const c_char) -> i64 {
    unsafe {
        if path_ptr.is_null() {
            let err_msg = alloc_c_string("file_delete: path is null");
            return create_result_enum_err_string(err_msg);
        }

        let path = match CStr::from_ptr(path_ptr).to_str() {
            Ok(s) => s,
            Err(_) => {
                let err_msg = alloc_c_string("file_delete: invalid path string");
                return create_result_enum_err_string(err_msg);
            }
        };

        match std::fs::remove_file(path) {
            Ok(_) => create_result_enum_ok_bool(true),
            Err(e) => {
                let err_msg = alloc_c_string(&format!("file_delete failed: {}", e));
                create_result_enum_err_string(err_msg)
            }
        }
    }
}

/// Rename or move a file
/// Returns Result<Bool, String>
#[no_mangle]
pub extern "C" fn plat_file_rename(old_path_ptr: *const c_char, new_path_ptr: *const c_char) -> i64 {
    unsafe {
        if old_path_ptr.is_null() {
            let err_msg = alloc_c_string("file_rename: old_path is null");
            return create_result_enum_err_string(err_msg);
        }

        if new_path_ptr.is_null() {
            let err_msg = alloc_c_string("file_rename: new_path is null");
            return create_result_enum_err_string(err_msg);
        }

        let old_path = match CStr::from_ptr(old_path_ptr).to_str() {
            Ok(s) => s,
            Err(_) => {
                let err_msg = alloc_c_string("file_rename: invalid old_path string");
                return create_result_enum_err_string(err_msg);
            }
        };

        let new_path = match CStr::from_ptr(new_path_ptr).to_str() {
            Ok(s) => s,
            Err(_) => {
                let err_msg = alloc_c_string("file_rename: invalid new_path string");
                return create_result_enum_err_string(err_msg);
            }
        };

        match std::fs::rename(old_path, new_path) {
            Ok(_) => create_result_enum_ok_bool(true),
            Err(e) => {
                let err_msg = alloc_c_string(&format!("file_rename failed: {}", e));
                create_result_enum_err_string(err_msg)
            }
        }
    }
}

/// Create a directory (parent must exist)
/// Returns Result<Bool, String>
#[no_mangle]
pub extern "C" fn plat_dir_create(path_ptr: *const c_char) -> i64 {
    unsafe {
        if path_ptr.is_null() {
            let err_msg = alloc_c_string("dir_create: path is null");
            return create_result_enum_err_string(err_msg);
        }

        let path = match CStr::from_ptr(path_ptr).to_str() {
            Ok(s) => s,
            Err(_) => {
                let err_msg = alloc_c_string("dir_create: invalid path string");
                return create_result_enum_err_string(err_msg);
            }
        };

        match std::fs::create_dir(path) {
            Ok(_) => create_result_enum_ok_bool(true),
            Err(e) => {
                let err_msg = alloc_c_string(&format!("dir_create failed: {}", e));
                create_result_enum_err_string(err_msg)
            }
        }
    }
}

/// Create a directory with all parent directories
/// Returns Result<Bool, String>
#[no_mangle]
pub extern "C" fn plat_dir_create_all(path_ptr: *const c_char) -> i64 {
    unsafe {
        if path_ptr.is_null() {
            let err_msg = alloc_c_string("dir_create_all: path is null");
            return create_result_enum_err_string(err_msg);
        }

        let path = match CStr::from_ptr(path_ptr).to_str() {
            Ok(s) => s,
            Err(_) => {
                let err_msg = alloc_c_string("dir_create_all: invalid path string");
                return create_result_enum_err_string(err_msg);
            }
        };

        match std::fs::create_dir_all(path) {
            Ok(_) => create_result_enum_ok_bool(true),
            Err(e) => {
                let err_msg = alloc_c_string(&format!("dir_create_all failed: {}", e));
                create_result_enum_err_string(err_msg)
            }
        }
    }
}

/// Remove an empty directory
/// Returns Result<Bool, String>
#[no_mangle]
pub extern "C" fn plat_dir_remove(path_ptr: *const c_char) -> i64 {
    unsafe {
        if path_ptr.is_null() {
            let err_msg = alloc_c_string("dir_remove: path is null");
            return create_result_enum_err_string(err_msg);
        }

        let path = match CStr::from_ptr(path_ptr).to_str() {
            Ok(s) => s,
            Err(_) => {
                let err_msg = alloc_c_string("dir_remove: invalid path string");
                return create_result_enum_err_string(err_msg);
            }
        };

        match std::fs::remove_dir(path) {
            Ok(_) => create_result_enum_ok_bool(true),
            Err(e) => {
                let err_msg = alloc_c_string(&format!("dir_remove failed: {}", e));
                create_result_enum_err_string(err_msg)
            }
        }
    }
}

/// List directory contents (newline-separated file/directory names)
/// Returns Result<String, String>
#[no_mangle]
pub extern "C" fn plat_dir_list(path_ptr: *const c_char) -> i64 {
    unsafe {
        if path_ptr.is_null() {
            let err_msg = alloc_c_string("dir_list: path is null");
            return create_result_enum_err_string(err_msg);
        }

        let path = match CStr::from_ptr(path_ptr).to_str() {
            Ok(s) => s,
            Err(_) => {
                let err_msg = alloc_c_string("dir_list: invalid path string");
                return create_result_enum_err_string(err_msg);
            }
        };

        match std::fs::read_dir(path) {
            Ok(entries) => {
                let mut names = Vec::new();

                for entry in entries {
                    match entry {
                        Ok(e) => {
                            if let Some(name) = e.file_name().to_str() {
                                names.push(name.to_string());
                            }
                        }
                        Err(e) => {
                            let err_msg = alloc_c_string(&format!("dir_list: error reading entry: {}", e));
                            return create_result_enum_err_string(err_msg);
                        }
                    }
                }

                let result = names.join("\n");
                let c_str = alloc_c_string(&result);
                create_result_enum_ok_string(c_str)
            }
            Err(e) => {
                let err_msg = alloc_c_string(&format!("dir_list failed: {}", e));
                create_result_enum_err_string(err_msg)
            }
        }
    }
}

/// Read binary data from file
/// Returns Result<List[Int8], String>
#[no_mangle]
pub extern "C" fn plat_file_read_binary(fd: i32, max_bytes: i32) -> i64 {
    unsafe {
        let mut files = FILES.lock().unwrap();

        if let Some(file) = files.get_mut(&fd) {
            let mut buffer = vec![0u8; max_bytes as usize];

            match file.read(&mut buffer) {
                Ok(bytes_read) => {
                    buffer.truncate(bytes_read);

                    // Convert Vec<u8> to Vec<i8> for List[Int8]
                    let i8_buffer: Vec<i8> = buffer.into_iter().map(|b| b as i8).collect();

                    // Create array using plat_array_create_i8
                    let array_ptr = plat_array_create_i8(i8_buffer.as_ptr(), i8_buffer.len());

                    if array_ptr.is_null() {
                        let err_msg = alloc_c_string("file_read_binary: failed to allocate array");
                        return create_result_enum_err_string(err_msg);
                    }

                    create_result_enum_ok_list_i8(array_ptr)
                }
                Err(e) => {
                    let err_msg = alloc_c_string(&format!("file_read_binary failed: {}", e));
                    create_result_enum_err_string(err_msg)
                }
            }
        } else {
            let err_msg = alloc_c_string("file_read_binary: invalid file descriptor");
            create_result_enum_err_string(err_msg)
        }
    }
}

/// Write binary data to file
/// Returns Result<Int32, String> where Int32 is the number of bytes written
#[no_mangle]
pub extern "C" fn plat_file_write_binary(fd: i32, array_ptr: *const RuntimeArray) -> i64 {
    unsafe {
        if array_ptr.is_null() {
            let err_msg = alloc_c_string("file_write_binary: array is null");
            return create_result_enum_err_string(err_msg);
        }

        let array = &*array_ptr;

        // Verify this is an Int8 array
        if array.element_type != super::array::ARRAY_TYPE_I8 {
            let err_msg = alloc_c_string("file_write_binary: array must be List[Int8]");
            return create_result_enum_err_string(err_msg);
        }

        // Convert i8 data to u8 for writing
        let i8_slice = std::slice::from_raw_parts(array.data as *const i8, array.length);
        let u8_vec: Vec<u8> = i8_slice.iter().map(|&b| b as u8).collect();

        let mut files = FILES.lock().unwrap();

        if let Some(file) = files.get_mut(&fd) {
            match file.write(&u8_vec) {
                Ok(bytes_written) => {
                    // Ensure data is flushed to disk
                    if let Err(e) = file.flush() {
                        let err_msg = alloc_c_string(&format!("file_write_binary: failed to flush: {}", e));
                        return create_result_enum_err_string(err_msg);
                    }
                    create_result_enum_ok_i32(bytes_written as i32)
                }
                Err(e) => {
                    let err_msg = alloc_c_string(&format!("file_write_binary failed: {}", e));
                    create_result_enum_err_string(err_msg)
                }
            }
        } else {
            let err_msg = alloc_c_string("file_write_binary: invalid file descriptor");
            create_result_enum_err_string(err_msg)
        }
    }
}
