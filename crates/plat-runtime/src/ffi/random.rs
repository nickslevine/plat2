use rand::Rng;

/// Generate a random integer in the range [min, max]
#[no_mangle]
pub extern "C" fn plat_random_int(min: i64, max: i64) -> i64 {
    if min > max {
        return min;
    }

    let mut rng = rand::thread_rng();
    rng.gen_range(min..=max)
}

/// Generate a random float in the range [0.0, 1.0)
#[no_mangle]
pub extern "C" fn plat_random_float() -> f64 {
    let mut rng = rand::thread_rng();
    rng.gen()
}
