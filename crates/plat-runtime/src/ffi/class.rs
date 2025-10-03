use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use crate::types::{PlatClass, PlatValue, PlatString};
use gc::Gc;

/// Create a new class instance
#[no_mangle]
pub extern "C" fn plat_class_create(class_name_ptr: *const c_char) -> *mut PlatClass {
    if class_name_ptr.is_null() {
        return std::ptr::null_mut();
    }

    unsafe {
        let class_name = CStr::from_ptr(class_name_ptr).to_string_lossy().to_string();
        let class_instance = Box::new(PlatClass::new(class_name));
        Box::into_raw(class_instance)
    }
}

/// Set a field value in a class instance
#[no_mangle]
pub extern "C" fn plat_class_set_field_i32(
    class_ptr: *mut PlatClass,
    field_name_ptr: *const c_char,
    value: i32
) {
    if class_ptr.is_null() || field_name_ptr.is_null() {
        return;
    }

    unsafe {
        let class = &mut *class_ptr;
        let field_name = CStr::from_ptr(field_name_ptr).to_string_lossy().to_string();
        class.set_field(field_name, PlatValue::I32(value));
    }
}

/// Set a string field value in a class instance
#[no_mangle]
pub extern "C" fn plat_class_set_field_string(
    class_ptr: *mut PlatClass,
    field_name_ptr: *const c_char,
    value_ptr: *const c_char
) {
    if class_ptr.is_null() || field_name_ptr.is_null() || value_ptr.is_null() {
        return;
    }

    unsafe {
        let class = &mut *class_ptr;
        let field_name = CStr::from_ptr(field_name_ptr).to_string_lossy().to_string();
        let value_str = CStr::from_ptr(value_ptr).to_string_lossy().to_string();
        let plat_string = PlatString { data: Gc::new(value_str) };
        class.set_field(field_name, PlatValue::String(plat_string));
    }
}

/// Get an i32 field value from a class instance
#[no_mangle]
pub extern "C" fn plat_class_get_field_i32(
    class_ptr: *const PlatClass,
    field_name_ptr: *const c_char
) -> i32 {
    if class_ptr.is_null() || field_name_ptr.is_null() {
        return 0;
    }

    unsafe {
        let class = &*class_ptr;
        let field_name = CStr::from_ptr(field_name_ptr).to_string_lossy();
        match class.get_field(&field_name) {
            Some(PlatValue::I32(value)) => value,
            _ => 0, // Return default if field not found or wrong type
        }
    }
}

/// Get a string field value from a class instance (returns pointer to C string)
#[no_mangle]
pub extern "C" fn plat_class_get_field_string(
    class_ptr: *const PlatClass,
    field_name_ptr: *const c_char
) -> *const c_char {
    if class_ptr.is_null() || field_name_ptr.is_null() {
        return std::ptr::null();
    }

    unsafe {
        let class = &*class_ptr;
        let field_name = CStr::from_ptr(field_name_ptr).to_string_lossy();
        match class.get_field(&field_name) {
            Some(PlatValue::String(ref plat_string)) => {
                let c_string = CString::new(plat_string.data.as_str()).unwrap();
                c_string.into_raw() // Caller is responsible for freeing
            },
            _ => std::ptr::null(),
        }
    }
}

/// Convert class instance to string representation
#[no_mangle]
pub extern "C" fn plat_class_to_string(class_ptr: *const PlatClass) -> *const c_char {
    if class_ptr.is_null() {
        return std::ptr::null();
    }

    unsafe {
        let class = &*class_ptr;
        let string_repr = format!("{}", class);
        let c_string = CString::new(string_repr).unwrap();
        c_string.into_raw() // Caller is responsible for freeing
    }
}

// =============================================================================
// Point Class Method Implementations
// =============================================================================

/// Point::add method - adds two Point instances and returns a new Point
#[no_mangle]
pub extern "C" fn Point__add(self_ptr: *const PlatClass, other_ptr: *const PlatClass) -> *mut PlatClass {
    if self_ptr.is_null() || other_ptr.is_null() {
        return std::ptr::null_mut();
    }

    unsafe {
        let self_point = &*self_ptr;
        let other_point = &*other_ptr;

        // Get x and y values from both points
        let self_x = match self_point.get_field("x") {
            Some(PlatValue::I32(x)) => x,
            _ => 0,
        };
        let self_y = match self_point.get_field("y") {
            Some(PlatValue::I32(y)) => y,
            _ => 0,
        };

        let other_x = match other_point.get_field("x") {
            Some(PlatValue::I32(x)) => x,
            _ => 0,
        };
        let other_y = match other_point.get_field("y") {
            Some(PlatValue::I32(y)) => y,
            _ => 0,
        };

        // Create new point with sum
        let new_point = Box::new(PlatClass::new("Point".to_string()));
        let new_point_ptr = Box::into_raw(new_point);

        // Set fields for new point
        plat_class_set_field_i32(new_point_ptr, CString::new("x").unwrap().as_ptr(), self_x + other_x);
        plat_class_set_field_i32(new_point_ptr, CString::new("y").unwrap().as_ptr(), self_y + other_y);
        plat_class_set_field_string(new_point_ptr, CString::new("name").unwrap().as_ptr(), CString::new("sum").unwrap().as_ptr());

        new_point_ptr
    }
}

/// Point::change_name method - changes the name field of the point
#[no_mangle]
pub extern "C" fn Point__change_name(self_ptr: *mut PlatClass, new_name_ptr: *const c_char) {
    if self_ptr.is_null() || new_name_ptr.is_null() {
        return;
    }

    // SAFETY: plat_class_set_field_string is already an unsafe function
    plat_class_set_field_string(self_ptr, CString::new("name").unwrap().as_ptr(), new_name_ptr);
}

/// Point::get_magnitude method - calculates x*x + y*y
#[no_mangle]
pub extern "C" fn Point__get_magnitude(self_ptr: *const PlatClass) -> i32 {
    if self_ptr.is_null() {
        return 0;
    }

    unsafe {
        let point = &*self_ptr;

        let x = match point.get_field("x") {
            Some(PlatValue::I32(x)) => x,
            _ => 0,
        };
        let y = match point.get_field("y") {
            Some(PlatValue::I32(y)) => y,
            _ => 0,
        };

        x * x + y * y
    }
}
