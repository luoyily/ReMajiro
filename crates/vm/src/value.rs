use std::rc::Rc;
pub const TAG_INT: u32 = 0;
pub const TAG_FLOAT: u32 = 1;
pub const TAG_STRING: u32 = 2;
pub const TAG_INT_ARRAY: u32 = 3;
pub const TAG_FIXED_INT_ARRAY: u32 = 4;
pub const TAG_STRING_ARRAY: u32 = 5;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum Scope {
    Global = 0,
    Local = 1,
    Thread = 2,
    Stack = 3,
}
impl Scope {
    pub fn from_tag(tag: u16) -> Self {
        match (tag >> 5) & 7 {
            0 => Scope::Global,
            1 => Scope::Local,
            2 => Scope::Thread,
            _ => Scope::Stack,
        }
    }
}
#[derive(Debug, Clone)]
pub enum ValueData {
    Str(Vec<u8>),
    StringArray { dims: [u32; 3], cells: Vec<Value> },
    IntArray { ndim: u8, dims: [u32; 3], cells: Vec<Value> },
}
impl ValueData {
    fn display_string_lossy(&self) -> String {
        match self {
            ValueData::Str(b) => {
                let (cow, _, _) = encoding_rs::SHIFT_JIS.decode(b);
                cow.into_owned()
            }
            ValueData::StringArray { .. } => "<string-array>".to_string(),
            ValueData::IntArray { .. } => "<int-array>".to_string(),
        }
    }
}
#[derive(Clone)]
pub struct Value {
    pub scope: Scope,
    pub type_tag: u32,
    pub bits: u32,
    pub data: Option<Rc<ValueData>>,
}
impl std::fmt::Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.type_tag {
            TAG_INT => write!(f, "int({})", self.bits as i32),
            TAG_FLOAT => write!(f, "float({})", f32::from_bits(self.bits)),
            TAG_STRING => {
                match &self.data {
                    Some(d) => write!(f, "str({:?})", d.display_string_lossy()),
                    None => write!(f, "str(<null>)"),
                }
            }
            TAG_INT_ARRAY | TAG_FIXED_INT_ARRAY => {
                match self.data.as_deref() {
                    Some(ValueData::IntArray { dims, cells, .. }) => {
                        write!(f, "int_array({dims:?}, {} cells)", cells.len())
                    }
                    _ => write!(f, "int_array(<null>)"),
                }
            }
            TAG_STRING_ARRAY => write!(f, "string_array"),
            other => write!(f, "Value(tag={}, bits=0x{:08X})", other, self.bits),
        }
    }
}
impl Value {
    pub fn int(i: i32) -> Self {
        Value {
            scope: Scope::Stack,
            type_tag: TAG_INT,
            bits: i as u32,
            data: None,
        }
    }
    pub fn float(fl: f32) -> Self {
        Value {
            scope: Scope::Stack,
            type_tag: TAG_FLOAT,
            bits: fl.to_bits(),
            data: None,
        }
    }
    pub fn string(bytes: Vec<u8>) -> Self {
        Value {
            scope: Scope::Stack,
            type_tag: TAG_STRING,
            bits: 0,
            data: Some(Rc::new(ValueData::Str(bytes))),
        }
    }
    pub fn string_from_str(s: &str) -> Self {
        let (cow, _, _) = encoding_rs::SHIFT_JIS.encode(s);
        Self::string(cow.into_owned())
    }
    pub fn null() -> Self {
        Self::int(0)
    }
    pub fn string_array_empty() -> Self {
        Value {
            scope: Scope::Stack,
            type_tag: TAG_STRING_ARRAY,
            bits: 0,
            data: Some(
                Rc::new(ValueData::StringArray {
                    dims: [1, 1, 1],
                    cells: vec![Value::string(vec![0])],
                }),
            ),
        }
    }
    pub fn int_array(ndim: usize, dims: [u32; 3]) -> Self {
        let total = (0..ndim).map(|i| dims[i].max(1) as usize).product::<usize>().max(1);
        Value {
            scope: Scope::Stack,
            type_tag: TAG_INT_ARRAY,
            bits: 0,
            data: Some(
                Rc::new(ValueData::IntArray {
                    ndim: ndim as u8,
                    dims,
                    cells: vec![Value::null(); total],
                }),
            ),
        }
    }
    pub fn string_array(ndim: usize, dims: [u32; 3]) -> Self {
        let total = (0..ndim).map(|i| dims[i].max(1) as usize).product::<usize>().max(1);
        Value {
            scope: Scope::Stack,
            type_tag: TAG_STRING_ARRAY,
            bits: 0,
            data: Some(
                Rc::new(ValueData::StringArray {
                    dims,
                    cells: (0..total).map(|_| Value::string(vec![0])).collect(),
                }),
            ),
        }
    }
    pub fn as_int(&self) -> Option<i32> {
        if self.type_tag == TAG_INT { Some(self.bits as i32) } else { None }
    }
    pub fn as_float(&self) -> Option<f32> {
        if self.type_tag == TAG_FLOAT { Some(f32::from_bits(self.bits)) } else { None }
    }
    pub fn as_str_bytes(&self) -> Option<&[u8]> {
        if let Some(ValueData::Str(b)) = self.data.as_deref() { Some(b) } else { None }
    }
    pub fn truthy(&self) -> bool {
        match self.type_tag {
            TAG_INT => self.bits != 0,
            TAG_FLOAT => f32::from_bits(self.bits) != 0.0,
            TAG_STRING => self.as_str_bytes().map(|b| b.len() > 1).unwrap_or(false),
            _ => false,
        }
    }
}
#[derive(Debug, Default)]
pub struct Stack {
    vals: Vec<Value>,
}
impl Stack {
    pub fn new() -> Self {
        Self {
            vals: Vec::with_capacity(256),
        }
    }
    pub fn len(&self) -> usize {
        self.vals.len()
    }
    pub fn is_empty(&self) -> bool {
        self.vals.is_empty()
    }
    pub fn push(&mut self, v: Value) {
        self.vals.push(v);
    }
    pub fn pop(&mut self) -> Option<Value> {
        self.vals.pop()
    }
    pub fn peek(&self) -> Option<&Value> {
        self.vals.last()
    }
    pub fn peek_mut(&mut self) -> Option<&mut Value> {
        self.vals.last_mut()
    }
    pub fn peek_from_top(&self, n: usize) -> Option<&Value> {
        let len = self.vals.len();
        if n >= len {
            return None;
        }
        self.vals.get(len - 1 - n)
    }
    pub fn as_slice(&self) -> &[Value] {
        &self.vals
    }
    pub(crate) fn replace(&mut self, values: Vec<Value>) {
        self.vals = values;
        self.vals.reserve(256);
    }
    pub fn peek_slot_mut(&mut self, idx: usize) -> Option<&mut Value> {
        self.vals.get_mut(idx)
    }
    pub fn truncate(&mut self, new_len: usize) {
        self.vals.truncate(new_len);
    }
    pub fn slot(&self, idx: usize) -> Option<&Value> {
        self.vals.get(idx)
    }
}

