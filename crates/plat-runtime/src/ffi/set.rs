use std::ffi::CStr;
use std::os::raw::c_char;
use super::core::plat_gc_alloc;

// Set type constants
pub const SET_VALUE_TYPE_I32: u8 = 0;
pub const SET_VALUE_TYPE_I64: u8 = 1;
pub const SET_VALUE_TYPE_BOOL: u8 = 2;
pub const SET_VALUE_TYPE_STRING: u8 = 3;

/// Set structure for runtime (C-compatible)
/// Using vector-based implementation with type information
#[repr(C)]
pub struct RuntimeSet {
    pub(crate) values: *mut i64,               // Array of values (stored as i64)
    pub(crate) value_types: *mut u8,          // Array indicating the type of each value
    pub(crate) length: usize,
    pub(crate) capacity: usize,
}

/// Create a new set on the GC heap
#[no_mangle]
pub extern "C" fn plat_set_create(
    values: *const i64,
    value_types: *const u8,
    count: usize
) -> *mut RuntimeSet {
    if (values.is_null() || value_types.is_null()) && count > 0 {
        return std::ptr::null_mut();
    }

    // Allocate the set struct
    let set_size = std::mem::size_of::<RuntimeSet>();
    let set_ptr = plat_gc_alloc(set_size) as *mut RuntimeSet;

    if set_ptr.is_null() {
        return std::ptr::null_mut();
    }

    // Allocate space for values and types arrays
    let values_size = count * std::mem::size_of::<i64>();
    let types_size = count * std::mem::size_of::<u8>();

    let values_ptr = if count > 0 {
        plat_gc_alloc(values_size) as *mut i64
    } else {
        std::ptr::null_mut()
    };

    let types_ptr = if count > 0 {
        plat_gc_alloc(types_size) as *mut u8
    } else {
        std::ptr::null_mut()
    };

    // Copy data if provided and arrays allocated successfully
    if count > 0 && !values_ptr.is_null() && !types_ptr.is_null() {
        unsafe {
            // For Set, we need to deduplicate values
            let mut unique_values: Vec<(i64, u8)> = Vec::new();

            for i in 0..count {
                let value = *values.add(i);
                let value_type = *value_types.add(i);

                // Check if this value is already in the set
                let mut already_exists = false;
                for (existing_value, existing_type) in &unique_values {
                    if *existing_value == value && *existing_type == value_type {
                        already_exists = true;
                        break;
                    }
                }

                if !already_exists {
                    unique_values.push((value, value_type));
                }
            }

            // Copy the unique values
            for (i, (value, value_type)) in unique_values.iter().enumerate() {
                *values_ptr.add(i) = *value;
                *types_ptr.add(i) = *value_type;
            }

            // Initialize the set struct with unique count
            (*set_ptr) = RuntimeSet {
                values: values_ptr,
                value_types: types_ptr,
                length: unique_values.len(),
                capacity: unique_values.len(),
            };
        }
    } else {
        // Initialize empty set
        unsafe {
            (*set_ptr) = RuntimeSet {
                values: values_ptr,
                value_types: types_ptr,
                length: 0,
                capacity: 0,
            };
        }
    }

    set_ptr
}

/// Check if a value is in the set
#[no_mangle]
pub extern "C" fn plat_set_contains(set_ptr: *const RuntimeSet, value: i64, value_type: u8) -> bool {
    if set_ptr.is_null() {
        return false;
    }

    unsafe {
        let set = &*set_ptr;
        if set.values.is_null() || set.value_types.is_null() {
            return false;
        }

        // Linear search for the value
        for i in 0..set.length {
            let set_value = *set.values.add(i);
            let set_value_type = *set.value_types.add(i);
            if set_value == value && set_value_type == value_type {
                return true;
            }
        }

        false
    }
}

/// Get the length of a set
#[no_mangle]
pub extern "C" fn plat_set_len(set_ptr: *const RuntimeSet) -> usize {
    if set_ptr.is_null() {
        return 0;
    }

    unsafe {
        (*set_ptr).length
    }
}

/// Convert a set to a string for interpolation
#[no_mangle]
pub extern "C" fn plat_set_to_string(set_ptr: *const RuntimeSet) -> *const c_char {
    if set_ptr.is_null() {
        return std::ptr::null();
    }

    unsafe {
        let set = &*set_ptr;

        let mut result = String::from("{");

        for i in 0..set.length {
            if i > 0 {
                result.push_str(", ");
            }

            // Get value based on type
            if !set.values.is_null() && !set.value_types.is_null() {
                let value = *set.values.add(i);
                let value_type = *set.value_types.add(i);

                match value_type {
                    SET_VALUE_TYPE_I32 => {
                        result.push_str(&(value as i32).to_string());
                    },
                    SET_VALUE_TYPE_I64 => {
                        result.push_str(&value.to_string());
                    },
                    SET_VALUE_TYPE_BOOL => {
                        result.push_str(if value != 0 { "true" } else { "false" });
                    },
                    SET_VALUE_TYPE_STRING => {
                        let string_ptr = value as *const c_char;
                        if !string_ptr.is_null() {
                            if let Ok(string_val) = CStr::from_ptr(string_ptr).to_str() {
                                result.push('"');
                                result.push_str(string_val);
                                result.push('"');
                            } else {
                                result.push_str("\"<invalid>\"");
                            }
                        } else {
                            result.push_str("\"<null>\"");
                        }
                    },
                    _ => {
                        result.push_str("<unknown>");
                    }
                }
            }
        }

        result.push('}');

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

/// Add a value to a set (returns true if value was added, false if already exists)
#[no_mangle]
pub extern "C" fn plat_set_add(set_ptr: *mut RuntimeSet, value: i64, value_type: u8) -> bool {
    if set_ptr.is_null() {
        return false;
    }

    unsafe {
        let set = &mut *set_ptr;

        // Check if value already exists
        if plat_set_contains(set_ptr, value, value_type) {
            return false; // Value already exists
        }

        // Need to expand capacity
        let new_capacity = if set.capacity == 0 { 1 } else { set.capacity * 2 };

        // Allocate new arrays
        let new_values_size = new_capacity * std::mem::size_of::<i64>();
        let new_types_size = new_capacity * std::mem::size_of::<u8>();

        let new_values_ptr = plat_gc_alloc(new_values_size) as *mut i64;
        let new_types_ptr = plat_gc_alloc(new_types_size) as *mut u8;

        if new_values_ptr.is_null() || new_types_ptr.is_null() {
            return false;
        }

        // Copy existing data
        if set.length > 0 && !set.values.is_null() && !set.value_types.is_null() {
            std::ptr::copy_nonoverlapping(set.values, new_values_ptr, set.length);
            std::ptr::copy_nonoverlapping(set.value_types, new_types_ptr, set.length);
        }

        // Add new value
        *new_values_ptr.add(set.length) = value;
        *new_types_ptr.add(set.length) = value_type;

        // Update set
        set.values = new_values_ptr;
        set.value_types = new_types_ptr;
        set.length += 1;
        set.capacity = new_capacity;

        true
    }
}

/// Remove a value from a set (returns true if value was removed, false if not found)
#[no_mangle]
pub extern "C" fn plat_set_remove(set_ptr: *mut RuntimeSet, value: i64, value_type: u8) -> bool {
    if set_ptr.is_null() {
        return false;
    }

    unsafe {
        let set = &mut *set_ptr;
        if set.values.is_null() || set.value_types.is_null() {
            return false;
        }

        // Find the value to remove
        for i in 0..set.length {
            let set_value = *set.values.add(i);
            let set_value_type = *set.value_types.add(i);

            if set_value == value && set_value_type == value_type {
                // Found the value, remove it by shifting remaining elements
                for j in i..set.length - 1 {
                    *set.values.add(j) = *set.values.add(j + 1);
                    *set.value_types.add(j) = *set.value_types.add(j + 1);
                }
                set.length -= 1;
                return true;
            }
        }

        false // Value not found
    }
}

/// Clear all values from a set
#[no_mangle]
pub extern "C" fn plat_set_clear(set_ptr: *mut RuntimeSet) {
    if set_ptr.is_null() {
        return;
    }

    unsafe {
        let set = &mut *set_ptr;
        set.length = 0;
        // Note: We keep the allocated memory for reuse
    }
}

/// Get length of a set (alias for plat_set_len for consistency with other collections)
#[no_mangle]
pub extern "C" fn plat_set_length(set_ptr: *const RuntimeSet) -> i32 {
    plat_set_len(set_ptr) as i32
}

/// Create a union of two sets (returns new set)
#[no_mangle]
pub extern "C" fn plat_set_union(set1_ptr: *const RuntimeSet, set2_ptr: *const RuntimeSet) -> *mut RuntimeSet {
    if set1_ptr.is_null() || set2_ptr.is_null() {
        return std::ptr::null_mut();
    }

    unsafe {
        let set1 = &*set1_ptr;
        let set2 = &*set2_ptr;

        // Create a temporary vector to collect all unique values
        let mut union_values: Vec<(i64, u8)> = Vec::new();

        // Add all values from set1
        for i in 0..set1.length {
            if !set1.values.is_null() && !set1.value_types.is_null() {
                let value = *set1.values.add(i);
                let value_type = *set1.value_types.add(i);
                union_values.push((value, value_type));
            }
        }

        // Add values from set2 that aren't already in union
        for i in 0..set2.length {
            if !set2.values.is_null() && !set2.value_types.is_null() {
                let value = *set2.values.add(i);
                let value_type = *set2.value_types.add(i);

                // Check if this value is already in union
                let mut already_exists = false;
                for (existing_value, existing_type) in &union_values {
                    if *existing_value == value && *existing_type == value_type {
                        already_exists = true;
                        break;
                    }
                }

                if !already_exists {
                    union_values.push((value, value_type));
                }
            }
        }

        // Create new set with union values
        if union_values.is_empty() {
            plat_set_create(std::ptr::null(), std::ptr::null(), 0)
        } else {
            let values: Vec<i64> = union_values.iter().map(|(v, _)| *v).collect();
            let types: Vec<u8> = union_values.iter().map(|(_, t)| *t).collect();
            plat_set_create(values.as_ptr(), types.as_ptr(), union_values.len())
        }
    }
}

/// Create an intersection of two sets (returns new set)
#[no_mangle]
pub extern "C" fn plat_set_intersection(set1_ptr: *const RuntimeSet, set2_ptr: *const RuntimeSet) -> *mut RuntimeSet {
    if set1_ptr.is_null() || set2_ptr.is_null() {
        return std::ptr::null_mut();
    }

    unsafe {
        let set1 = &*set1_ptr;
        let _set2 = &*set2_ptr;

        // Create a temporary vector to collect intersection values
        let mut intersection_values: Vec<(i64, u8)> = Vec::new();

        // Check each value in set1 to see if it exists in set2
        for i in 0..set1.length {
            if !set1.values.is_null() && !set1.value_types.is_null() {
                let value = *set1.values.add(i);
                let value_type = *set1.value_types.add(i);

                // Check if this value exists in set2
                if plat_set_contains(set2_ptr, value, value_type) {
                    intersection_values.push((value, value_type));
                }
            }
        }

        // Create new set with intersection values
        if intersection_values.is_empty() {
            plat_set_create(std::ptr::null(), std::ptr::null(), 0)
        } else {
            let values: Vec<i64> = intersection_values.iter().map(|(v, _)| *v).collect();
            let types: Vec<u8> = intersection_values.iter().map(|(_, t)| *t).collect();
            plat_set_create(values.as_ptr(), types.as_ptr(), intersection_values.len())
        }
    }
}

/// Create a difference of two sets (set1 - set2, returns new set)
#[no_mangle]
pub extern "C" fn plat_set_difference(set1_ptr: *const RuntimeSet, set2_ptr: *const RuntimeSet) -> *mut RuntimeSet {
    if set1_ptr.is_null() || set2_ptr.is_null() {
        return std::ptr::null_mut();
    }

    unsafe {
        let set1 = &*set1_ptr;

        // Create a temporary vector to collect difference values
        let mut difference_values: Vec<(i64, u8)> = Vec::new();

        // Check each value in set1 to see if it does NOT exist in set2
        for i in 0..set1.length {
            if !set1.values.is_null() && !set1.value_types.is_null() {
                let value = *set1.values.add(i);
                let value_type = *set1.value_types.add(i);

                // Include value if it's NOT in set2
                if !plat_set_contains(set2_ptr, value, value_type) {
                    difference_values.push((value, value_type));
                }
            }
        }

        // Create new set with difference values
        if difference_values.is_empty() {
            plat_set_create(std::ptr::null(), std::ptr::null(), 0)
        } else {
            let values: Vec<i64> = difference_values.iter().map(|(v, _)| *v).collect();
            let types: Vec<u8> = difference_values.iter().map(|(_, t)| *t).collect();
            plat_set_create(values.as_ptr(), types.as_ptr(), difference_values.len())
        }
    }
}

/// Check if set1 is a subset of set2
#[no_mangle]
pub extern "C" fn plat_set_is_subset_of(set1_ptr: *const RuntimeSet, set2_ptr: *const RuntimeSet) -> bool {
    if set1_ptr.is_null() || set2_ptr.is_null() {
        return false;
    }

    unsafe {
        let set1 = &*set1_ptr;

        // Empty set is a subset of any set
        if set1.length == 0 {
            return true;
        }

        // Check that every element in set1 exists in set2
        for i in 0..set1.length {
            if !set1.values.is_null() && !set1.value_types.is_null() {
                let value = *set1.values.add(i);
                let value_type = *set1.value_types.add(i);

                if !plat_set_contains(set2_ptr, value, value_type) {
                    return false; // Found element in set1 that's not in set2
                }
            }
        }

        true
    }
}

/// Check if set1 is a superset of set2
#[no_mangle]
pub extern "C" fn plat_set_is_superset_of(set1_ptr: *const RuntimeSet, set2_ptr: *const RuntimeSet) -> bool {
    // set1 is a superset of set2 if set2 is a subset of set1
    plat_set_is_subset_of(set2_ptr, set1_ptr)
}

/// Check if two sets are disjoint (have no common elements)
#[no_mangle]
pub extern "C" fn plat_set_is_disjoint_from(set1_ptr: *const RuntimeSet, set2_ptr: *const RuntimeSet) -> bool {
    if set1_ptr.is_null() || set2_ptr.is_null() {
        return true;
    }

    unsafe {
        let set1 = &*set1_ptr;

        // Check if any element from set1 exists in set2
        for i in 0..set1.length {
            if !set1.values.is_null() && !set1.value_types.is_null() {
                let value = *set1.values.add(i);
                let value_type = *set1.value_types.add(i);

                if plat_set_contains(set2_ptr, value, value_type) {
                    return false; // Found common element
                }
            }
        }

        true // No common elements found
    }
}
