use std::env;
use std::ffi::CString;
use std::os::raw::c_char;

/// Exit the process with the given exit code
#[no_mangle]
pub extern "C" fn plat_process_exit(code: i32) -> ! {
    std::process::exit(code)
}

/// Get command-line arguments as a newline-separated string
/// Returns null pointer on error
#[no_mangle]
pub extern "C" fn plat_process_args() -> *mut c_char {
    let args: Vec<String> = env::args().collect();
    let result = args.join("\n");

    match CString::new(result) {
        Ok(c_string) => c_string.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}
