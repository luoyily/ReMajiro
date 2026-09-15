pub(super) struct ParsedLine {
    pub(super) head: Vec<u8>,
    pub(super) tail: Option<Vec<u8>>,
}
pub(super) fn read_next_line(data: &[u8], cursor: &mut usize) -> Option<Vec<u8>> {
    if *cursor >= data.len() {
        return None;
    }
    let start = *cursor;
    let end = data[*cursor..]
        .iter()
        .position(|&b| b == b'\n')
        .map(|i| *cursor + i)
        .unwrap_or(data.len());
    *cursor = (end + 1).min(data.len());
    let mut line_end = end;
    if line_end > start && data[line_end - 1] == b'\r' {
        line_end -= 1;
    }
    Some(data[start..line_end].to_vec())
}
pub(super) fn parse_line_token(line: &[u8]) -> ParsedLine {
    let mut s = line;
    if let Some(i) = s.iter().position(|&b| b == b';') {
        s = &s[..i];
    }
    s = strip_substring(s, b"//");
    let (head_raw, tail) = match s.iter().position(|&b| b == b',') {
        Some(i) => {
            let before = &s[..i];
            let after = &s[i + 1..];
            let tail_trim = trim_trailing(after);
            if tail_trim.is_empty() {
                (before, None)
            } else {
                (before, Some(after.to_vec()))
            }
        }
        None => (s, None),
    };
    let head = trim_whitespace(head_raw);
    let head = unwrap_quotes(head);
    ParsedLine {
        head: head.to_vec(),
        tail: tail.map(|t| t.to_vec()),
    }
}
pub(super) fn parse_raw_line(line: &[u8]) -> Vec<u8> {
    let mut s = line;
    if let Some(i) = s.iter().position(|&b| b == b';') {
        s = &s[..i];
    }
    s = strip_substring(s, b"//");
    trim_whitespace(s).to_vec()
}
fn trim_whitespace(s: &[u8]) -> &[u8] {
    let mut start = 0;
    while start < s.len() && s[start] <= 0x20 {
        start += 1;
    }
    let mut end = s.len();
    while end > start && s[end - 1] <= 0x20 {
        end -= 1;
    }
    &s[start..end]
}
fn trim_trailing(s: &[u8]) -> &[u8] {
    let mut end = s.len();
    while end > 0 && s[end - 1] <= 0x20 {
        end -= 1;
    }
    &s[..end]
}
fn strip_substring<'a>(haystack: &'a [u8], needle: &[u8]) -> &'a [u8] {
    if needle.is_empty() || haystack.len() < needle.len() {
        return haystack;
    }
    for i in 0..=(haystack.len() - needle.len()) {
        if &haystack[i..i + needle.len()] == needle {
            return &haystack[..i];
        }
    }
    haystack
}
fn unwrap_quotes(s: &[u8]) -> &[u8] {
    if s.len() >= 2 && s[0] == b'"' && s[s.len() - 1] == b'"' {
        &s[1..s.len() - 1]
    } else {
        s
    }
}
pub(super) fn sjis_to_string(bytes: &[u8]) -> String {
    let (cow, _, _) = encoding_rs::SHIFT_JIS.decode(bytes);
    cow.trim_end_matches('\0').to_string()
}
pub(super) fn resource_candidates(bytes: &[u8]) -> Vec<String> {
    let mut candidates = image_name_candidates(&sjis_to_string(bytes));
    let bytes = bytes.split(|&b| b == 0).next().unwrap_or(bytes);
    if !bytes.iter().all(|&b| b < 0x80) {
        let repaired = mixed_encoding_decode(bytes);
        for variant in ascii_width_variants(&repaired) {
            for candidate in image_name_candidates(&variant) {
                if !candidates.contains(&candidate) {
                    candidates.push(candidate);
                }
            }
        }
    }
    candidates
}
fn mixed_encoding_decode(bytes: &[u8]) -> String {
    fn sjis_lead(b: u8) -> bool {
        (0x81..=0x9F).contains(&b) || (0xE0..=0xFC).contains(&b)
    }
    let mut out = String::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b < 0x80 {
            out.push(b as char);
            i += 1;
            continue;
        }
        if (0xC2..=0xF4).contains(&b) {
            let len = match b {
                0xE0..=0xEF => 3,
                0xF0..=0xF4 => 4,
                _ => 2,
            };
            if i + len <= bytes.len() {
                if let Ok(text) = std::str::from_utf8(&bytes[i..i + len]) {
                    out.push_str(text);
                    i += len;
                    continue;
                }
            }
        }
        let step = if sjis_lead(b) && i + 1 < bytes.len() { 2 } else { 1 };
        let (cow, _, _) = encoding_rs::SHIFT_JIS.decode(&bytes[i..i + step]);
        out.push_str(&cow);
        i += step;
    }
    out
}
fn ascii_width_variants(text: &str) -> Vec<String> {
    let mut half = String::with_capacity(text.len());
    let mut full = String::with_capacity(text.len());
    for c in text.chars() {
        let half_char = matches!(c, '\u{FF21}'..='\u{FF3A}' | '\u{FF41}'..='\u{FF5A}')
            .then(|| char::from_u32(c as u32 - 0xFEE0))
            .flatten();
        let full_char = c
            .is_ascii_alphabetic()
            .then(|| char::from_u32(c as u32 + 0xFEE0))
            .flatten();
        half.push(half_char.unwrap_or(c));
        full.push(full_char.unwrap_or(c));
    }
    let mut variants = vec![text.to_string()];
    for variant in [half, full] {
        if !variants.contains(&variant) {
            variants.push(variant);
        }
    }
    variants
}
pub(super) fn image_name_candidates(name: &str) -> Vec<String> {
    let bare = name.rsplit(['/', '\\']).next().unwrap_or(name);
    if let Some(dot) = bare.rfind('.') {
        if bare[dot..].eq_ignore_ascii_case(".png") {
            let stem_len = name.len() - bare.len() + dot;
            let stem = &name[..stem_len];
            return vec![name.to_string(), format!("{stem}.rct"), format!("{stem}.rc8"),];
        }
        vec![name.to_string()]
    } else {
        vec![name.to_string(), format!("{name}.rct"), format!("{name}.rc8"),]
    }
}

