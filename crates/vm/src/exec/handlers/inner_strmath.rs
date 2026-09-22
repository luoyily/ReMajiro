use crate::exec::VmError;
use crate::host::Host;
use crate::value::Value;
use super::helpers::{decode_sjis_lossy, sjis_mbsicmp, utf8_override_text, InnerArgs};
use super::inner::InnerOutcome;
pub(super) const HASHES: &[u32] = &[
    0x00D696CC, 0x05EA6E4D, 0x06906970, 0x109CA5DB, 0x12877048, 0x134D5585, 0x1F57B724,
    0x25401A1F, 0x266A3C79, 0x3D053317, 0x3FE05B39, 0x45815F6D, 0x4694366A, 0x4F62152A,
    0x54EF8174, 0x69DB4512, 0x6CFC884A, 0x6F769889, 0x8AEF9167, 0x924EE3EB, 0x959B0A16,
    0x96B07AEE, 0xAD74D9AC, 0xB0C8F550, 0xC6282F19, 0xEE2B6D92,
];
const MAX_ARRAY_CELLS: usize = 1_000_000;
impl crate::exec::Vm {
    pub(crate) fn handle_text_first_line_localized<H: Host>(
        &mut self,
        hash: u32,
        host: &mut H,
    ) -> Option<Result<InnerOutcome, VmError>> {
        if hash != 0x05EA6E4D || !host.localized_first_line_enabled() {
            return None;
        }
        let mut bytes = self.text.localized_page_accumulator.clone();
        bytes.push(0);
        self.stack.push(Value::string(bytes));
        Some(Ok(InnerOutcome::Normal))
    }
    pub(super) fn handle_inner_strmath(
        &mut self,
        hash: u32,
        count: usize,
        _has_retval: bool,
        args: &[Value],
    ) -> Option<Result<InnerOutcome, VmError>> {
        let top = |k: usize| -> usize { count.saturating_sub(1).saturating_sub(k) };
        match hash {
            0x00D696CC => {
                let csv = InnerArgs::new(args, count).cstr(0);
                let fields = csv
                    .iter()
                    .filter(|&&byte| byte == b',')
                    .count()
                    .saturating_add(1)
                    .min(i32::MAX as usize) as i32;
                self.stack.push(Value::int(fields));
                Some(Ok(InnerOutcome::Normal))
            }
            0x134D5585 => {
                self.stack.push(Value::string_array_empty());
                Some(Ok(InnerOutcome::Normal))
            }
            0x4694366A => {
                let src = args
                    .get(top(0))
                    .and_then(Value::as_str_bytes)
                    .map(cstr_bytes)
                    .unwrap_or(&[]);
                self.stack
                    .push(Value::int(crate::formats_crc32(&mbcs_tolower(src)) as i32));
                Some(Ok(InnerOutcome::Normal))
            }
            0x45815F6D => {
                let left = args.get(top(1)).and_then(Value::as_str_bytes).unwrap_or(&[]);
                let right = args
                    .get(top(0))
                    .and_then(Value::as_str_bytes)
                    .unwrap_or(&[]);
                self.stack.push(Value::int(sjis_mbsicmp(left, right)));
                Some(Ok(InnerOutcome::Normal))
            }
            0x4F62152A => {
                let left = args
                    .get(top(0))
                    .and_then(Value::as_str_bytes)
                    .map(cstr_bytes)
                    .unwrap_or(&[]);
                let right = args
                    .get(top(1))
                    .and_then(Value::as_str_bytes)
                    .map(cstr_bytes)
                    .unwrap_or(&[]);
                let matched = native_tail_wildcard_compare(
                    &mbcs_tolower(left),
                    &mbcs_tolower(right),
                );
                self.stack.push(Value::int(if matched { -1 } else { 0 }));
                Some(Ok(InnerOutcome::Normal))
            }
            0xC6282F19 => {
                let bound = args.get(top(0)).map(|value| value.bits as i32).unwrap_or(0);
                let value = if bound == 0 {
                    0
                } else {
                    msvc_rand(&mut self.rng_state) % bound
                };
                self.stack.push(Value::int(value));
                Some(Ok(InnerOutcome::Normal))
            }
            0xEE2B6D92 | 0x12877048 => {
                const DEG_TO_RAD: f64 = f64::from_bits(0x3F91_DF46_A250_6B91);
                let degrees = args
                    .get(top(0))
                    .map(|value| f32::from_bits(value.bits))
                    .unwrap_or(0.0);
                let radians = f64::from(degrees) * DEG_TO_RAD;
                let value = if hash == 0xEE2B6D92 {
                    radians.sin()
                } else {
                    radians.cos()
                } as f32;
                self.stack.push(Value::float(value));
                Some(Ok(InnerOutcome::Normal))
            }
            0x959B0A16 => {
                let value = args
                    .get(top(0))
                    .and_then(Value::as_int)
                    .unwrap_or(0)
                    .wrapping_abs();
                self.stack.push(Value::int(value));
                Some(Ok(InnerOutcome::Normal))
            }
            0x3D053317 => {
                let value = args
                    .iter()
                    .take(count.min(args.len()))
                    .filter_map(Value::as_int)
                    .min()
                    .unwrap_or(0);
                self.stack.push(Value::int(value));
                Some(Ok(InnerOutcome::Normal))
            }
            0xAD74D9AC => {
                let src = args
                    .get(top(0))
                    .and_then(Value::as_str_bytes)
                    .map(cstr_bytes)
                    .unwrap_or(&[]);
                self.stack.push(Value::string(mbcs_tolower(src)));
                Some(Ok(InnerOutcome::Normal))
            }
            0x25401A1F => {
                let length = args.get(top(1)).and_then(Value::as_int).unwrap_or(0).max(0)
                    as usize;
                let src = args
                    .get(top(0))
                    .and_then(Value::as_str_bytes)
                    .map(cstr_bytes)
                    .unwrap_or(&[]);
                let chars = mbcs_len(src);
                let start = chars.saturating_sub(length);
                self.stack.push(Value::string(mbcs_substr(src, start, length)));
                Some(Ok(InnerOutcome::Normal))
            }
            0x05EA6E4D => {
                let mut bytes = self.text.page_accumulator.clone();
                bytes.push(0);
                self.stack.push(Value::string(bytes));
                Some(Ok(InnerOutcome::Normal))
            }
            0xB0C8F550 => {
                let src = args
                    .get(top(0))
                    .and_then(|v| v.as_str_bytes())
                    .map(cstr_bytes)
                    .unwrap_or(&[]);
                self.stack.push(Value::int(mbcs_len(src) as i32));
                Some(Ok(InnerOutcome::Normal))
            }
            0x6CFC884A => {
                let value = args
                    .iter()
                    .take(count.min(args.len()))
                    .filter_map(Value::as_int)
                    .max()
                    .unwrap_or(0);
                eprintln!("[STRMATH] max count={} -> {}", count, value);
                self.stack.push(Value::int(value));
                Some(Ok(InnerOutcome::Normal))
            }
            0x06906970 => {
                let length = args.get(top(1)).and_then(|v| v.as_int()).unwrap_or(0);
                let src = args
                    .get(top(0))
                    .and_then(|v| v.as_str_bytes())
                    .map(cstr_bytes)
                    .unwrap_or(&[]);
                let result = if length < 0 {
                    src.to_vec()
                } else {
                    mbcs_substr(src, 0, length as usize)
                };
                eprintln!(
                    "[STRMATH] str_left src={:?} len={} → {:?}",
                    String::from_utf8_lossy(src), length, String::from_utf8_lossy(&
                    result)
                );
                self.stack.push(Value::string(result));
                Some(Ok(InnerOutcome::Normal))
            }
            0x96B07AEE => {
                let (d0, d1, d2) = match count {
                    1 => {
                        let d = args
                            .get(top(0))
                            .and_then(|v| v.as_int())
                            .unwrap_or(1)
                            .max(1) as u32;
                        (d, 1, 1)
                    }
                    2 => {
                        let d0 = args
                            .get(top(1))
                            .and_then(|v| v.as_int())
                            .unwrap_or(1)
                            .max(1) as u32;
                        let d1 = args
                            .get(top(0))
                            .and_then(|v| v.as_int())
                            .unwrap_or(1)
                            .max(1) as u32;
                        (d0, d1, 1)
                    }
                    3 => {
                        let d0 = args
                            .get(top(2))
                            .and_then(|v| v.as_int())
                            .unwrap_or(1)
                            .max(1) as u32;
                        let d1 = args
                            .get(top(1))
                            .and_then(|v| v.as_int())
                            .unwrap_or(1)
                            .max(1) as u32;
                        let d2 = args
                            .get(top(0))
                            .and_then(|v| v.as_int())
                            .unwrap_or(1)
                            .max(1) as u32;
                        (d0, d1, d2)
                    }
                    _ => (1, 1, 1),
                };
                let total = (d0 as usize)
                    .checked_mul(d1 as usize)
                    .and_then(|n| n.checked_mul(d2 as usize))
                    .unwrap_or(usize::MAX);
                if total > MAX_ARRAY_CELLS {
                    return Some(
                        Err(
                            VmError::Other(
                                format!(
                                    "refusing oversized int array [{}×{}×{}] ({} cells)", d0,
                                    d1, d2, total
                                ),
                            ),
                        ),
                    );
                }
                eprintln!(
                    "[STRMATH] alloc_int_array count={} → {}D [{}×{}×{}] = {} cells",
                    count, count.max(1), d0, d1, d2, total
                );
                self.stack.push(Value::int_array(count.clamp(1, 3), [d0, d1, d2]));
                Some(Ok(InnerOutcome::Normal))
            }
            0x1F57B724 => {
                let (d0, d1, d2) = match count {
                    1 => {
                        let d = args
                            .get(top(0))
                            .and_then(|v| v.as_int())
                            .unwrap_or(1)
                            .max(1) as u32;
                        (d, 1, 1)
                    }
                    2 => {
                        let d0 = args
                            .get(top(1))
                            .and_then(|v| v.as_int())
                            .unwrap_or(1)
                            .max(1) as u32;
                        let d1 = args
                            .get(top(0))
                            .and_then(|v| v.as_int())
                            .unwrap_or(1)
                            .max(1) as u32;
                        (d0, d1, 1)
                    }
                    3 => {
                        let d0 = args
                            .get(top(2))
                            .and_then(|v| v.as_int())
                            .unwrap_or(1)
                            .max(1) as u32;
                        let d1 = args
                            .get(top(1))
                            .and_then(|v| v.as_int())
                            .unwrap_or(1)
                            .max(1) as u32;
                        let d2 = args
                            .get(top(0))
                            .and_then(|v| v.as_int())
                            .unwrap_or(1)
                            .max(1) as u32;
                        (d0, d1, d2)
                    }
                    _ => (1, 1, 1),
                };
                let total = (d0 as usize)
                    .checked_mul(d1 as usize)
                    .and_then(|n| n.checked_mul(d2 as usize))
                    .unwrap_or(usize::MAX);
                if total > MAX_ARRAY_CELLS {
                    return Some(
                        Err(
                            VmError::Other(
                                format!(
                                    "refusing oversized string array [{}×{}×{}] ({} cells)",
                                    d0, d1, d2, total
                                ),
                            ),
                        ),
                    );
                }
                eprintln!(
                    "[STRMATH] alloc_nested_array count={} → {}D [{}×{}×{}] = {} cells",
                    count, count.max(1), d0, d1, d2, total
                );
                self.stack.push(Value::string_array(count.clamp(1, 3), [d0, d1, d2]));
                Some(Ok(InnerOutcome::Normal))
            }
            0x8AEF9167 => {
                let src = args.get(top(0)).and_then(|v| v.as_str_bytes()).unwrap_or(&[]);
                let start = args.get(top(1)).and_then(|v| v.as_int()).unwrap_or(0).max(0)
                    as usize;
                let length = args
                    .get(top(2))
                    .and_then(|v| v.as_int())
                    .unwrap_or(0)
                    .max(0) as usize;
                let sub = mbcs_substr(src, start, length);
                eprintln!(
                    "[STRMATH] str_mid src_len={} start={} len={} → {:?}", src.len(),
                    start, length, String::from_utf8_lossy(& sub)
                );
                self.stack.push(Value::string(sub));
                Some(Ok(InnerOutcome::Normal))
            }
            0x69DB4512 => {
                let needle = args
                    .get(top(1))
                    .and_then(|v| v.as_str_bytes())
                    .unwrap_or(&[]);
                let haystack = args
                    .get(top(0))
                    .and_then(|v| v.as_str_bytes())
                    .unwrap_or(&[]);
                let pos = mbcs_find(haystack, needle);
                self.stack.push(Value::int(pos));
                Some(Ok(InnerOutcome::Normal))
            }
            0x109CA5DB => {
                let s = args.get(top(0)).and_then(|v| v.as_str_bytes()).unwrap_or(&[]);
                let n = parse_atoi(s);
                eprintln!(
                    "[STRMATH] str_to_int {:?} → {}", String::from_utf8_lossy(s), n
                );
                self.stack.push(Value::int(n));
                Some(Ok(InnerOutcome::Normal))
            }
            0x924EE3EB => {
                let r = clamp_byte(
                    args.get(top(0)).and_then(|v| v.as_int()).unwrap_or(0),
                );
                let g = clamp_byte(
                    args.get(top(1)).and_then(|v| v.as_int()).unwrap_or(0),
                );
                let b = clamp_byte(
                    args.get(top(2)).and_then(|v| v.as_int()).unwrap_or(0),
                );
                let color = r | (g << 8) | (b << 16);
                eprintln!(
                    "[STRMATH] color_rgb r={} g={} b={} → 0x{:06X}", r, g, b, color
                );
                self.stack.push(Value::int(color));
                Some(Ok(InnerOutcome::Normal))
            }
            0x54EF8174 => {
                let r = clamp_byte(
                    args.get(top(0)).and_then(|v| v.as_int()).unwrap_or(0),
                );
                let g = clamp_byte(
                    args.get(top(1)).and_then(|v| v.as_int()).unwrap_or(0),
                );
                let b = clamp_byte(
                    args.get(top(2)).and_then(|v| v.as_int()).unwrap_or(0),
                );
                let a = clamp_byte(
                    args.get(top(3)).and_then(|v| v.as_int()).unwrap_or(0),
                );
                let color = r | (g << 8) | (b << 16) | (a << 24);
                eprintln!(
                    "[STRMATH] color_argb r={} g={} b={} a={} → 0x{:08X}", r, g, b, a,
                    color
                );
                self.stack.push(Value::int(color));
                Some(Ok(InnerOutcome::Normal))
            }
            0x6F769889 => {
                let n = args.get(top(0)).and_then(|v| v.as_int()).unwrap_or(0);
                let s = n.to_string();
                eprintln!("[STRMATH] int_to_str {} → {:?}", n, s);
                self.stack.push(Value::string(s.into_bytes()));
                Some(Ok(InnerOutcome::Normal))
            }
            0x266A3C79 => {
                let src = args.get(top(0)).and_then(|v| v.as_str_bytes()).unwrap_or(&[]);
                let out: Vec<u8> = mbcs_toupper(src);
                eprintln!(
                    "[STRMATH] str_toupper {:?} → {:?}", String::from_utf8_lossy(src),
                    String::from_utf8_lossy(& out)
                );
                self.stack.push(Value::string(out));
                Some(Ok(InnerOutcome::Normal))
            }
            0x3FE05B39 => {
                let csv = args.get(top(0)).and_then(|v| v.as_str_bytes()).unwrap_or(&[]);
                let index = args.get(top(1)).and_then(|v| v.as_int()).unwrap_or(0).max(0)
                    as usize;
                let field = csv_field(csv, index);
                eprintln!(
                    "[STRMATH] str_csv_field {:?} [{}] → {:?}",
                    String::from_utf8_lossy(csv), index, String::from_utf8_lossy(& field)
                );
                self.stack.push(Value::string(field));
                Some(Ok(InnerOutcome::Normal))
            }
            _ => None,
        }
    }
}
fn mbcs_substr(src: &[u8], start: usize, length: usize) -> Vec<u8> {
    if let Ok(text) = std::str::from_utf8(src) {
        if !text.is_ascii() {
            return utf8_char_slice(text, start, length);
        }
    }
    let mut idx = 0usize;
    let mut chars_consumed = 0usize;
    while chars_consumed < start && idx < src.len() {
        let step = if is_sjis_lead(src[idx]) { 2 } else { 1 };
        idx += step;
        chars_consumed += 1;
    }
    if idx >= src.len() {
        return Vec::new();
    }
    let take_start = idx;
    let mut taken = 0usize;
    while taken < length && idx < src.len() {
        let step = if is_sjis_lead(src[idx]) { 2 } else { 1 };
        idx = idx.saturating_add(step).min(src.len());
        taken += 1;
    }
    src[take_start..idx].to_vec()
}
fn cstr_bytes(bytes: &[u8]) -> &[u8] {
    bytes.split(|&b| b == 0).next().unwrap_or(bytes)
}
fn mbcs_len(bytes: &[u8]) -> usize {
    if let Some(text) = utf8_override_text(bytes) {
        return text.chars().count();
    }
    let bytes = cstr_bytes(bytes);
    let mut byte = 0usize;
    let mut chars = 0usize;
    while byte < bytes.len() {
        byte += if is_sjis_lead(bytes[byte]) && byte + 1 < bytes.len() { 2 } else { 1 };
        chars += 1;
    }
    chars
}
fn mbcs_find(haystack: &[u8], needle: &[u8]) -> i32 {
    let haystack = cstr_bytes(haystack);
    let needle = cstr_bytes(needle);
    if needle.is_empty() {
        return 0;
    }
    let mut byte = 0usize;
    let mut char_index = 0i32;
    while byte + needle.len() <= haystack.len() {
        if &haystack[byte..byte + needle.len()] == needle {
            return char_index;
        }
        byte
            += if is_sjis_lead(haystack[byte]) && byte + 1 < haystack.len() {
                2
            } else {
                1
            };
        char_index += 1;
    }
    cross_encoding_find(haystack, needle)
}
fn cross_encoding_find(haystack: &[u8], needle: &[u8]) -> i32 {
    match (utf8_override_text(haystack), utf8_override_text(needle)) {
        (Some(haystack_text), None) => {
            text_find(haystack_text, &decode_sjis_lossy(needle))
        }
        (None, Some(needle_text)) => text_find(&decode_sjis_lossy(haystack), needle_text),
        _ => -1,
    }
}
fn text_find(haystack: &str, needle: &str) -> i32 {
    match haystack.find(needle) {
        Some(byte_pos) => haystack[..byte_pos].chars().count() as i32,
        None => -1,
    }
}
fn utf8_char_slice(text: &str, start: usize, length: usize) -> Vec<u8> {
    let start_byte = match text.char_indices().nth(start) {
        Some((i, _)) => i,
        None => return Vec::new(),
    };
    let end_byte = text
        .char_indices()
        .nth(start.saturating_add(length))
        .map_or(text.len(), |(i, _)| i);
    text.as_bytes()[start_byte..end_byte].to_vec()
}
#[inline]
fn is_sjis_lead(b: u8) -> bool {
    (0x81..=0x9F).contains(&b) || (0xE0..=0xFC).contains(&b)
}
#[inline]
fn clamp_byte(x: i32) -> i32 {
    x.clamp(0, 255)
}
fn msvc_rand(state: &mut u32) -> i32 {
    *state = state.wrapping_mul(214_013).wrapping_add(2_531_011);
    ((*state >> 16) & 0x7fff) as i32
}
fn parse_atoi(s: &[u8]) -> i32 {
    let mut i = 0;
    while i < s.len() && s[i].is_ascii_whitespace() {
        i += 1;
    }
    let mut sign: i32 = 1;
    if i < s.len() && (s[i] == b'+' || s[i] == b'-') {
        if s[i] == b'-' {
            sign = -1;
        }
        i += 1;
    }
    let mut acc: i64 = 0;
    while i < s.len() && s[i].is_ascii_digit() {
        acc = acc * 10 + (s[i] - b'0') as i64;
        if acc > i32::MAX as i64 && sign > 0 {
            return i32::MAX;
        }
        if acc > (i32::MAX as i64) + 1 && sign < 0 {
            return i32::MIN;
        }
        i += 1;
    }
    (sign as i64 * acc) as i32
}
fn mbcs_toupper(s: &[u8]) -> Vec<u8> {
    mbcs_ascii_case(s, |b| b.to_ascii_uppercase())
}
fn mbcs_tolower(s: &[u8]) -> Vec<u8> {
    mbcs_ascii_case(s, |b| b.to_ascii_lowercase())
}
fn mbcs_ascii_case(s: &[u8], fold: impl Fn(u8) -> u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    let mut index = 0;
    while index < s.len() {
        let byte = s[index];
        if is_sjis_lead(byte) && index + 1 < s.len() {
            out.extend_from_slice(&s[index..index + 2]);
            index += 2;
        } else {
            out.push(fold(byte));
            index += 1;
        }
    }
    out
}
fn native_tail_wildcard_compare(left: &[u8], right: &[u8]) -> bool {
    let mut li = 0isize;
    let mut ri = 0isize;
    while byte_at(left, li) != 0 {
        let r = byte_at(right, ri);
        if r == 0 {
            break;
        }
        let l = byte_at(left, li);
        if l == b'*' {
            ri += remaining_len(right, ri) - remaining_len(left, li);
        } else if r == b'*' {
            li += remaining_len(left, li) - remaining_len(right, ri);
        } else if l != b'?' && r != b'?' && l != r {
            break;
        }
        li += 1;
        ri += 1;
    }
    byte_at(left, li) == 0 && byte_at(right, ri) == 0
}
fn byte_at(bytes: &[u8], index: isize) -> u8 {
    usize::try_from(index).ok().and_then(|index| bytes.get(index).copied()).unwrap_or(0)
}
fn remaining_len(bytes: &[u8], index: isize) -> isize {
    let Ok(index) = usize::try_from(index) else {
        return 0;
    };
    bytes.get(index..).map_or(0, |tail| tail.len() as isize)
}
fn csv_field(csv: &[u8], index: usize) -> Vec<u8> {
    let mut p = 0usize;
    for _ in 0..index {
        match csv[p..].iter().position(|&b| b == b',') {
            Some(i) => p += i + 1,
            None => return Vec::new(),
        }
    }
    let end = csv[p..]
        .iter()
        .position(|&b| b == b',')
        .map(|i| p + i)
        .unwrap_or(csv.len());
    let raw = &csv[p..end];
    let mut start = 0usize;
    while start < raw.len() && raw[start] <= 0x20 {
        start += 1;
    }
    let mut stop = raw.len();
    while stop > start && raw[stop - 1] <= 0x20 {
        stop -= 1;
    }
    let mut field = raw[start..stop].to_vec();
    if field.len() >= 2 && field[0] == b'"' && field[field.len() - 1] == b'"' {
        field = field[1..field.len() - 1].to_vec();
    }
    field
}

