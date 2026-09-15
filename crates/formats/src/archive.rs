use encoding_rs::SHIFT_JIS;
use std::path::PathBuf;
#[derive(Debug)]
pub struct Archive {
    pub version: ArcVersion,
    pub entries: Vec<ArcEntry>,
}
#[derive(Debug)]
pub struct ArcEntry {
    pub name: String,
    pub data: Vec<u8>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArcVersion {
    V1,
    V2,
    V3,
}
impl Archive {
    pub fn parse(data: &[u8]) -> Result<Self, String> {
        Self::parse_with(data, false)
    }
    pub fn parse_with(data: &[u8], decode_sjis: bool) -> Result<Self, String> {
        if data.len() < 28 {
            return Err("too small".into());
        }
        let magic_end = data[..16].iter().position(|&b| b == 0).unwrap_or(16);
        let magic = std::str::from_utf8(&data[..magic_end]).map_err(|_| "bad magic")?;
        let ver = match magic {
            "MajiroArcV1.000" => ArcVersion::V1,
            "MajiroArcV2.000" => ArcVersion::V2,
            "MajiroArcV3.000" => ArcVersion::V3,
            _ => return Err(format!("unknown magic: '{}'", magic)),
        };
        let count = slip(data, 4) as usize;
        let names_off = slip(data, 5) as usize;
        let data_off = slip(data, 6) as usize;
        if count == 0 {
            return Ok(Archive {
                version: ver,
                entries: vec![],
            });
        }
        if count > 500_000 {
            return Err(format!("count too large: {}", count));
        }
        if names_off < 0x1C || names_off >= data.len() {
            return Err("bad names_off".into());
        }
        if data_off <= names_off || data_off > data.len() {
            return Err("bad data_off".into());
        }
        let entry_size = match ver {
            ArcVersion::V1 => 8,
            ArcVersion::V2 => 12,
            ArcVersion::V3 => 16,
        };
        let table_entries = if ver == ArcVersion::V1 { count + 1 } else { count };
        let table_size = table_entries
            .checked_mul(entry_size)
            .ok_or_else(|| "entry table size overflow".to_string())?;
        let table_start: usize = 0x1C;
        let table_end = table_start
            .checked_add(table_size)
            .ok_or_else(|| "entry table end overflow".to_string())?;
        if table_end != names_off {
            return Err(
                format!(
                    "table size mismatch: table_end=0x{:X} names_off=0x{:X}", table_end,
                    names_off
                ),
            );
        }
        let table = &data[table_start..table_end];
        let names_size = data_off - names_off;
        let names_data = &data[names_off..names_off + names_size];
        let names: Vec<String> = split_names(names_data, decode_sjis);
        if names.len() < count {
            return Err(format!("expected {} names, got {}", count, names.len()));
        }
        let hash_size = match ver {
            ArcVersion::V3 => 8,
            _ => 4,
        };
        let mut entries = Vec::with_capacity(count);
        for (i, name) in names.iter().take(count).enumerate() {
            let entry_off = i * entry_size;
            let data_offset = slip(table, (entry_off + hash_size) / 4) as usize;
            let data_size = match ver {
                ArcVersion::V1 => {
                    let next_off = slip(table, (entry_off + entry_size + hash_size) / 4)
                        as usize;
                    next_off.saturating_sub(data_offset)
                }
                _ => slip(table, (entry_off + hash_size + 4) / 4) as usize,
            };
            let data_end = data_offset
                .checked_add(data_size)
                .ok_or_else(|| format!("entry {i} data range overflow"))?;
            if data_offset < data_off || data_end > data.len() {
                return Err(
                    format!(
                        "entry {i} data range 0x{data_offset:X}..0x{data_end:X} outside archive"
                    ),
                );
            }
            let entry_data = data[data_offset..data_end].to_vec();
            entries
                .push(ArcEntry {
                    name: name.clone(),
                    data: entry_data,
                });
        }
        Ok(Archive { version: ver, entries })
    }
}
fn slip(bytes: &[u8], idx: usize) -> u32 {
    let s = idx * 4;
    if s + 4 > bytes.len() {
        0
    } else {
        u32::from_le_bytes([bytes[s], bytes[s + 1], bytes[s + 2], bytes[s + 3]])
    }
}
fn split_names(data: &[u8], decode_sjis: bool) -> Vec<String> {
    let mut names = Vec::new();
    let mut start = 0;
    while start < data.len() {
        let end = data[start..]
            .iter()
            .position(|&b| b == 0)
            .map(|p| start + p)
            .unwrap_or(data.len());
        let name_bytes = &data[start..end];
        if name_bytes.is_empty() {
            start = end + 1;
            continue;
        }
        let name = if decode_sjis {
            let (cow, _, had_errors) = SHIFT_JIS.decode(name_bytes);
            if had_errors {
                String::from_utf8_lossy(name_bytes).into_owned()
            } else {
                cow.into_owned()
            }
        } else {
            let (cow, _, had_errors) = SHIFT_JIS.decode(name_bytes);
            if had_errors {
                String::from_utf8_lossy(name_bytes).into_owned()
            } else {
                cow.into_owned()
            }
        };
        names.push(name);
        start = end + 1;
    }
    names
}
#[derive(Debug, Clone)]
struct IndexEntry {
    data_offset: u32,
    data_size: u32,
}
pub struct ArcIndex {
    path: PathBuf,
    memory: Option<Vec<u8>>,
    remote: Option<(std::sync::Arc<dyn crate::remote::RemoteReader>, String)>,
    version: ArcVersion,
    files: std::collections::HashMap<String, IndexEntry>,
}
impl ArcIndex {
    pub fn archive_stem(&self) -> &str {
        self.path.file_stem().and_then(|s| s.to_str()).unwrap_or_default()
    }
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, String> {
        let path = path.into();
        use std::io::{Read, Seek, SeekFrom};
        let mut file = std::fs::File::open(&path)
            .map_err(|e| format!("open {}: {}", path.display(), e))?;
        let archive_len = file
            .metadata()
            .map_err(|e| format!("stat {}: {}", path.display(), e))?
            .len();
        let mut header = [0u8; 28];
        file.read_exact(&mut header)
            .map_err(|e| format!("read header {}: {}", path.display(), e))?;
        let data_off = slip(&header, 6) as usize;
        if data_off < header.len() || data_off as u64 > archive_len {
            return Err("bad data_off".into());
        }
        let mut metadata = vec![0u8; data_off];
        file.seek(SeekFrom::Start(0))
            .and_then(|_| file.read_exact(&mut metadata))
            .map_err(|e| format!("read index {}: {}", path.display(), e))?;
        Self::parse(&metadata, path, archive_len, None)
    }
    pub fn from_memory(data: Vec<u8>) -> Result<Self, String> {
        let archive_len = data.len() as u64;
        let memory = Some(data);
        let index = Self::parse(
            memory.as_deref().unwrap(),
            PathBuf::new(),
            archive_len,
            None,
        )?;
        Ok(Self { memory, ..index })
    }
    pub fn from_remote(
        reader: std::sync::Arc<dyn crate::remote::RemoteReader>,
        name: &str,
    ) -> Result<Self, String> {
        let archive_len = reader.size_of(name)?;
        let header = reader.read_at(name, 0, 28)?;
        let data_off = slip(&header, 6) as u64;
        if data_off < 28 || data_off > archive_len {
            return Err("bad data_off".into());
        }
        if data_off > u32::MAX as u64 {
            return Err("archive index prefix exceeds u32 range".into());
        }
        let metadata = reader.read_at(name, 0, data_off as u32)?;
        let mut index = Self::parse(&metadata, PathBuf::from(name), archive_len, None)?;
        index.remote = Some((reader, name.to_string()));
        Ok(index)
    }
    fn parse(
        data: &[u8],
        path: PathBuf,
        archive_len: u64,
        memory: Option<Vec<u8>>,
    ) -> Result<Self, String> {
        if data.len() < 28 {
            return Err("too small".into());
        }
        let magic_end = data[..16].iter().position(|&b| b == 0).unwrap_or(16);
        let magic = std::str::from_utf8(&data[..magic_end]).map_err(|_| "bad magic")?;
        let ver = match magic {
            "MajiroArcV1.000" => ArcVersion::V1,
            "MajiroArcV2.000" => ArcVersion::V2,
            "MajiroArcV3.000" => ArcVersion::V3,
            _ => return Err(format!("unknown magic: '{}'", magic)),
        };
        let count = slip(data, 4) as usize;
        let names_off = slip(data, 5) as usize;
        let data_off = slip(data, 6) as usize;
        if count == 0 {
            return Ok(Self {
                path,
                memory,
                remote: None,
                version: ver,
                files: Default::default(),
            });
        }
        if count > 500_000 {
            return Err(format!("count too large: {}", count));
        }
        if names_off < 0x1C || names_off >= data.len() || data_off <= names_off
            || data_off > data.len()
        {
            return Err("bad offsets".into());
        }
        let entry_size = match ver {
            ArcVersion::V1 => 8,
            ArcVersion::V2 => 12,
            ArcVersion::V3 => 16,
        };
        let table_entries = if ver == ArcVersion::V1 { count + 1 } else { count };
        let table_start: usize = 0x1C;
        let table_size = table_entries
            .checked_mul(entry_size)
            .ok_or_else(|| "entry table size overflow".to_string())?;
        let table_end = table_start
            .checked_add(table_size)
            .ok_or_else(|| "entry table end overflow".to_string())?;
        if table_end != names_off || table_end > data.len() {
            return Err("entry table does not end at names_off".into());
        }
        let table = &data[table_start..table_end];
        let names_size = data_off - names_off;
        let names_data = &data[names_off..names_off + names_size];
        let names: Vec<String> = split_names(names_data, false);
        if names.len() < count {
            return Err(format!("expected {count} names, got {}", names.len()));
        }
        let hash_size = match ver {
            ArcVersion::V3 => 8,
            _ => 4,
        };
        let mut files = std::collections::HashMap::with_capacity(count);
        for (i, name) in names.iter().take(count).enumerate() {
            let off = i * entry_size;
            let data_offset = slip(table, (off + hash_size) / 4) as u32;
            let data_size = match ver {
                ArcVersion::V1 => {
                    let next = slip(table, (off + entry_size + hash_size) / 4) as u32;
                    next.saturating_sub(data_offset)
                }
                _ => slip(table, (off + hash_size + 4) / 4) as u32,
            };
            let data_end = u64::from(data_offset)
                .checked_add(u64::from(data_size))
                .ok_or_else(|| format!("entry {i} data range overflow"))?;
            if u64::from(data_offset) < data_off as u64 || data_end > archive_len {
                return Err(format!("entry {i} data range outside archive"));
            }
            let name = name.to_lowercase();
            files
                .insert(
                    name,
                    IndexEntry {
                        data_offset,
                        data_size,
                    },
                );
        }
        Ok(Self {
            path,
            memory,
            remote: None,
            version: ver,
            files,
        })
    }
    pub fn find_name(&self, name: &str) -> Option<(u32, u32)> {
        let key = name.to_lowercase();
        self.files.get(&key).map(|e| (e.data_offset, e.data_size))
    }
    pub fn read(&self, offset: u32, size: u32) -> Result<Vec<u8>, std::io::Error> {
        if let Some(bytes) = &self.memory {
            let start = offset as usize;
            let end = start + size as usize;
            return bytes
                .get(start..end)
                .map(<[u8]>::to_vec)
                .ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        "entry range outside memory archive",
                    )
                });
        }
        if let Some((reader, name)) = &self.remote {
            return reader
                .read_at(name, u64::from(offset), size)
                .map_err(std::io::Error::other);
        }
        use std::io::{Read, Seek, SeekFrom};
        let mut file = std::fs::File::open(&self.path)?;
        file.seek(SeekFrom::Start(offset as u64))?;
        let mut buf = vec![0u8; size as usize];
        file.read_exact(&mut buf)?;
        Ok(buf)
    }
    pub fn find(&self, name: &str) -> Option<Vec<u8>> {
        let (off, size) = self.find_name(name)?;
        self.read(off, size).ok()
    }
    pub fn entry_count(&self) -> usize {
        self.files.len()
    }
    pub fn version(&self) -> ArcVersion {
        self.version
    }
}

