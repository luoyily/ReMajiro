const fn is_sjis_lead(b: u8) -> bool {
    (b >= 0x81 && b <= 0x9F) || (b >= 0xE0 && b <= 0xFC)
}
fn sjis_inc(bytes: &[u8], i: usize) -> usize {
    if i < bytes.len() && is_sjis_lead(bytes[i]) {
        (i + 2).min(bytes.len())
    } else {
        (i + 1).min(bytes.len())
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ControlCodeKind {
    Newline = b'n',
    NewlineRelative = b'N',
    Wait = b'w',
    PageClear = b'P',
    PagePause = b'p',
    Color = b'c',
    Font = b'f',
    Position = b'l',
    Speed = b's',
    Delay = b't',
    Voice = b'v',
    OffsetSave = b'o',
    Restore = b'r',
    ExecScript = b'x',
    Continue = b'z',
    Graphics = b'g',
}
impl ControlCodeKind {
    pub const fn from_byte(b: u8) -> Option<Self> {
        Some(
            match b {
                b'n' => Self::Newline,
                b'N' => Self::NewlineRelative,
                b'w' => Self::Wait,
                b'P' => Self::PageClear,
                b'p' => Self::PagePause,
                b'c' => Self::Color,
                b'f' => Self::Font,
                b'l' => Self::Position,
                b's' => Self::Speed,
                b't' => Self::Delay,
                b'v' => Self::Voice,
                b'o' => Self::OffsetSave,
                b'r' => Self::Restore,
                b'x' => Self::ExecScript,
                b'z' => Self::Continue,
                b'g' => Self::Graphics,
                _ => return None,
            },
        )
    }
    pub const fn as_byte(self) -> u8 {
        self as u8
    }
    pub const fn as_name(self) -> &'static str {
        match self {
            Self::Newline => "newline",
            Self::NewlineRelative => "newline_rel",
            Self::Wait => "wait",
            Self::PageClear => "page_clear",
            Self::PagePause => "page_pause",
            Self::Color => "color",
            Self::Font => "font",
            Self::Position => "position",
            Self::Speed => "speed",
            Self::Delay => "delay",
            Self::Voice => "voice",
            Self::OffsetSave => "offset_save",
            Self::Restore => "restore",
            Self::ExecScript => "exec_script",
            Self::Continue => "continue",
            Self::Graphics => "graphics",
        }
    }
    pub const fn fixed_payload_len(self) -> Option<usize> {
        Some(
            match self {
                Self::Newline
                | Self::NewlineRelative
                | Self::Wait
                | Self::PageClear
                | Self::PagePause
                | Self::Restore
                | Self::Continue => 0,
                Self::Color | Self::Position | Self::OffsetSave => 20,
                Self::Graphics => 60,
                Self::Speed
                | Self::Delay
                | Self::Font
                | Self::Voice
                | Self::ExecScript => return None,
            },
        )
    }
}
pub fn parse_hex_field(bytes: &[u8]) -> u32 {
    let mut acc: u32 = 0;
    let mut consumed = 0;
    for &b in bytes {
        if consumed >= 8 {
            break;
        }
        let nibble = match b {
            b'0'..=b'9' => b - b'0',
            b'a'..=b'f' => b - b'a' + 10,
            b'A'..=b'F' => b - b'A' + 10,
            _ => continue,
        };
        acc = (acc << 4) | nibble as u32;
        consumed += 1;
    }
    acc
}
pub fn parse_color(bytes: &[u8]) -> u32 {
    parse_hex_field(bytes)
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlCode {
    pub kind: ControlCodeKind,
    pub payload: Vec<u8>,
}
impl ControlCode {
    pub fn colors(&self) -> Option<(u32, u32)> {
        if self.kind != ControlCodeKind::Color {
            return None;
        }
        let p = &self.payload;
        let fg = parse_color(p.get(2..10).unwrap_or(&[]));
        let bg = parse_color(p.get(12..20).unwrap_or(&[]));
        Some((fg, bg))
    }
    pub fn position(&self) -> Option<(i32, i32)> {
        let ok = matches!(
            self.kind, ControlCodeKind::Position | ControlCodeKind::OffsetSave
        );
        if !ok {
            return None;
        }
        let p = &self.payload;
        let x = parse_hex_field(p.get(2..10).unwrap_or(&[])) as i32;
        let y = parse_hex_field(p.get(12..20).unwrap_or(&[])) as i32;
        Some((x, y))
    }
    pub fn scalar(&self) -> Option<u32> {
        if !matches!(self.kind, ControlCodeKind::Speed | ControlCodeKind::Delay) {
            return None;
        }
        Some(parse_hex_field(self.payload.get(2..10).unwrap_or(&self.payload)))
    }
    pub fn voice(&self) -> Option<(u8, &[u8])> {
        if self.kind != ControlCodeKind::Voice {
            return None;
        }
        let p = &self.payload;
        let ch = p.first().copied().unwrap_or(b'0').wrapping_sub(b'0');
        let name = p.get(1..).unwrap_or(&[]);
        let name = match name.iter().position(|&b| b == 0) {
            Some(n) => &name[..n],
            None => name,
        };
        Some((ch, name))
    }
    pub fn script_name(&self) -> Option<&[u8]> {
        if self.kind != ControlCodeKind::ExecScript {
            return None;
        }
        let name = &self.payload;
        match name.iter().position(|&b| b == 0) {
            Some(n) => Some(&name[..n]),
            None => Some(name),
        }
    }
    pub fn font(&self) -> Option<FontSpec<'_>> {
        if self.kind != ControlCodeKind::Font {
            return None;
        }
        let p = &self.payload;
        let val = |range: std::ops::Range<usize>| -> Option<u32> {
            let v = parse_hex_field(p.get(range).unwrap_or(&[]));
            (v != u32::MAX).then_some(v)
        };
        let face_start = 40.min(p.len());
        let mut face = p.get(face_start..).unwrap_or(&[]);
        if let Some(n) = face.iter().position(|&b| b == 0) {
            face = &face[..n];
        }
        Some((val(2..10), val(12..20), val(22..30), val(32..40), face))
    }
    pub fn graphics(&self) -> Option<[Option<i32>; 6]> {
        if self.kind != ControlCodeKind::Graphics {
            return None;
        }
        let p = &self.payload;
        let mut out = [None; 6];
        let starts = [2usize, 12, 22, 32, 42, 52];
        for (i, &s) in starts.iter().enumerate() {
            let v = parse_hex_field(p.get(s..s + 8).unwrap_or(&[])) as i32;
            out[i] = (v != -99).then_some(v);
        }
        Some(out)
    }
}
pub type FontSpec<'a> = (Option<u32>, Option<u32>, Option<u32>, Option<u32>, &'a [u8]);
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextToken {
    Text(Vec<u8>),
    Control(ControlCode),
}
pub const ESCAPE: u8 = 0x5C;
pub fn tokenize(bytes: &[u8]) -> Vec<TextToken> {
    let mut out = Vec::new();
    let mut text_run: Vec<u8> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let is_escape = bytes[i] == ESCAPE && i + 1 < bytes.len()
            && ControlCodeKind::from_byte(bytes[i + 1]).is_some();
        if is_escape {
            let kind = ControlCodeKind::from_byte(bytes[i + 1]).unwrap();
            if !text_run.is_empty() {
                out.push(TextToken::Text(std::mem::take(&mut text_run)));
            }
            i += 2;
            let payload_start = i;
            let payload_end = match kind.fixed_payload_len() {
                Some(fixed) => (i + fixed).min(bytes.len()),
                None => {
                    while i < bytes.len() {
                        if bytes[i] == ESCAPE && i + 1 < bytes.len()
                            && ControlCodeKind::from_byte(bytes[i + 1]).is_some()
                        {
                            break;
                        }
                        i = sjis_inc(bytes, i);
                    }
                    i
                }
            };
            let payload = bytes[payload_start..payload_end].to_vec();
            i = payload_end;
            out.push(TextToken::Control(ControlCode { kind, payload }));
        } else {
            let next = sjis_inc(bytes, i);
            text_run.extend_from_slice(&bytes[i..next]);
            i = next;
        }
    }
    if !text_run.is_empty() {
        out.push(TextToken::Text(text_run));
    }
    out
}
pub fn format_tokens(tokens: &[TextToken]) -> String {
    let mut s = String::new();
    for tok in tokens {
        match tok {
            TextToken::Text(bytes) => {
                let (cow, _, _) = encoding_rs::SHIFT_JIS.decode(bytes);
                s.push_str(&cow);
            }
            TextToken::Control(cc) => {
                s.push_str(&format_control(cc));
            }
        }
    }
    s
}
pub fn format_control(cc: &ControlCode) -> String {
    use ControlCodeKind as K;
    match cc.kind {
        K::Newline | K::NewlineRelative => "(newline)".to_string(),
        K::Wait => "(wait)".to_string(),
        K::PageClear => "(page_clear)".to_string(),
        K::PagePause => "(page_pause)".to_string(),
        K::Continue => "(continue)".to_string(),
        K::Restore => "(restore)".to_string(),
        K::Color => {
            let (fg, bg) = cc.colors().unwrap_or((0, 0));
            format!("(color:#{:06X}/#{:06X})", fg & 0xFFFFFF, bg & 0xFFFFFF)
        }
        K::Position => {
            let (x, y) = cc.position().unwrap_or((0, 0));
            format!("(pos:{},{})", x, y)
        }
        K::OffsetSave => {
            let (x, y) = cc.position().unwrap_or((0, 0));
            format!("(offset:+{},+{})", x, y)
        }
        K::Speed => format!("(speed:{})", cc.scalar().unwrap_or(0)),
        K::Delay => format!("(delay:{})", cc.scalar().unwrap_or(0)),
        K::Voice => {
            let (ch, name) = cc.voice().unwrap_or((0, &[]));
            let (cow, _, _) = encoding_rs::SHIFT_JIS.decode(name);
            format!("(voice ch={} {:?})", ch, cow)
        }
        K::ExecScript => {
            let name = cc.script_name().unwrap_or(&[]);
            let (cow, _, _) = encoding_rs::SHIFT_JIS.decode(name);
            format!("(exec:{:?})", cow)
        }
        K::Font => {
            let (sz, w, lh, sp, face) = cc
                .font()
                .unwrap_or((None, None, None, None, &[]));
            let (fcow, _, _) = encoding_rs::SHIFT_JIS.decode(face);
            format!("(font size={:?} wt={:?} lh={:?} sp={:?} {:?})", sz, w, lh, sp, fcow)
        }
        K::Graphics => {
            let params = cc.graphics().unwrap_or([None; 6]);
            let parts: Vec<String> = params
                .iter()
                .map(|p| match p {
                    Some(v) => v.to_string(),
                    None => "-".to_string(),
                })
                .collect();
            format!("(gfx:{})", parts.join(","))
        }
    }
}

