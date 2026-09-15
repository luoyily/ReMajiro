use crate::crypto::x1_crypt;
#[derive(Debug)]
pub struct MjoFile {
    pub magic: String,
    pub encrypted: bool,
    pub main_offset: u32,
    pub line_count: u32,
    pub entry_count: u32,
    pub entries: Vec<MjoEntry>,
    pub data: Vec<u8>,
    pub data_size: u32,
}
#[derive(Debug, Clone)]
pub struct MjoEntry {
    pub name_hash: u32,
    pub offset: u32,
}
#[derive(Debug)]
pub enum MjoError {
    TooSmall,
    BadMagic(String),
    TooManyEntries(u32),
    InvalidOffset {
        kind: &'static str,
        index: Option<usize>,
        offset: u32,
        data_size: u32,
    },
    Truncated,
    Io(std::io::Error),
}
impl std::fmt::Display for MjoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MjoError::TooSmall => write!(f, "file too small for .mjo header"),
            MjoError::BadMagic(s) => write!(f, "unknown magic: '{}'", s),
            MjoError::TooManyEntries(n) => write!(f, "entry count too large: {}", n),
            MjoError::InvalidOffset { kind, index, offset, data_size } => {
                match index {
                    Some(index) => {
                        write!(
                            f,
                            "{kind} {index} offset 0x{offset:X} exceeds code size 0x{data_size:X}"
                        )
                    }
                    None => {
                        write!(
                            f,
                            "{kind} offset 0x{offset:X} exceeds code size 0x{data_size:X}"
                        )
                    }
                }
            }
            MjoError::Truncated => write!(f, "file truncated"),
            MjoError::Io(e) => write!(f, "I/O error: {}", e),
        }
    }
}
impl std::error::Error for MjoError {}
impl From<std::io::Error> for MjoError {
    fn from(e: std::io::Error) -> Self {
        MjoError::Io(e)
    }
}
impl MjoFile {
    pub fn parse(data: &[u8]) -> Result<Self, MjoError> {
        if data.len() < 0x1C + 4 {
            return Err(MjoError::TooSmall);
        }
        let magic_end = data[..16].iter().position(|&b| b == 0).unwrap_or(16);
        let magic = std::str::from_utf8(&data[..magic_end])
            .map_err(|_| MjoError::BadMagic("invalid UTF-8".into()))?
            .to_string();
        let encrypted = match magic.as_str() {
            "MajiroObjV1.000" => false,
            "MajiroObjX1.000" => true,
            _ => return Err(MjoError::BadMagic(magic)),
        };
        let main_offset = read_u32(data, 0x10);
        let line_count = read_u32(data, 0x14);
        let entry_count = read_u32(data, 0x18);
        if entry_count > 100_000 {
            return Err(MjoError::TooManyEntries(entry_count));
        }
        let entry_table_start: usize = 0x1C;
        let entry_table_size = (entry_count as usize)
            .checked_mul(8)
            .ok_or(MjoError::Truncated)?;
        let entry_table_end = entry_table_start
            .checked_add(entry_table_size)
            .ok_or(MjoError::Truncated)?;
        if entry_table_end + 4 > data.len() {
            return Err(MjoError::Truncated);
        }
        let mut entries = Vec::with_capacity(entry_count as usize);
        for i in 0..entry_count as usize {
            let off = entry_table_start + i * 8;
            entries
                .push(MjoEntry {
                    name_hash: read_u32(data, off),
                    offset: read_u32(data, off + 4),
                });
        }
        let data_size = read_u32(data, entry_table_end);
        let data_start = entry_table_end + 4;
        let data_end = data_start
            .checked_add(data_size as usize)
            .ok_or(MjoError::Truncated)?;
        if data_end > data.len() {
            return Err(MjoError::Truncated);
        }
        if main_offset > data_size {
            return Err(MjoError::InvalidOffset {
                kind: "main",
                index: None,
                offset: main_offset,
                data_size,
            });
        }
        for (index, entry) in entries.iter().enumerate() {
            if entry.offset > data_size {
                return Err(MjoError::InvalidOffset {
                    kind: "entry",
                    index: Some(index),
                    offset: entry.offset,
                    data_size,
                });
            }
        }
        let mut payload = data[data_start..data_end].to_vec();
        if encrypted {
            x1_crypt(&mut payload);
        }
        Ok(MjoFile {
            magic,
            encrypted,
            main_offset,
            line_count,
            entry_count,
            entries,
            data: payload,
            data_size,
        })
    }
}
fn read_u32(data: &[u8], offset: usize) -> u32 {
    if offset + 4 > data.len() {
        return 0;
    }
    u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}
pub fn read_u16(data: &[u8], offset: usize) -> u16 {
    if offset + 2 > data.len() {
        return 0;
    }
    u16::from_le_bytes([data[offset], data[offset + 1]])
}
pub fn read_sjis_string(data: &[u8], offset: usize) -> Option<(String, usize)> {
    if offset >= data.len() {
        return None;
    }
    let end = data[offset..].iter().position(|&b| b == 0)?;
    let bytes = &data[offset..offset + end];
    let (cow, _, had_errors) = encoding_rs::SHIFT_JIS.decode(bytes);
    if had_errors {
        Some((String::from_utf8_lossy(bytes).into_owned(), end + 1))
    } else {
        Some((cow.into_owned(), end + 1))
    }
}

