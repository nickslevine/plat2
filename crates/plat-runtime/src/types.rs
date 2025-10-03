use std::fmt;
use gc::{Gc, Trace, Finalize};

/// Runtime value types for the Plat language
#[derive(Debug, Clone, PartialEq, Trace, Finalize)]
pub enum PlatValue {
    Bool(bool),
    I32(i32),
    I64(i64),
    String(PlatString),
    Array(PlatArray),
    Dict(PlatDict),
    Set(PlatSet),
    Class(PlatClass),
    Unit,
}

/// GC-managed string type using gc crate
#[derive(Debug, Clone, Trace, Finalize)]
pub struct PlatString {
    pub(crate) data: Gc<String>,
}

impl PartialEq for PlatString {
    fn eq(&self, other: &Self) -> bool {
        self.data.as_str() == other.data.as_str()
    }
}

impl PlatString {
    pub fn new(s: String) -> Self {
        Self { data: Gc::new(s) }
    }

    pub fn from_str(s: &str) -> Self {
        Self { data: Gc::new(s.to_string()) }
    }

    pub fn as_str(&self) -> &str {
        self.data.as_str()
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// String concatenation
    pub fn concat(&self, other: &PlatString) -> PlatString {
        PlatString::new(format!("{}{}", self.data.as_str(), other.data.as_str()))
    }
}

impl fmt::Display for PlatString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.data.as_str())
    }
}

/// GC-managed homogeneous array type
#[derive(Debug, Clone, Trace, Finalize)]
pub struct PlatArray {
    pub(crate) data: Gc<Vec<i32>>, // For now, only support i32 arrays (we can extend later)
    pub(crate) length: usize,
}

impl PartialEq for PlatArray {
    fn eq(&self, other: &Self) -> bool {
        self.data.as_ref() == other.data.as_ref()
    }
}

impl PlatArray {
    pub fn new_i32(elements: Vec<i32>) -> Self {
        let length = elements.len();
        Self {
            data: Gc::new(elements),
            length,
        }
    }

    pub fn len(&self) -> usize {
        self.length
    }

    pub fn is_empty(&self) -> bool {
        self.length == 0
    }

    pub fn get(&self, index: usize) -> Option<i32> {
        self.data.as_ref().get(index).copied()
    }

    pub fn as_slice(&self) -> &[i32] {
        self.data.as_ref().as_slice()
    }
}

/// GC-managed dictionary type (using vector of key-value pairs for simplicity)
#[derive(Debug, Clone, Trace, Finalize)]
pub struct PlatDict {
    pub(crate) data: Gc<Vec<(String, PlatValue)>>,
}

impl PartialEq for PlatDict {
    fn eq(&self, other: &Self) -> bool {
        self.data.as_ref() == other.data.as_ref()
    }
}

impl PlatDict {
    pub fn new() -> Self {
        Self {
            data: Gc::new(Vec::new()),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            data: Gc::new(Vec::with_capacity(capacity)),
        }
    }

    pub fn from_pairs(pairs: Vec<(String, PlatValue)>) -> Self {
        Self {
            data: Gc::new(pairs),
        }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn get(&self, key: &str) -> Option<&PlatValue> {
        self.data.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.data.iter().map(|(k, _)| k)
    }

    pub fn values(&self) -> impl Iterator<Item = &PlatValue> {
        self.data.iter().map(|(_, v)| v)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &PlatValue)> {
        self.data.iter().map(|(k, v)| (k, v))
    }
}

impl fmt::Display for PlatDict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{")?;
        for (i, (key, value)) in self.data.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "\"{}\": {}", key, value)?;
        }
        write!(f, "}}")
    }
}

/// GC-managed set type (using vector for simplicity, maintaining uniqueness)
#[derive(Debug, Clone, Trace, Finalize)]
pub struct PlatSet {
    pub(crate) data: Gc<Vec<PlatValue>>,
}

impl PartialEq for PlatSet {
    fn eq(&self, other: &Self) -> bool {
        self.data.as_ref() == other.data.as_ref()
    }
}

impl PlatSet {
    pub fn new() -> Self {
        Self {
            data: Gc::new(Vec::new()),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            data: Gc::new(Vec::with_capacity(capacity)),
        }
    }

    pub fn insert(&mut self, value: PlatValue) {
        // Only insert if value is not already present
        if !self.data.contains(&value) {
            unsafe {
                let ptr = Gc::into_raw(self.data.clone()) as *mut Vec<PlatValue>;
                (*ptr).push(value);
                self.data = Gc::from_raw(ptr);
            }
        }
    }

    pub fn contains(&self, value: &PlatValue) -> bool {
        self.data.contains(value)
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &PlatValue> {
        self.data.iter()
    }

    pub fn as_slice(&self) -> &[PlatValue] {
        self.data.as_ref().as_slice()
    }
}

impl fmt::Display for PlatSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{")?;
        for (i, value) in self.data.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}", value)?;
        }
        write!(f, "}}")
    }
}

/// GC-managed class object for storing class instances with their fields
#[derive(Debug, Clone, Trace, Finalize)]
pub struct PlatClass {
    pub class_name: String,
    pub fields: Gc<std::collections::HashMap<String, PlatValue>>,
}

impl PartialEq for PlatClass {
    fn eq(&self, other: &Self) -> bool {
        self.class_name == other.class_name &&
        self.fields.as_ref() == other.fields.as_ref()
    }
}

impl PlatClass {
    pub fn new(class_name: String) -> Self {
        Self {
            class_name,
            fields: Gc::new(std::collections::HashMap::new()),
        }
    }

    pub fn get_field(&self, field_name: &str) -> Option<PlatValue> {
        self.fields.get(field_name).cloned()
    }

    pub fn set_field(&mut self, field_name: String, value: PlatValue) {
        unsafe {
            let ptr = Gc::into_raw(self.fields.clone()) as *mut std::collections::HashMap<String, PlatValue>;
            (*ptr).insert(field_name, value);
            self.fields = Gc::from_raw(ptr);
        }
    }
}

impl fmt::Display for PlatClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{{", self.class_name)?;
        for (i, (key, value)) in self.fields.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}: {}", key, value)?;
        }
        write!(f, "}}")
    }
}

impl fmt::Display for PlatValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlatValue::Bool(b) => write!(f, "{}", b),
            PlatValue::I32(i) => write!(f, "{}", i),
            PlatValue::I64(i) => write!(f, "{}", i),
            PlatValue::String(s) => write!(f, "{}", s),
            PlatValue::Array(arr) => {
                write!(f, "[")?;
                for (i, elem) in arr.as_slice().iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", elem)?;
                }
                write!(f, "]")
            }
            PlatValue::Dict(dict) => write!(f, "{}", dict),
            PlatValue::Set(set) => write!(f, "{}", set),
            PlatValue::Class(class) => write!(f, "{}", class),
            PlatValue::Unit => write!(f, "()"),
        }
    }
}

impl From<bool> for PlatValue {
    fn from(b: bool) -> Self {
        PlatValue::Bool(b)
    }
}

impl From<i32> for PlatValue {
    fn from(i: i32) -> Self {
        PlatValue::I32(i)
    }
}

impl From<i64> for PlatValue {
    fn from(i: i64) -> Self {
        PlatValue::I64(i)
    }
}

impl From<String> for PlatValue {
    fn from(s: String) -> Self {
        PlatValue::String(PlatString::new(s))
    }
}

impl From<PlatArray> for PlatValue {
    fn from(arr: PlatArray) -> Self {
        PlatValue::Array(arr)
    }
}

impl From<&str> for PlatValue {
    fn from(s: &str) -> Self {
        PlatValue::String(PlatString::from_str(s))
    }
}

impl From<PlatString> for PlatValue {
    fn from(s: PlatString) -> Self {
        PlatValue::String(s)
    }
}

impl From<PlatDict> for PlatValue {
    fn from(dict: PlatDict) -> Self {
        PlatValue::Dict(dict)
    }
}

impl From<PlatSet> for PlatValue {
    fn from(set: PlatSet) -> Self {
        PlatValue::Set(set)
    }
}
