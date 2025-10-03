use std::ffi::CStr;
use std::os::raw::c_char;
use super::core::plat_gc_alloc;

// Array element type constants
pub const ARRAY_TYPE_I32: u8 = 0;
pub const ARRAY_TYPE_I64: u8 = 1;
pub const ARRAY_TYPE_BOOL: u8 = 2;
pub const ARRAY_TYPE_STRING: u8 = 3;
pub const ARRAY_TYPE_CLASS: u8 = 4; // Custom class pointers (8 bytes like strings)

/// Array structure for runtime (C-compatible)
/// Generic data pointer that can hold any type
#[repr(C)]
pub struct RuntimeArray {
    pub(crate) data: *mut u8, // Generic byte pointer for any type
    pub(crate) length: usize,
    pub(crate) capacity: usize,
    pub(crate) element_size: usize, // Size of each element in bytes
    pub(crate) element_type: u8, // Type discriminant: 0=i32, 1=i64, 2=bool, 3=string
}

/// Create a new i32 array on the GC heap
#[no_mangle]
pub extern "C" fn plat_array_create_i32(elements: *const i32, count: usize) -> *mut RuntimeArray {
    create_typed_array(elements as *const u8, count, std::mem::size_of::<i32>(), ARRAY_TYPE_I32)
}

/// Create a new i64 array on the GC heap
#[no_mangle]
pub extern "C" fn plat_array_create_i64(elements: *const i64, count: usize) -> *mut RuntimeArray {
    create_typed_array(elements as *const u8, count, std::mem::size_of::<i64>(), ARRAY_TYPE_I64)
}

/// Create a new bool array on the GC heap
#[no_mangle]
pub extern "C" fn plat_array_create_bool(elements: *const bool, count: usize) -> *mut RuntimeArray {
    create_typed_array(elements as *const u8, count, std::mem::size_of::<bool>(), ARRAY_TYPE_BOOL)
}

/// Create a new string array on the GC heap
#[no_mangle]
pub extern "C" fn plat_array_create_string(elements: *const *const c_char, count: usize) -> *mut RuntimeArray {
    create_typed_array(elements as *const u8, count, std::mem::size_of::<*const c_char>(), ARRAY_TYPE_STRING)
}

/// Create a new class array on the GC heap (custom class pointers)
#[no_mangle]
pub extern "C" fn plat_array_create_class(elements: *const *const u8, count: usize) -> *mut RuntimeArray {
    create_typed_array(elements as *const u8, count, std::mem::size_of::<*const u8>(), ARRAY_TYPE_CLASS)
}

/// Generic array creation helper
fn create_typed_array(elements: *const u8, count: usize, element_size: usize, element_type: u8) -> *mut RuntimeArray {
    if elements.is_null() && count > 0 {
        return std::ptr::null_mut();
    }

    // Allocate the array struct
    let array_size = std::mem::size_of::<RuntimeArray>();
    let array_ptr = plat_gc_alloc(array_size) as *mut RuntimeArray;

    if array_ptr.is_null() {
        return std::ptr::null_mut();
    }

    // Allocate space for the data
    let data_size = count * element_size;
    let data_ptr = if count > 0 {
        plat_gc_alloc(data_size)
    } else {
        std::ptr::null_mut()
    };

    if count > 0 && data_ptr.is_null() {
        return std::ptr::null_mut();
    }

    // Copy elements to the data array
    if count > 0 {
        unsafe {
            std::ptr::copy_nonoverlapping(elements, data_ptr, data_size);
        }
    }

    // Initialize the array struct
    unsafe {
        (*array_ptr) = RuntimeArray {
            data: data_ptr,
            length: count,
            capacity: count,
            element_size,
            element_type,
        };
    }

    array_ptr
}

/// Legacy function for backward compatibility
#[no_mangle]
pub extern "C" fn plat_array_create(elements: *const i32, count: usize) -> *mut RuntimeArray {
    plat_array_create_i32(elements, count)
}

/// Legacy function that returns the appropriate type based on array discriminant
/// Returns as i64 to handle all types uniformly (bool fits in i32, strings return pointer)
#[no_mangle]
pub extern "C" fn plat_array_get(array_ptr: *const RuntimeArray, index: usize) -> i64 {
    if array_ptr.is_null() {
        return 0;
    }

    unsafe {
        let array = &*array_ptr;
        if index >= array.length || array.data.is_null() {
            return 0;
        }

        match array.element_type {
            ARRAY_TYPE_I32 => {
                let data_ptr = array.data as *const i32;
                *data_ptr.add(index) as i64
            },
            ARRAY_TYPE_I64 => {
                let data_ptr = array.data as *const i64;
                *data_ptr.add(index)
            },
            ARRAY_TYPE_BOOL => {
                let data_ptr = array.data as *const bool;
                if *data_ptr.add(index) { 1 } else { 0 }
            },
            ARRAY_TYPE_STRING => {
                let data_ptr = array.data as *const *const c_char;
                *data_ptr.add(index) as i64
            },
            ARRAY_TYPE_CLASS => {
                let data_ptr = array.data as *const *const u8;
                *data_ptr.add(index) as i64
            },
            _ => 0,
        }
    }
}

/// Get the length of an array
///
/// # Safety
/// This function works with raw pointers from generated code
#[no_mangle]
pub extern "C" fn plat_array_len(array_ptr: *const RuntimeArray) -> usize {
    if array_ptr.is_null() {
        return 0;
    }

    unsafe {
        (*array_ptr).length
    }
}

/// Convert an array to a string for interpolation
///
/// # Safety
/// This function works with raw pointers and returns GC memory
#[no_mangle]
pub extern "C" fn plat_array_to_string(array_ptr: *const RuntimeArray) -> *const c_char {
    if array_ptr.is_null() {
        return std::ptr::null();
    }

    unsafe {
        let array = &*array_ptr;

        let mut result = String::from("[");

        for i in 0..array.length {
            if i > 0 {
                result.push_str(", ");
            }
            if !array.data.is_null() {
                match array.element_type {
                    ARRAY_TYPE_I32 => {
                        let data_ptr = array.data as *const i32;
                        let value = *data_ptr.add(i);
                        result.push_str(&value.to_string());
                    },
                    ARRAY_TYPE_I64 => {
                        let data_ptr = array.data as *const i64;
                        let value = *data_ptr.add(i);
                        result.push_str(&value.to_string());
                    },
                    ARRAY_TYPE_BOOL => {
                        let data_ptr = array.data as *const bool;
                        let value = *data_ptr.add(i);
                        result.push_str(if value { "true" } else { "false" });
                    },
                    ARRAY_TYPE_STRING => {
                        let data_ptr = array.data as *const *const c_char;
                        let string_ptr = *data_ptr.add(i);
                        if !string_ptr.is_null() {
                            let c_str = std::ffi::CStr::from_ptr(string_ptr);
                            if let Ok(str_slice) = c_str.to_str() {
                                result.push('"');
                                result.push_str(str_slice);
                                result.push('"');
                            } else {
                                result.push_str("\"<invalid>\"");
                            }
                        } else {
                            result.push_str("\"<null>\"");
                        }
                    },
                    ARRAY_TYPE_CLASS => {
                        let data_ptr = array.data as *const *const u8;
                        let class_ptr = *data_ptr.add(i);
                        // For class instances, show pointer address
                        result.push_str(&format!("<instance@{:p}>", class_ptr));
                    },
                    _ => {
                        result.push_str("<unknown>");
                    }
                }
            }
        }

        result.push(']');

        // Allocate result on GC heap
        let mut result_bytes = result.into_bytes();
        result_bytes.push(0); // null terminator

        let size = result_bytes.len();
        let gc_ptr = plat_gc_alloc(size);

        if gc_ptr.is_null() {
            return std::ptr::null();
        }

        // Copy result to GC memory
        std::ptr::copy_nonoverlapping(result_bytes.as_ptr(), gc_ptr, size);

        gc_ptr as *const c_char
    }
}

/// Get the length of an array (alias for plat_array_len)
#[no_mangle]
pub extern "C" fn plat_array_length(array_ptr: *const RuntimeArray) -> i32 {
    plat_array_len(array_ptr) as i32
}

/// Safely get an element from array, returns Option<T> encoded as (found: bool, value: i64)
#[no_mangle]
pub extern "C" fn plat_array_get_safe(array_ptr: *const RuntimeArray, index: i32) -> (bool, i64) {
    if array_ptr.is_null() || index < 0 {
        return (false, 0);
    }

    unsafe {
        let array = &*array_ptr;
        let index = index as usize;

        if index >= array.length || array.data.is_null() {
            return (false, 0);
        }

        let value = match array.element_type {
            ARRAY_TYPE_I32 => {
                let data_ptr = array.data as *const i32;
                *data_ptr.add(index) as i64
            },
            ARRAY_TYPE_I64 => {
                let data_ptr = array.data as *const i64;
                *data_ptr.add(index)
            },
            ARRAY_TYPE_BOOL => {
                let data_ptr = array.data as *const bool;
                if *data_ptr.add(index) { 1 } else { 0 }
            },
            ARRAY_TYPE_STRING => {
                let data_ptr = array.data as *const *const c_char;
                *data_ptr.add(index) as i64
            },
            ARRAY_TYPE_CLASS => {
                let data_ptr = array.data as *const *const u8;
                *data_ptr.add(index) as i64
            },
            _ => return (false, 0),
        };

        (true, value)
    }
}

/// Set an element in array at given index (mutates array)
#[no_mangle]
pub extern "C" fn plat_array_set(array_ptr: *mut RuntimeArray, index: i32, value: i64) -> bool {
    if array_ptr.is_null() || index < 0 {
        return false;
    }

    unsafe {
        let array = &mut *array_ptr;
        let index = index as usize;

        if index >= array.length || array.data.is_null() {
            return false;
        }

        match array.element_type {
            ARRAY_TYPE_I32 => {
                let data_ptr = array.data as *mut i32;
                *data_ptr.add(index) = value as i32;
            },
            ARRAY_TYPE_I64 => {
                let data_ptr = array.data as *mut i64;
                *data_ptr.add(index) = value;
            },
            ARRAY_TYPE_BOOL => {
                let data_ptr = array.data as *mut bool;
                *data_ptr.add(index) = value != 0;
            },
            ARRAY_TYPE_STRING => {
                let data_ptr = array.data as *mut *const c_char;
                *data_ptr.add(index) = value as *const c_char;
            },
            ARRAY_TYPE_CLASS => {
                let data_ptr = array.data as *mut *const u8;
                *data_ptr.add(index) = value as *const u8;
            },
            _ => return false,
        };

        true
    }
}

/// Append element to end of array (reallocates if needed)
#[no_mangle]
pub extern "C" fn plat_array_append(array_ptr: *mut RuntimeArray, value: i64) -> bool {
    if array_ptr.is_null() {
        return false;
    }

    unsafe {
        let array = &mut *array_ptr;

        // Check if we need to grow the array
        if array.length >= array.capacity {
            let new_capacity = if array.capacity == 0 { 4 } else { array.capacity * 2 };
            let new_size = new_capacity * array.element_size;
            let new_data_ptr = plat_gc_alloc(new_size);

            if new_data_ptr.is_null() {
                return false;
            }

            // Copy existing data
            if array.length > 0 && !array.data.is_null() {
                let old_size = array.length * array.element_size;
                std::ptr::copy_nonoverlapping(array.data, new_data_ptr, old_size);
            }

            array.data = new_data_ptr;
            array.capacity = new_capacity;
        }

        // Add the new element
        match array.element_type {
            ARRAY_TYPE_I32 => {
                let data_ptr = array.data as *mut i32;
                *data_ptr.add(array.length) = value as i32;
            },
            ARRAY_TYPE_I64 => {
                let data_ptr = array.data as *mut i64;
                *data_ptr.add(array.length) = value;
            },
            ARRAY_TYPE_BOOL => {
                let data_ptr = array.data as *mut bool;
                *data_ptr.add(array.length) = value != 0;
            },
            ARRAY_TYPE_STRING => {
                let data_ptr = array.data as *mut *const c_char;
                *data_ptr.add(array.length) = value as *const c_char;
            },
            ARRAY_TYPE_CLASS => {
                let data_ptr = array.data as *mut *const u8;
                *data_ptr.add(array.length) = value as *const u8;
            },
            _ => return false,
        };

        array.length += 1;
        true
    }
}

/// Insert element at specific index (shifts elements right)
#[no_mangle]
pub extern "C" fn plat_array_insert_at(array_ptr: *mut RuntimeArray, index: i32, value: i64) -> bool {
    if array_ptr.is_null() || index < 0 {
        return false;
    }

    unsafe {
        let array = &mut *array_ptr;
        let index = index as usize;

        if index > array.length {
            return false; // Can't insert beyond length
        }

        // Check if we need to grow the array
        if array.length >= array.capacity {
            let new_capacity = if array.capacity == 0 { 4 } else { array.capacity * 2 };
            let new_size = new_capacity * array.element_size;
            let new_data_ptr = plat_gc_alloc(new_size);

            if new_data_ptr.is_null() {
                return false;
            }

            // Copy existing data
            if array.length > 0 && !array.data.is_null() {
                let old_size = array.length * array.element_size;
                std::ptr::copy_nonoverlapping(array.data, new_data_ptr, old_size);
            }

            array.data = new_data_ptr;
            array.capacity = new_capacity;
        }

        // Shift elements right from insertion point
        if index < array.length {
            let elements_to_move = array.length - index;
            match array.element_type {
                ARRAY_TYPE_I32 => {
                    let data_ptr = array.data as *mut i32;
                    std::ptr::copy(data_ptr.add(index), data_ptr.add(index + 1), elements_to_move);
                },
                ARRAY_TYPE_I64 => {
                    let data_ptr = array.data as *mut i64;
                    std::ptr::copy(data_ptr.add(index), data_ptr.add(index + 1), elements_to_move);
                },
                ARRAY_TYPE_BOOL => {
                    let data_ptr = array.data as *mut bool;
                    std::ptr::copy(data_ptr.add(index), data_ptr.add(index + 1), elements_to_move);
                },
                ARRAY_TYPE_STRING => {
                    let data_ptr = array.data as *mut *const c_char;
                    std::ptr::copy(data_ptr.add(index), data_ptr.add(index + 1), elements_to_move);
                },
                ARRAY_TYPE_CLASS => {
                    let data_ptr = array.data as *mut *const u8;
                    std::ptr::copy(data_ptr.add(index), data_ptr.add(index + 1), elements_to_move);
                },
                _ => return false,
            }
        }

        // Insert the new element
        match array.element_type {
            ARRAY_TYPE_I32 => {
                let data_ptr = array.data as *mut i32;
                *data_ptr.add(index) = value as i32;
            },
            ARRAY_TYPE_I64 => {
                let data_ptr = array.data as *mut i64;
                *data_ptr.add(index) = value;
            },
            ARRAY_TYPE_BOOL => {
                let data_ptr = array.data as *mut bool;
                *data_ptr.add(index) = value != 0;
            },
            ARRAY_TYPE_STRING => {
                let data_ptr = array.data as *mut *const c_char;
                *data_ptr.add(index) = value as *const c_char;
            },
            ARRAY_TYPE_CLASS => {
                let data_ptr = array.data as *mut *const u8;
                *data_ptr.add(index) = value as *const u8;
            },
            _ => return false,
        };

        array.length += 1;
        true
    }
}

/// Remove element at specific index, returns Option<T> encoded as (found: bool, value: i64)
#[no_mangle]
pub extern "C" fn plat_array_remove_at(array_ptr: *mut RuntimeArray, index: i32) -> (bool, i64) {
    if array_ptr.is_null() || index < 0 {
        return (false, 0);
    }

    unsafe {
        let array = &mut *array_ptr;
        let index = index as usize;

        if index >= array.length || array.data.is_null() {
            return (false, 0);
        }

        // Get the value being removed
        let removed_value = match array.element_type {
            ARRAY_TYPE_I32 => {
                let data_ptr = array.data as *const i32;
                *data_ptr.add(index) as i64
            },
            ARRAY_TYPE_I64 => {
                let data_ptr = array.data as *const i64;
                *data_ptr.add(index)
            },
            ARRAY_TYPE_BOOL => {
                let data_ptr = array.data as *const bool;
                if *data_ptr.add(index) { 1 } else { 0 }
            },
            ARRAY_TYPE_STRING => {
                let data_ptr = array.data as *const *const c_char;
                *data_ptr.add(index) as i64
            },
            ARRAY_TYPE_CLASS => {
                let data_ptr = array.data as *const *const u8;
                *data_ptr.add(index) as i64
            },
            _ => return (false, 0),
        };

        // Shift elements left to fill the gap
        if index < array.length - 1 {
            let elements_to_move = array.length - index - 1;
            match array.element_type {
                ARRAY_TYPE_I32 => {
                    let data_ptr = array.data as *mut i32;
                    std::ptr::copy(data_ptr.add(index + 1), data_ptr.add(index), elements_to_move);
                },
                ARRAY_TYPE_I64 => {
                    let data_ptr = array.data as *mut i64;
                    std::ptr::copy(data_ptr.add(index + 1), data_ptr.add(index), elements_to_move);
                },
                ARRAY_TYPE_BOOL => {
                    let data_ptr = array.data as *mut bool;
                    std::ptr::copy(data_ptr.add(index + 1), data_ptr.add(index), elements_to_move);
                },
                ARRAY_TYPE_STRING => {
                    let data_ptr = array.data as *mut *const c_char;
                    std::ptr::copy(data_ptr.add(index + 1), data_ptr.add(index), elements_to_move);
                },
                ARRAY_TYPE_CLASS => {
                    let data_ptr = array.data as *mut *const u8;
                    std::ptr::copy(data_ptr.add(index + 1), data_ptr.add(index), elements_to_move);
                },
                _ => return (false, 0),
            }
        }

        array.length -= 1;
        (true, removed_value)
    }
}

/// Clear all elements from array
#[no_mangle]
pub extern "C" fn plat_array_clear(array_ptr: *mut RuntimeArray) -> bool {
    if array_ptr.is_null() {
        return false;
    }

    unsafe {
        let array = &mut *array_ptr;
        array.length = 0;
        true
    }
}

/// Check if array contains a specific value
#[no_mangle]
pub extern "C" fn plat_array_contains(array_ptr: *const RuntimeArray, value: i64) -> bool {
    if array_ptr.is_null() {
        return false;
    }

    unsafe {
        let array = &*array_ptr;
        if array.data.is_null() {
            return false;
        }

        for i in 0..array.length {
            let element_value = match array.element_type {
                ARRAY_TYPE_I32 => {
                    let data_ptr = array.data as *const i32;
                    *data_ptr.add(i) as i64
                },
                ARRAY_TYPE_I64 => {
                    let data_ptr = array.data as *const i64;
                    *data_ptr.add(i)
                },
                ARRAY_TYPE_BOOL => {
                    let data_ptr = array.data as *const bool;
                    if *data_ptr.add(i) { 1 } else { 0 }
                },
                ARRAY_TYPE_STRING => {
                    let data_ptr = array.data as *const *const c_char;
                    *data_ptr.add(i) as i64
                },
            ARRAY_TYPE_CLASS => {
                let data_ptr = array.data as *const *const u8;
                *data_ptr.add(i) as i64
            },
            _ => continue,
            };

            if element_value == value {
                return true;
            }
        }

        false
    }
}

/// Find index of first occurrence of value, returns Option<i32> encoded as (found: bool, index: i32)
#[no_mangle]
pub extern "C" fn plat_array_index_of(array_ptr: *const RuntimeArray, value: i64) -> (bool, i32) {
    if array_ptr.is_null() {
        return (false, -1);
    }

    unsafe {
        let array = &*array_ptr;
        if array.data.is_null() {
            return (false, -1);
        }

        for i in 0..array.length {
            let element_value = match array.element_type {
                ARRAY_TYPE_I32 => {
                    let data_ptr = array.data as *const i32;
                    *data_ptr.add(i) as i64
                },
                ARRAY_TYPE_I64 => {
                    let data_ptr = array.data as *const i64;
                    *data_ptr.add(i)
                },
                ARRAY_TYPE_BOOL => {
                    let data_ptr = array.data as *const bool;
                    if *data_ptr.add(i) { 1 } else { 0 }
                },
                ARRAY_TYPE_STRING => {
                    let data_ptr = array.data as *const *const c_char;
                    *data_ptr.add(i) as i64
                },
            ARRAY_TYPE_CLASS => {
                let data_ptr = array.data as *const *const u8;
                *data_ptr.add(i) as i64
            },
            _ => continue,
            };

            if element_value == value {
                return (true, i as i32);
            }
        }

        (false, -1)
    }
}

/// Count occurrences of value in array
#[no_mangle]
pub extern "C" fn plat_array_count(array_ptr: *const RuntimeArray, value: i64) -> i32 {
    if array_ptr.is_null() {
        return 0;
    }

    unsafe {
        let array = &*array_ptr;
        if array.data.is_null() {
            return 0;
        }

        let mut count = 0;
        for i in 0..array.length {
            let element_value = match array.element_type {
                ARRAY_TYPE_I32 => {
                    let data_ptr = array.data as *const i32;
                    *data_ptr.add(i) as i64
                },
                ARRAY_TYPE_I64 => {
                    let data_ptr = array.data as *const i64;
                    *data_ptr.add(i)
                },
                ARRAY_TYPE_BOOL => {
                    let data_ptr = array.data as *const bool;
                    if *data_ptr.add(i) { 1 } else { 0 }
                },
                ARRAY_TYPE_STRING => {
                    let data_ptr = array.data as *const *const c_char;
                    *data_ptr.add(i) as i64
                },
            ARRAY_TYPE_CLASS => {
                let data_ptr = array.data as *const *const u8;
                *data_ptr.add(i) as i64
            },
            _ => continue,
            };

            if element_value == value {
                count += 1;
            }
        }

        count
    }
}

/// Create a slice of array from start to end (exclusive)
#[no_mangle]
pub extern "C" fn plat_array_slice(array_ptr: *const RuntimeArray, start: i32, end: i32) -> *mut RuntimeArray {
    if array_ptr.is_null() || start < 0 || end < start {
        return std::ptr::null_mut();
    }

    unsafe {
        let array = &*array_ptr;
        let start = start as usize;
        let end = end as usize;

        if start >= array.length || end > array.length || array.data.is_null() {
            return std::ptr::null_mut();
        }

        let slice_length = end - start;

        // Create new array with sliced elements
        match array.element_type {
            ARRAY_TYPE_I32 => {
                let data_ptr = array.data as *const i32;
                plat_array_create_i32(data_ptr.add(start), slice_length)
            },
            ARRAY_TYPE_I64 => {
                let data_ptr = array.data as *const i64;
                plat_array_create_i64(data_ptr.add(start), slice_length)
            },
            ARRAY_TYPE_BOOL => {
                let data_ptr = array.data as *const bool;
                plat_array_create_bool(data_ptr.add(start), slice_length)
            },
            ARRAY_TYPE_STRING => {
                let data_ptr = array.data as *const *const c_char;
                plat_array_create_string(data_ptr.add(start), slice_length)
            },
            ARRAY_TYPE_CLASS => {
                let data_ptr = array.data as *const *const u8;
                plat_array_create_class(data_ptr.add(start), slice_length)
            },
            _ => std::ptr::null_mut(),
        }
    }
}

/// Concatenate two arrays of the same type
#[no_mangle]
pub extern "C" fn plat_array_concat(array1_ptr: *const RuntimeArray, array2_ptr: *const RuntimeArray) -> *mut RuntimeArray {
    if array1_ptr.is_null() || array2_ptr.is_null() {
        return std::ptr::null_mut();
    }

    unsafe {
        let array1 = &*array1_ptr;
        let array2 = &*array2_ptr;

        // Arrays must be same type
        if array1.element_type != array2.element_type {
            return std::ptr::null_mut();
        }

        let total_length = array1.length + array2.length;

        // Allocate new array
        let array_size = std::mem::size_of::<RuntimeArray>();
        let new_array_ptr = plat_gc_alloc(array_size) as *mut RuntimeArray;

        if new_array_ptr.is_null() {
            return std::ptr::null_mut();
        }

        // Allocate data for combined array
        let data_size = total_length * array1.element_size;
        let new_data_ptr = if total_length > 0 {
            plat_gc_alloc(data_size)
        } else {
            std::ptr::null_mut()
        };

        if total_length > 0 && new_data_ptr.is_null() {
            return std::ptr::null_mut();
        }

        // Copy data from both arrays
        if total_length > 0 && !array1.data.is_null() && !array2.data.is_null() {
            let size1 = array1.length * array1.element_size;
            let size2 = array2.length * array2.element_size;

            std::ptr::copy_nonoverlapping(array1.data, new_data_ptr, size1);
            std::ptr::copy_nonoverlapping(array2.data, new_data_ptr.add(size1), size2);
        }

        // Initialize new array
        (*new_array_ptr) = RuntimeArray {
            data: new_data_ptr,
            length: total_length,
            capacity: total_length,
            element_size: array1.element_size,
            element_type: array1.element_type,
        };

        new_array_ptr
    }
}

/// Check if all elements satisfy predicate (simplified: check if all elements are non-zero/true)
#[no_mangle]
pub extern "C" fn plat_array_all_truthy(array_ptr: *const RuntimeArray) -> bool {
    if array_ptr.is_null() {
        return true; // Empty arrays are vacuously true
    }

    unsafe {
        let array = &*array_ptr;
        if array.data.is_null() || array.length == 0 {
            return true;
        }

        for i in 0..array.length {
            let element_value = match array.element_type {
                ARRAY_TYPE_I32 => {
                    let data_ptr = array.data as *const i32;
                    *data_ptr.add(i) as i64
                },
                ARRAY_TYPE_I64 => {
                    let data_ptr = array.data as *const i64;
                    *data_ptr.add(i)
                },
                ARRAY_TYPE_BOOL => {
                    let data_ptr = array.data as *const bool;
                    if *data_ptr.add(i) { 1 } else { 0 }
                },
                ARRAY_TYPE_STRING => {
                    let data_ptr = array.data as *const *const c_char;
                    let str_ptr = *data_ptr.add(i);
                    if str_ptr.is_null() { 0 } else { 1 }
                },
                ARRAY_TYPE_CLASS => {
                    let data_ptr = array.data as *const *const u8;
                    let class_ptr = *data_ptr.add(i);
                    if class_ptr.is_null() { 0 } else { 1 }
                },
                _ => 0,
            };

            if element_value == 0 {
                return false;
            }
        }

        true
    }
}

/// Check if any element satisfies predicate (simplified: check if any element is non-zero/true)
#[no_mangle]
pub extern "C" fn plat_array_any_truthy(array_ptr: *const RuntimeArray) -> bool {
    if array_ptr.is_null() {
        return false;
    }

    unsafe {
        let array = &*array_ptr;
        if array.data.is_null() || array.length == 0 {
            return false;
        }

        for i in 0..array.length {
            let element_value = match array.element_type {
                ARRAY_TYPE_I32 => {
                    let data_ptr = array.data as *const i32;
                    *data_ptr.add(i) as i64
                },
                ARRAY_TYPE_I64 => {
                    let data_ptr = array.data as *const i64;
                    *data_ptr.add(i)
                },
                ARRAY_TYPE_BOOL => {
                    let data_ptr = array.data as *const bool;
                    if *data_ptr.add(i) { 1 } else { 0 }
                },
                ARRAY_TYPE_STRING => {
                    let data_ptr = array.data as *const *const c_char;
                    let str_ptr = *data_ptr.add(i);
                    if str_ptr.is_null() { 0 } else { 1 }
                },
                ARRAY_TYPE_CLASS => {
                    let data_ptr = array.data as *const *const u8;
                    let class_ptr = *data_ptr.add(i);
                    if class_ptr.is_null() { 0 } else { 1 }
                },
                _ => 0,
            };

            if element_value != 0 {
                return true;
            }
        }

        false
    }
}
