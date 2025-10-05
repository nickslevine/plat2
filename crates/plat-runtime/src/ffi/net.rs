use std::ffi::CStr;
use std::os::raw::c_char;
use std::net::{TcpListener, TcpStream};
use std::io::{Read, Write};
use std::sync::Mutex;
use std::collections::HashMap;
use super::core::{plat_gc_alloc, plat_gc_alloc_atomic};

// Global socket storage
// Maps file descriptor (i32) to either TcpListener or TcpStream
lazy_static::lazy_static! {
    static ref LISTENERS: Mutex<HashMap<i32, TcpListener>> = Mutex::new(HashMap::new());
    static ref STREAMS: Mutex<HashMap<i32, TcpStream>> = Mutex::new(HashMap::new());
    static ref NEXT_FD: Mutex<i32> = Mutex::new(1000); // Start at 1000 to avoid conflicts
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

/// Create a TCP listener bound to host:port
/// Returns Result<Int32, String> where Int32 is the file descriptor
#[no_mangle]
pub extern "C" fn plat_tcp_listen(host_ptr: *const c_char, port: i32) -> i64 {
    unsafe {
        if host_ptr.is_null() {
            let err_msg = alloc_c_string("tcp_listen: host is null");
            return create_result_enum_err_string(err_msg);
        }

        let host = match CStr::from_ptr(host_ptr).to_str() {
            Ok(s) => s,
            Err(_) => {
                let err_msg = alloc_c_string("tcp_listen: invalid host string");
                return create_result_enum_err_string(err_msg);
            }
        };

        let addr = format!("{}:{}", host, port);
        match TcpListener::bind(&addr) {
            Ok(listener) => {
                let fd = next_fd();
                LISTENERS.lock().unwrap().insert(fd, listener);
                create_result_enum_ok_i32(fd)
            }
            Err(e) => {
                let err_msg = alloc_c_string(&format!("tcp_listen failed: {}", e));
                create_result_enum_err_string(err_msg)
            }
        }
    }
}

/// Accept a connection on the listener
/// Returns Result<Int32, String> where Int32 is the client socket file descriptor
#[no_mangle]
pub extern "C" fn plat_tcp_accept(listener_fd: i32) -> i64 {
    unsafe {
        let listeners = LISTENERS.lock().unwrap();

        if let Some(listener) = listeners.get(&listener_fd) {
            // We need to clone the listener to avoid borrow checker issues
            // This is safe because TcpListener can be cloned
            match listener.try_clone() {
                Ok(cloned_listener) => {
                    drop(listeners); // Release the lock before blocking

                    match cloned_listener.accept() {
                        Ok((stream, _addr)) => {
                            let fd = next_fd();
                            STREAMS.lock().unwrap().insert(fd, stream);
                            create_result_enum_ok_i32(fd)
                        }
                        Err(e) => {
                            let err_msg = alloc_c_string(&format!("tcp_accept failed: {}", e));
                            create_result_enum_err_string(err_msg)
                        }
                    }
                }
                Err(e) => {
                    let err_msg = alloc_c_string(&format!("tcp_accept: failed to clone listener: {}", e));
                    create_result_enum_err_string(err_msg)
                }
            }
        } else {
            let err_msg = alloc_c_string("tcp_accept: invalid listener file descriptor");
            create_result_enum_err_string(err_msg)
        }
    }
}

/// Connect to host:port
/// Returns Result<Int32, String> where Int32 is the socket file descriptor
#[no_mangle]
pub extern "C" fn plat_tcp_connect(host_ptr: *const c_char, port: i32) -> i64 {
    unsafe {
        if host_ptr.is_null() {
            let err_msg = alloc_c_string("tcp_connect: host is null");
            return create_result_enum_err_string(err_msg);
        }

        let host = match CStr::from_ptr(host_ptr).to_str() {
            Ok(s) => s,
            Err(_) => {
                let err_msg = alloc_c_string("tcp_connect: invalid host string");
                return create_result_enum_err_string(err_msg);
            }
        };

        let addr = format!("{}:{}", host, port);
        match TcpStream::connect(&addr) {
            Ok(stream) => {
                let fd = next_fd();
                STREAMS.lock().unwrap().insert(fd, stream);
                create_result_enum_ok_i32(fd)
            }
            Err(e) => {
                let err_msg = alloc_c_string(&format!("tcp_connect failed: {}", e));
                create_result_enum_err_string(err_msg)
            }
        }
    }
}

/// Read up to max_bytes from socket
/// Returns Result<String, String>
#[no_mangle]
pub extern "C" fn plat_tcp_read(socket_fd: i32, max_bytes: i32) -> i64 {
    unsafe {
        let mut streams = STREAMS.lock().unwrap();

        if let Some(stream) = streams.get_mut(&socket_fd) {
            let mut buffer = vec![0u8; max_bytes as usize];

            match stream.read(&mut buffer) {
                Ok(bytes_read) => {
                    buffer.truncate(bytes_read);

                    // Convert bytes to string (handle UTF-8 properly)
                    match String::from_utf8(buffer) {
                        Ok(s) => {
                            let c_str = alloc_c_string(&s);
                            create_result_enum_ok_string(c_str)
                        }
                        Err(_) => {
                            let err_msg = alloc_c_string("tcp_read: received invalid UTF-8 data");
                            create_result_enum_err_string(err_msg)
                        }
                    }
                }
                Err(e) => {
                    let err_msg = alloc_c_string(&format!("tcp_read failed: {}", e));
                    create_result_enum_err_string(err_msg)
                }
            }
        } else {
            let err_msg = alloc_c_string("tcp_read: invalid socket file descriptor");
            create_result_enum_err_string(err_msg)
        }
    }
}

/// Write data to socket
/// Returns Result<Int32, String> where Int32 is the number of bytes written
#[no_mangle]
pub extern "C" fn plat_tcp_write(socket_fd: i32, data_ptr: *const c_char) -> i64 {
    unsafe {
        if data_ptr.is_null() {
            let err_msg = alloc_c_string("tcp_write: data is null");
            return create_result_enum_err_string(err_msg);
        }

        let data = match CStr::from_ptr(data_ptr).to_str() {
            Ok(s) => s,
            Err(_) => {
                let err_msg = alloc_c_string("tcp_write: invalid data string");
                return create_result_enum_err_string(err_msg);
            }
        };

        let mut streams = STREAMS.lock().unwrap();

        if let Some(stream) = streams.get_mut(&socket_fd) {
            match stream.write(data.as_bytes()) {
                Ok(bytes_written) => {
                    create_result_enum_ok_i32(bytes_written as i32)
                }
                Err(e) => {
                    let err_msg = alloc_c_string(&format!("tcp_write failed: {}", e));
                    create_result_enum_err_string(err_msg)
                }
            }
        } else {
            let err_msg = alloc_c_string("tcp_write: invalid socket file descriptor");
            create_result_enum_err_string(err_msg)
        }
    }
}

/// Close socket
/// Returns Result<Bool, String>
#[no_mangle]
pub extern "C" fn plat_tcp_close(socket_fd: i32) -> i64 {
    unsafe {
        // Try to remove from streams first
        if STREAMS.lock().unwrap().remove(&socket_fd).is_some() {
            return create_result_enum_ok_bool(true);
        }

        // Try to remove from listeners
        if LISTENERS.lock().unwrap().remove(&socket_fd).is_some() {
            return create_result_enum_ok_bool(true);
        }

        // Socket not found
        let err_msg = alloc_c_string("tcp_close: invalid socket file descriptor");
        create_result_enum_err_string(err_msg)
    }
}
