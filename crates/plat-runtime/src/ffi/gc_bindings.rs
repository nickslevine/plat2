use std::os::raw::c_void;

// Boehm GC C API declarations
extern "C" {
    /// Initialize the garbage collector (optional, auto-initializes on first alloc)
    pub fn GC_init();

    /// Allocate memory on GC heap (can return null on OOM)
    pub fn GC_malloc(size: usize) -> *mut c_void;

    /// Allocate atomic (pointer-free) memory - faster, no scanning
    pub fn GC_malloc_atomic(size: usize) -> *mut c_void;

    /// Explicitly trigger garbage collection
    pub fn GC_gcollect();

    /// Get heap size in bytes
    pub fn GC_get_heap_size() -> usize;

    /// Get total bytes allocated
    pub fn GC_get_total_bytes() -> usize;

    /// Get free bytes in heap
    pub fn GC_get_free_bytes() -> usize;

    /// Enable/disable GC
    pub fn GC_enable();
    pub fn GC_disable();
}

/// Safety wrapper for GC initialization
pub fn init_gc() {
    unsafe {
        GC_init();
    }
}

/// Safety wrapper for GC allocation
///
/// # Arguments
/// * `size` - Number of bytes to allocate
/// * `atomic` - If true, allocate pointer-free memory (optimization)
///
/// # Returns
/// Pointer to allocated memory, or null on OOM
pub fn gc_alloc(size: usize, atomic: bool) -> *mut u8 {
    unsafe {
        let ptr = if atomic {
            GC_malloc_atomic(size)
        } else {
            GC_malloc(size)
        };
        ptr as *mut u8
    }
}

/// Trigger explicit garbage collection
pub fn gc_collect() {
    unsafe {
        GC_gcollect();
    }
}

/// Get GC statistics
pub fn gc_stats() -> GCStats {
    unsafe {
        GCStats {
            heap_size: GC_get_heap_size(),
            total_bytes: GC_get_total_bytes(),
            free_bytes: GC_get_free_bytes(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct GCStats {
    pub heap_size: usize,
    pub total_bytes: usize,
    pub free_bytes: usize,
}
