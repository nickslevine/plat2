use std::time::{SystemTime, UNIX_EPOCH};
use std::thread;

/// Get the current Unix timestamp in milliseconds
#[no_mangle]
pub extern "C" fn plat_time_now() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis() as i64,
        Err(_) => 0, // Return 0 on error (should be extremely rare)
    }
}

/// Sleep for the specified number of milliseconds
#[no_mangle]
pub extern "C" fn plat_time_sleep(millis: i64) {
    if millis > 0 {
        thread::sleep(std::time::Duration::from_millis(millis as u64));
    }
}
