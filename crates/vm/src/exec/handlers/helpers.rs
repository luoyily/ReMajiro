use crate::value::Value;
use std::rc::Rc;
#[derive(Clone, Copy)]
pub(super) struct InnerArgs<'a> {
    values: &'a [Value],
    len: usize,
}
impl<'a> InnerArgs<'a> {
    pub(super) fn new(values: &'a [Value], declared_count: usize) -> Self {
        Self {
            values,
            len: declared_count.min(values.len()),
        }
    }
    pub(super) fn len(self) -> usize {
        self.len
    }
    pub(super) fn top(self, n: usize) -> Option<&'a Value> {
        (n < self.len).then(|| self.values.get(self.len - 1 - n)).flatten()
    }
    pub(super) fn int(self, n: usize) -> Option<i32> {
        self.top(n).and_then(Value::as_int)
    }
    pub(super) fn int_or(self, n: usize, default: i32) -> i32 {
        self.int(n).unwrap_or(default)
    }
    pub(super) fn bytes(self, n: usize) -> Option<&'a [u8]> {
        self.top(n).and_then(Value::as_str_bytes)
    }
    pub(super) fn bytes_or_empty(self, n: usize) -> &'a [u8] {
        self.bytes(n).unwrap_or(&[])
    }
    pub(super) fn cstr(self, n: usize) -> &'a [u8] {
        let bytes = self.bytes_or_empty(n);
        bytes.split(|&byte| byte == 0).next().unwrap_or(bytes)
    }
}
pub(crate) fn sjis_mbsicmp(a: &[u8], b: &[u8]) -> i32 {
    let cmp = sjis_mbsicmp_bytes(a, b);
    if cmp != 0 && cross_encoding_equal(a, b) {
        return 0;
    }
    cmp
}
fn cross_encoding_equal(a: &[u8], b: &[u8]) -> bool {
    match (utf8_override_text(a), utf8_override_text(b)) {
        (Some(text_a), None) => text_a.eq_ignore_ascii_case(&decode_sjis_lossy(b)),
        (None, Some(text_b)) => text_b.eq_ignore_ascii_case(&decode_sjis_lossy(a)),
        _ => false,
    }
}
pub(crate) fn utf8_override_text(bytes: &[u8]) -> Option<&str> {
    let bytes = bytes.split(|&b| b == 0).next().unwrap_or(bytes);
    let text = std::str::from_utf8(bytes).ok()?;
    (!text.is_ascii()).then_some(text)
}
pub(crate) fn decode_sjis_lossy(bytes: &[u8]) -> String {
    let bytes = bytes.split(|&b| b == 0).next().unwrap_or(bytes);
    encoding_rs::SHIFT_JIS.decode(bytes).0.into_owned()
}
fn sjis_mbsicmp_bytes(a: &[u8], b: &[u8]) -> i32 {
    let a = match a.iter().position(|&c| c == 0) {
        Some(i) => &a[..i],
        None => a,
    };
    let b = match b.iter().position(|&c| c == 0) {
        Some(i) => &b[..i],
        None => b,
    };
    let mut ai = 0;
    let mut bi = 0;
    loop {
        let ca = next_sjis_folded(a, &mut ai);
        let cb = next_sjis_folded(b, &mut bi);
        if ca != cb {
            return if ca < cb { -1 } else { 1 };
        }
        if ca == 0 {
            return 0;
        }
    }
}
fn next_sjis_folded(bytes: &[u8], index: &mut usize) -> u16 {
    let Some(&first) = bytes.get(*index) else {
        return 0;
    };
    *index += 1;
    if is_sjis_lead(first) {
        let Some(&second) = bytes.get(*index) else {
            return 0;
        };
        *index += 1;
        u16::from(first) << 8 | u16::from(second)
    } else {
        u16::from(ascii_tolower(first))
    }
}
#[inline]
pub(crate) fn is_sjis_lead(byte: u8) -> bool {
    matches!(byte, 0x81..= 0x9f | 0xe0..= 0xfc)
}
#[inline]
pub(crate) fn ascii_tolower(c: u8) -> u8 {
    if c.is_ascii_uppercase() { c + 32 } else { c }
}
pub(crate) fn c_string_concat(left: &[u8], right: &[u8]) -> Vec<u8> {
    let left_end = left.iter().position(|&byte| byte == 0).unwrap_or(left.len());
    let right_end = right.iter().position(|&byte| byte == 0).unwrap_or(right.len());
    let terminated = left_end < left.len() || right_end < right.len();
    let mut result = Vec::with_capacity(left_end + right_end + usize::from(terminated));
    result.extend_from_slice(&left[..left_end]);
    result.extend_from_slice(&right[..right_end]);
    if terminated {
        result.push(0);
    }
    result
}
#[inline]
pub(crate) fn cur_f(v: &Value) -> f64 {
    match v.type_tag {
        crate::value::TAG_FLOAT => f32::from_bits(v.bits) as f64,
        _ => v.bits as i32 as f64,
    }
}
pub(crate) fn write_array_cell_inner(
    data: &mut Option<Rc<crate::value::ValueData>>,
    linear: usize,
    v: Value,
) {
    if let Some(rc) = data {
        match Rc::make_mut(rc) {
            crate::value::ValueData::IntArray { cells, .. } => {
                if linear >= cells.len() {
                    cells.resize(linear + 1, Value::null());
                }
                cells[linear] = v;
            }
            crate::value::ValueData::StringArray { cells, .. } => {
                if linear >= cells.len() {
                    cells.resize(linear + 1, Value::string(vec![0]));
                }
                cells[linear] = v;
            }
            crate::value::ValueData::Str(_) => {}
        }
    }
}
pub(crate) fn values_compare(a: &Value, b: &Value) -> i32 {
    use crate::value::{TAG_FLOAT, TAG_INT, TAG_STRING};
    match a.type_tag {
        TAG_INT => (a.bits as i32).wrapping_sub(b.bits as i32),
        TAG_FLOAT => {
            let af = f32::from_bits(a.bits) as f64;
            let bf = f32::from_bits(b.bits) as f64;
            if af < bf { -1 } else if af > bf { 1 } else { 0 }
        }
        TAG_STRING => {
            let ab = a.as_str_bytes().unwrap_or(&[]);
            let bb = b.as_str_bytes().unwrap_or(&[]);
            sjis_mbsicmp(ab, bb)
        }
        _ => 0,
    }
}
pub(crate) fn values_not_equal(a: &Value, b: &Value) -> bool {
    use crate::value::{TAG_FLOAT, TAG_INT, TAG_STRING};
    match a.type_tag {
        TAG_INT => a.bits != b.bits,
        TAG_FLOAT => f32::from_bits(a.bits) != f32::from_bits(b.bits),
        TAG_STRING => {
            let ab = a.as_str_bytes().unwrap_or(&[]);
            let bb = b.as_str_bytes().unwrap_or(&[]);
            sjis_mbsicmp(ab, bb) != 0
        }
        _ => true,
    }
}

