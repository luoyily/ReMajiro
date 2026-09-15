use std::sync::Arc;
pub const MAGIC_SAV: &[u8; 15] = b"MajiroSavV1.103";
pub const MAGIC_MSS: &[u8; 15] = b"MajiroSysV1.100";
pub const SAV_HEADER_SIZE: usize = 0xA3E0;
pub const MSS_HEADER_SIZE: usize = 0x9C58;
pub const VALUE_SIZE: usize = 16;
pub const SCRIPT_ENTRY_SIZE: usize = 168;
pub const SCRIPT_NAME_SIZE: usize = 128;
pub const READMARK_HEADER_SIZE: usize = 0x94;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SavLayout {
    pub magic: &'static [u8; 15],
    pub header_size: usize,
    pub count_block: usize,
    pub thumb_pixel_size: usize,
}
impl SavLayout {
    pub const OWARUSEKAI: SavLayout = SavLayout {
        magic: MAGIC_SAV,
        header_size: 0xA3E0,
        count_block: 0x90,
        thumb_pixel_size: 3,
    };
    pub const RURI: SavLayout = SavLayout {
        magic: b"MajiroSavV1.105",
        header_size: 0xA560,
        count_block: 0x210,
        thumb_pixel_size: 4,
    };
    #[inline]
    pub const fn header_gap_size(&self) -> usize {
        self.count_block - 0x90
    }
    #[inline]
    pub const fn audio_blob_size(&self) -> usize {
        0x6F0
    }
    #[inline]
    const fn off_audio_blob(&self) -> usize {
        self.count_block + 0x1C
    }
    #[inline]
    const fn off_text_first_render(&self) -> usize {
        self.off_audio_blob() + self.audio_blob_size()
    }
    #[inline]
    const fn off_text_state_blob(&self) -> usize {
        self.off_text_first_render() + 4
    }
    #[inline]
    pub const fn text_state_blob_size(&self) -> usize {
        self.header_size - self.off_text_state_blob()
    }
}
const OFF_DESCRIPTION: usize = 0x10;
const OFF_SCRIPT_COUNT: usize = 0x90;
pub const AUDIO_BLOB_SIZE: usize = 0x6F0;
const TAG_INT: u32 = 0;
const TAG_FLOAT: u32 = 1;
const TAG_STRING: u32 = 2;
const TAG_ARRAY3D: u32 = 3;
const TAG_ARRAY3D_ALT: u32 = 4;
const TAG_COMPLEX: u32 = 5;
const ARRAY_DATA_OFFSET: usize = 20;
const ARRAY_OBJECT_BASE_SIZE: usize = 24;
const COMPLEX_HEADER_SIZE: usize = 24;
const STRING_HEADER_SIZE: usize = 12;
#[derive(Debug)]
pub enum SaveError {
    BadMagic { expected: &'static [u8; 15], actual: Vec<u8> },
    Truncated { needed: usize, have: usize },
    SectionSizeMismatch { claimed: usize, actual: usize },
    InvalidTypeTag(u32),
    OffsetOutOfBounds { offset: usize, extra_len: usize },
    BadObjectHeader { offset: usize, detail: &'static str },
}
impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadMagic { expected, actual } => {
                write!(
                    f, "bad magic: expected {:?}, got {:?}", std::str::from_utf8(&
                    expected[..]).unwrap_or("?"), std::str::from_utf8(actual)
                    .unwrap_or("?")
                )
            }
            Self::Truncated { needed, have } => {
                write!(f, "truncated: need {} bytes, have {}", needed, have)
            }
            Self::SectionSizeMismatch { claimed, actual } => {
                write!(
                    f, "section size mismatch: claimed {}, actual {}", claimed, actual
                )
            }
            Self::InvalidTypeTag(t) => write!(f, "invalid type_tag {}", t),
            Self::OffsetOutOfBounds { offset, extra_len } => {
                write!(
                    f, "offset_marker {} out of extra-data bounds ({})", offset,
                    extra_len
                )
            }
            Self::BadObjectHeader { offset, detail } => {
                write!(f, "bad object header at extra+0x{:X}: {}", offset, detail)
            }
        }
    }
}
impl std::error::Error for SaveError {}
#[derive(Debug, Clone, Default)]
pub struct SavFile {
    pub header: SavHeader,
    pub thumbnail: Option<Thumbnail>,
    pub script_state: Vec<ScriptStateEntry>,
    pub global_slot_heads: Vec<u32>,
    pub thread_slot_heads: Vec<u32>,
    pub local_slot_heads: Vec<u32>,
    pub globals: Vec<SerValue>,
    pub thread_globals: Vec<SerValue>,
    pub locals: Vec<SerValue>,
}
#[derive(Debug, Clone)]
pub struct SavHeader {
    pub magic: [u8; 16],
    pub description: [u8; 128],
    pub header_gap: Vec<u8>,
    pub script_count: u32,
    pub thumb_w: u32,
    pub thumb_h: u32,
    pub global_count: u32,
    pub thread_count: u32,
    pub local_count: u32,
    pub extra_size: u32,
    pub audio_blob: Vec<u8>,
    pub text_first_render: u32,
    pub text_state_blob: Vec<u8>,
}
impl SavHeader {
    pub fn new(layout: &SavLayout) -> Self {
        let mut magic = [0u8; 16];
        magic[..15].copy_from_slice(layout.magic);
        Self {
            magic,
            description: [0u8; 128],
            header_gap: vec![0u8; layout.header_gap_size()],
            script_count: 0,
            thumb_w: 0,
            thumb_h: 0,
            global_count: 0,
            thread_count: 0,
            local_count: 0,
            extra_size: 0,
            audio_blob: vec![0u8; layout.audio_blob_size()],
            text_first_render: 0,
            text_state_blob: vec![0u8; layout.text_state_blob_size()],
        }
    }
}
impl Default for SavHeader {
    fn default() -> Self {
        SavHeader::new(&SavLayout::OWARUSEKAI)
    }
}
#[derive(Debug, Clone)]
pub struct Thumbnail {
    pub width: u32,
    pub height: u32,
    pub pixels_bgr: Vec<u8>,
    pub pixels_extra: Option<Vec<u8>>,
}
#[derive(Debug, Clone)]
pub struct ScriptStateEntry {
    pub name: [u8; 128],
    pub ip_offset: u32,
    pub refs: [u32; 3],
    pub frame_count: u32,
    pub extra_offsets: [u32; 5],
}
impl Default for ScriptStateEntry {
    fn default() -> Self {
        Self {
            name: [0u8; 128],
            ip_offset: 0,
            refs: [0u8; 3].map(|_| 0),
            frame_count: 0,
            extra_offsets: [0u8; 5].map(|_| 0),
        }
    }
}
#[derive(Debug, Clone)]
pub enum SerValue {
    Int(i32),
    Float(f32),
    String(Arc<[u8]>),
    Array(Arc<ArrayPayload>),
    Complex(Arc<ComplexPayload>),
}
#[derive(Debug, Clone)]
pub struct ArrayPayload {
    pub dims: [u32; 3],
    pub cells: Vec<i32>,
}
#[derive(Debug, Clone)]
pub struct ComplexPayload {
    pub dims: [u32; 3],
    pub entries: Vec<Option<Arc<[u8]>>>,
}
impl SerValue {
    pub fn type_tag(&self) -> u32 {
        match self {
            SerValue::Int(_) => TAG_INT,
            SerValue::Float(_) => TAG_FLOAT,
            SerValue::String(_) => TAG_STRING,
            SerValue::Array(_) => TAG_ARRAY3D,
            SerValue::Complex(_) => TAG_COMPLEX,
        }
    }
}
fn rd_u32(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}
fn wr_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}
pub fn read_sav(data: &[u8]) -> Result<SavFile, SaveError> {
    read_sav_with_layout(data, &SavLayout::OWARUSEKAI)
}
pub fn read_sav_with_layout(
    data: &[u8],
    layout: &SavLayout,
) -> Result<SavFile, SaveError> {
    if data.len() < layout.header_size {
        return Err(SaveError::Truncated {
            needed: layout.header_size,
            have: data.len(),
        });
    }
    let header = read_header(data, layout)?;
    let thumb_size = if header.thumb_w > 0 {
        layout
            .thumb_pixel_size
            .checked_mul(header.thumb_w as usize)
            .and_then(|n| n.checked_mul(header.thumb_h as usize))
            .ok_or(SaveError::SectionSizeMismatch {
                claimed: usize::MAX,
                actual: 0,
            })?
    } else {
        0
    };
    let script_size = SCRIPT_ENTRY_SIZE * (header.script_count as usize + 1);
    let global_size = VALUE_SIZE * header.global_count as usize;
    let thread_size = VALUE_SIZE * header.thread_count as usize;
    let local_size = VALUE_SIZE * header.local_count as usize;
    let extra_size = header.extra_size as usize;
    let off = layout.header_size;
    let thumb_end = off + thumb_size;
    let script_end = thumb_end + script_size;
    let global_end = script_end + global_size;
    let thread_end = global_end + thread_size;
    let local_end = thread_end + local_size;
    let extra_end = local_end + extra_size;
    if extra_end != data.len() {
        return Err(SaveError::SectionSizeMismatch {
            claimed: extra_end,
            actual: data.len(),
        });
    }
    let thumbnail = if header.thumb_w > 0 {
        let raw = &data[off..thumb_end];
        if layout.thumb_pixel_size == 4 {
            let count = raw.len() / 4;
            let mut bgr = Vec::with_capacity(count * 3);
            let mut extra = Vec::with_capacity(count);
            for px in raw.chunks_exact(4) {
                bgr.extend_from_slice(&px[..3]);
                extra.push(px[3]);
            }
            Some(Thumbnail {
                width: header.thumb_w,
                height: header.thumb_h,
                pixels_bgr: bgr,
                pixels_extra: Some(extra),
            })
        } else {
            Some(Thumbnail {
                width: header.thumb_w,
                height: header.thumb_h,
                pixels_bgr: raw.to_vec(),
                pixels_extra: None,
            })
        }
    } else {
        None
    };
    let script_state = read_script_state(&data[thumb_end..script_end])?;
    let global_slots = &data[script_end..global_end];
    let thread_slots = &data[global_end..thread_end];
    let local_slots = &data[thread_end..local_end];
    let extra = &data[local_end..extra_end];
    let global_slot_heads = read_slot_heads(global_slots);
    let thread_slot_heads = read_slot_heads(thread_slots);
    let local_slot_heads = read_slot_heads(local_slots);
    let dimension_layout = detect_save_dimension_layout(
        global_slots,
        thread_slots,
        local_slots,
    );
    let (globals, thread_globals, locals) = deserialize_three_pools_with_layout(
        global_slots,
        thread_slots,
        local_slots,
        extra,
        dimension_layout,
    )?;
    Ok(SavFile {
        header,
        thumbnail,
        script_state,
        global_slot_heads,
        thread_slot_heads,
        local_slot_heads,
        globals,
        thread_globals,
        locals,
    })
}
pub fn write_sav(file: &mut SavFile) -> Result<Vec<u8>, SaveError> {
    write_sav_with_layout(file, &SavLayout::OWARUSEKAI)
}
pub fn write_sav_with_layout(
    file: &mut SavFile,
    layout: &SavLayout,
) -> Result<Vec<u8>, SaveError> {
    let (extra, mut global_slots, mut thread_slots, mut local_slots) = serialize_three_pools(
        &file.globals,
        &file.thread_globals,
        &file.locals,
    )?;
    write_scalar_scopes(&mut global_slots, 1);
    write_scalar_scopes(&mut thread_slots, 2);
    write_scalar_scopes(&mut local_slots, 3);
    write_slot_heads(&mut global_slots, &file.global_slot_heads);
    write_slot_heads(&mut thread_slots, &file.thread_slot_heads);
    write_slot_heads(&mut local_slots, &file.local_slot_heads);
    file.header.global_count = file.globals.len() as u32;
    file.header.thread_count = file.thread_globals.len() as u32;
    file.header.local_count = file.locals.len() as u32;
    file.header.script_count = file
        .script_state
        .len()
        .checked_sub(1)
        .ok_or(SaveError::SectionSizeMismatch {
            claimed: 0,
            actual: 0,
        })? as u32;
    file.header.extra_size = extra.len() as u32;
    let mut out = Vec::with_capacity(
        layout.header_size
            + if file.header.thumb_w > 0 {
                layout.thumb_pixel_size * file.header.thumb_w as usize
                    * file.header.thumb_h as usize
            } else {
                0
            } + SCRIPT_ENTRY_SIZE * file.script_state.len() + global_slots.len()
            + thread_slots.len() + local_slots.len() + extra.len(),
    );
    write_header(&mut out, &file.header, layout);
    if file.header.thumb_w > 0 {
        let pixel_count = file.header.thumb_w as usize * file.header.thumb_h as usize;
        if let Some(t) = &file.thumbnail {
            match (layout.thumb_pixel_size, &t.pixels_extra) {
                (4, extra) => {
                    let alpha = extra.clone().unwrap_or_else(|| vec![0xFF; pixel_count]);
                    for (bgr, a) in t.pixels_bgr.chunks_exact(3).zip(alpha.iter()) {
                        out.extend_from_slice(bgr);
                        out.push(*a);
                    }
                }
                _ => out.extend_from_slice(&t.pixels_bgr),
            }
        } else {
            out.extend(std::iter::repeat_n(0, layout.thumb_pixel_size * pixel_count));
        }
    }
    for entry in &file.script_state {
        write_script_entry(&mut out, entry);
    }
    out.extend_from_slice(&global_slots);
    out.extend_from_slice(&thread_slots);
    out.extend_from_slice(&local_slots);
    out.extend_from_slice(&extra);
    Ok(out)
}
fn read_slot_heads(slots: &[u8]) -> Vec<u32> {
    slots.chunks_exact(VALUE_SIZE).map(|slot| rd_u32(slot, 0)).collect()
}
fn write_slot_heads(slots: &mut [u8], heads: &[u32]) {
    for (slot, head) in slots.chunks_exact_mut(VALUE_SIZE).zip(heads) {
        slot[..4].copy_from_slice(&head.to_le_bytes());
    }
}
fn write_scalar_scopes(slots: &mut [u8], scope: u32) {
    for slot in slots.chunks_exact_mut(VALUE_SIZE) {
        let tag = rd_u32(slot, 4);
        if matches!(tag, TAG_INT | TAG_FLOAT) {
            slot[8..12].copy_from_slice(&scope.to_le_bytes());
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiskDimensionLayout {
    NativeReversed,
    LegacyForward,
    Auto,
}
fn detect_save_dimension_layout(
    global_slots: &[u8],
    thread_slots: &[u8],
    local_slots: &[u8],
) -> DiskDimensionLayout {
    let mut native_votes = 0usize;
    let mut legacy_votes = 0usize;
    for (slots, expected_scope) in [
        (global_slots, 1u32),
        (thread_slots, 2u32),
        (local_slots, 3u32),
    ] {
        for slot in slots.chunks_exact(VALUE_SIZE) {
            if !matches!(rd_u32(slot, 4), TAG_INT | TAG_FLOAT) {
                continue;
            }
            match rd_u32(slot, 8) {
                marker if marker == expected_scope => native_votes += 1,
                0 => legacy_votes += 1,
                _ => {}
            }
        }
    }
    match native_votes.cmp(&legacy_votes) {
        std::cmp::Ordering::Greater => DiskDimensionLayout::NativeReversed,
        std::cmp::Ordering::Less => DiskDimensionLayout::LegacyForward,
        std::cmp::Ordering::Equal => DiskDimensionLayout::Auto,
    }
}
fn runtime_dims_from_disk(raw: [u32; 3], layout: DiskDimensionLayout) -> [u32; 3] {
    match layout {
        DiskDimensionLayout::NativeReversed => [raw[2], raw[1], raw[0]],
        DiskDimensionLayout::LegacyForward => raw,
        DiskDimensionLayout::Auto => {
            if raw[0] == 1 && raw[2] != 1 {
                [raw[2], raw[1], raw[0]]
            } else if raw[2] == 1 && raw[0] != 1 {
                raw
            } else {
                [raw[2], raw[1], raw[0]]
            }
        }
    }
}
fn disk_dims_from_runtime(dims: [u32; 3]) -> [u32; 3] {
    [dims[2], dims[1], dims[0]]
}
pub fn read_sav_meta(data: &[u8]) -> Result<(SavHeader, Option<Thumbnail>), SaveError> {
    read_sav_meta_with_layout(data, &SavLayout::OWARUSEKAI)
}
pub fn read_sav_meta_with_layout(
    data: &[u8],
    layout: &SavLayout,
) -> Result<(SavHeader, Option<Thumbnail>), SaveError> {
    if data.len() < layout.header_size {
        return Err(SaveError::Truncated {
            needed: layout.header_size,
            have: data.len(),
        });
    }
    let header = read_header(data, layout)?;
    let thumb_size = if header.thumb_w > 0 {
        layout.thumb_pixel_size * header.thumb_w as usize * header.thumb_h as usize
    } else {
        0
    };
    if data.len() < layout.header_size + thumb_size {
        return Err(SaveError::Truncated {
            needed: layout.header_size + thumb_size,
            have: data.len(),
        });
    }
    let thumbnail = if header.thumb_w > 0 {
        let raw = &data[layout.header_size..layout.header_size + thumb_size];
        if layout.thumb_pixel_size == 4 {
            let count = raw.len() / 4;
            let mut bgr = Vec::with_capacity(count * 3);
            let mut extra = Vec::with_capacity(count);
            for px in raw.chunks_exact(4) {
                bgr.extend_from_slice(&px[..3]);
                extra.push(px[3]);
            }
            Some(Thumbnail {
                width: header.thumb_w,
                height: header.thumb_h,
                pixels_bgr: bgr,
                pixels_extra: Some(extra),
            })
        } else {
            Some(Thumbnail {
                width: header.thumb_w,
                height: header.thumb_h,
                pixels_bgr: raw.to_vec(),
                pixels_extra: None,
            })
        }
    } else {
        None
    };
    Ok((header, thumbnail))
}
pub fn patch_sav_description(
    data: &mut [u8],
    description: &[u8],
) -> Result<(), SaveError> {
    patch_sav_description_with_layout(data, description, &SavLayout::OWARUSEKAI)
}
pub fn patch_sav_description_with_layout(
    data: &mut [u8],
    description: &[u8],
    layout: &SavLayout,
) -> Result<(), SaveError> {
    if data.len() < layout.header_size {
        return Err(SaveError::Truncated {
            needed: layout.header_size,
            have: data.len(),
        });
    }
    let mut buf = [0u8; 128];
    let n = description.len().min(127);
    buf[..n].copy_from_slice(&description[..n]);
    data[OFF_DESCRIPTION..OFF_DESCRIPTION + 128].copy_from_slice(&buf);
    Ok(())
}
fn read_header(data: &[u8], layout: &SavLayout) -> Result<SavHeader, SaveError> {
    let mut magic = [0u8; 16];
    magic.copy_from_slice(&data[..16]);
    if &magic[..15] != layout.magic {
        return Err(SaveError::BadMagic {
            expected: layout.magic,
            actual: magic.to_vec(),
        });
    }
    let mut description = [0u8; 128];
    description.copy_from_slice(&data[OFF_DESCRIPTION..OFF_DESCRIPTION + 128]);
    let header_gap = data[OFF_SCRIPT_COUNT..layout.count_block].to_vec();
    let cb = layout.count_block;
    let audio = layout.off_audio_blob();
    let audio_blob = data[audio..audio + layout.audio_blob_size()].to_vec();
    let tfr = layout.off_text_first_render();
    let tsb = layout.off_text_state_blob();
    let text_state_blob = data[tsb..tsb + layout.text_state_blob_size()].to_vec();
    Ok(SavHeader {
        magic,
        description,
        header_gap,
        script_count: rd_u32(data, cb),
        thumb_w: rd_u32(data, cb + 0x04),
        thumb_h: rd_u32(data, cb + 0x08),
        global_count: rd_u32(data, cb + 0x0C),
        thread_count: rd_u32(data, cb + 0x10),
        local_count: rd_u32(data, cb + 0x14),
        extra_size: rd_u32(data, cb + 0x18),
        audio_blob,
        text_first_render: rd_u32(data, tfr),
        text_state_blob,
    })
}
fn write_header(out: &mut Vec<u8>, h: &SavHeader, layout: &SavLayout) {
    let mut magic = [0u8; 16];
    magic[..15].copy_from_slice(layout.magic);
    out.extend_from_slice(&magic);
    out.extend_from_slice(&h.description);
    let gap_size = layout.header_gap_size();
    debug_assert!(
        h.header_gap.len() <= gap_size, "header_gap larger than layout: {} > {}", h
        .header_gap.len(), gap_size
    );
    out.extend_from_slice(&h.header_gap);
    out.extend(std::iter::repeat_n(0, gap_size - h.header_gap.len()));
    let mut counts = [0u8; 0x1C];
    counts[0x00..0x04].copy_from_slice(&h.script_count.to_le_bytes());
    counts[0x04..0x08].copy_from_slice(&h.thumb_w.to_le_bytes());
    counts[0x08..0x0C].copy_from_slice(&h.thumb_h.to_le_bytes());
    counts[0x0C..0x10].copy_from_slice(&h.global_count.to_le_bytes());
    counts[0x10..0x14].copy_from_slice(&h.thread_count.to_le_bytes());
    counts[0x14..0x18].copy_from_slice(&h.local_count.to_le_bytes());
    counts[0x18..0x1C].copy_from_slice(&h.extra_size.to_le_bytes());
    out.extend_from_slice(&counts);
    debug_assert_eq!(
        h.audio_blob.len(), layout.audio_blob_size(), "audio_blob size drift"
    );
    out.extend_from_slice(&h.audio_blob);
    out.extend_from_slice(&h.text_first_render.to_le_bytes());
    debug_assert_eq!(
        h.text_state_blob.len(), layout.text_state_blob_size(),
        "text_state_blob size drift"
    );
    out.extend_from_slice(&h.text_state_blob);
    debug_assert_eq!(out.len(), layout.header_size);
}
fn read_script_state(buf: &[u8]) -> Result<Vec<ScriptStateEntry>, SaveError> {
    if !buf.len().is_multiple_of(SCRIPT_ENTRY_SIZE) {
        return Err(SaveError::Truncated {
            needed: buf.len() + (SCRIPT_ENTRY_SIZE - buf.len() % SCRIPT_ENTRY_SIZE),
            have: buf.len(),
        });
    }
    let n = buf.len() / SCRIPT_ENTRY_SIZE;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let base = i * SCRIPT_ENTRY_SIZE;
        let mut name = [0u8; 128];
        name.copy_from_slice(&buf[base..base + SCRIPT_NAME_SIZE]);
        let m = base + SCRIPT_NAME_SIZE;
        out.push(ScriptStateEntry {
            name,
            ip_offset: rd_u32(buf, m),
            refs: [rd_u32(buf, m + 4), rd_u32(buf, m + 8), rd_u32(buf, m + 12)],
            frame_count: rd_u32(buf, m + 16),
            extra_offsets: [
                rd_u32(buf, m + 20),
                rd_u32(buf, m + 24),
                rd_u32(buf, m + 28),
                rd_u32(buf, m + 32),
                rd_u32(buf, m + 36),
            ],
        });
    }
    Ok(out)
}
fn write_script_entry(out: &mut Vec<u8>, e: &ScriptStateEntry) {
    out.extend_from_slice(&e.name);
    let m = out.len();
    wr_u32(out, e.ip_offset);
    wr_u32(out, e.refs[0]);
    wr_u32(out, e.refs[1]);
    wr_u32(out, e.refs[2]);
    wr_u32(out, e.frame_count);
    wr_u32(out, e.extra_offsets[0]);
    wr_u32(out, e.extra_offsets[1]);
    wr_u32(out, e.extra_offsets[2]);
    wr_u32(out, e.extra_offsets[3]);
    wr_u32(out, e.extra_offsets[4]);
    debug_assert_eq!(out.len() - m, 40);
}
pub type SerializedPools = (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>);
fn disk_string_len(bytes: &[u8]) -> usize {
    bytes.len() + usize::from(!bytes.ends_with(&[0]))
}
pub fn serialize_three_pools(
    globals: &[SerValue],
    thread_globals: &[SerValue],
    locals: &[SerValue],
) -> Result<SerializedPools, SaveError> {
    let mut ser = Serializer::new();
    ser.count_pool(globals)?;
    ser.count_pool(thread_globals)?;
    ser.count_pool(locals)?;
    let extra_cap = ser.total_object_size;
    ser.extra = Vec::with_capacity(extra_cap);
    ser.offsets.clear();
    let global_slots = ser.write_pool(globals)?;
    let thread_slots = ser.write_pool(thread_globals)?;
    let local_slots = ser.write_pool(locals)?;
    Ok((ser.extra, global_slots, thread_slots, local_slots))
}
struct Serializer {
    sizes: Vec<(usize, usize)>,
    offsets: Vec<(usize, usize)>,
    total_object_size: usize,
    extra: Vec<u8>,
}
impl Serializer {
    fn new() -> Self {
        Self {
            sizes: Vec::new(),
            offsets: Vec::new(),
            total_object_size: 0,
            extra: Vec::new(),
        }
    }
    fn count_pool(&mut self, pool: &[SerValue]) -> Result<(), SaveError> {
        for v in pool {
            self.count_value(v)?;
        }
        Ok(())
    }
    fn count_value(&mut self, v: &SerValue) -> Result<(), SaveError> {
        match v {
            SerValue::Int(_) | SerValue::Float(_) => Ok(()),
            SerValue::String(s) => {
                let key = Arc::as_ptr(s) as *const () as usize;
                if !self.sizes.iter().any(|(k, _)| *k == key) {
                    let size = STRING_HEADER_SIZE + disk_string_len(s);
                    self.sizes.push((key, size));
                    self.total_object_size += size;
                }
                Ok(())
            }
            SerValue::Array(a) => {
                let key = Arc::as_ptr(a) as *const () as usize;
                if !self.sizes.iter().any(|(k, _)| *k == key) {
                    let cells = a.cells.len();
                    let size = ARRAY_OBJECT_BASE_SIZE + 4 * cells;
                    self.sizes.push((key, size));
                    self.total_object_size += size;
                }
                Ok(())
            }
            SerValue::Complex(c) => {
                let key = Arc::as_ptr(c) as *const () as usize;
                if !self.sizes.iter().any(|(k, _)| *k == key) {
                    let n = c.entries.len();
                    let size = COMPLEX_HEADER_SIZE + 4 * n;
                    for cs in c.entries.iter().flatten() {
                        let ck = Arc::as_ptr(cs) as *const () as usize;
                        if !self.sizes.iter().any(|(k, _)| *k == ck) {
                            let csize = STRING_HEADER_SIZE + disk_string_len(cs);
                            self.sizes.push((ck, csize));
                            self.total_object_size += csize;
                        }
                    }
                    self.sizes.push((key, size));
                    self.total_object_size += size;
                }
                Ok(())
            }
        }
    }
    fn write_pool(&mut self, pool: &[SerValue]) -> Result<Vec<u8>, SaveError> {
        let mut slots = Vec::with_capacity(pool.len() * VALUE_SIZE);
        for v in pool {
            self.write_value(v, &mut slots)?;
        }
        Ok(slots)
    }
    fn write_value(
        &mut self,
        v: &SerValue,
        slots: &mut Vec<u8>,
    ) -> Result<(), SaveError> {
        let tag = v.type_tag();
        match v {
            SerValue::Int(i) => {
                wr_u32(slots, 0);
                wr_u32(slots, tag);
                wr_u32(slots, 0);
                wr_u32(slots, *i as u32);
            }
            SerValue::Float(f) => {
                wr_u32(slots, 0);
                wr_u32(slots, tag);
                wr_u32(slots, 0);
                wr_u32(slots, f.to_bits());
            }
            SerValue::String(s) => {
                let off = self.ensure_string(s)? as u32;
                wr_u32(slots, 0);
                wr_u32(slots, tag);
                wr_u32(slots, off);
                wr_u32(slots, 0);
            }
            SerValue::Array(a) => {
                let off = self.ensure_array(a)? as u32;
                wr_u32(slots, 0);
                wr_u32(slots, tag);
                wr_u32(slots, off);
                wr_u32(slots, 0);
            }
            SerValue::Complex(c) => {
                let off = self.ensure_complex(c)? as u32;
                wr_u32(slots, 0);
                wr_u32(slots, tag);
                wr_u32(slots, off);
                wr_u32(slots, 0);
            }
        }
        Ok(())
    }
    fn ensure_string(&mut self, s: &Arc<[u8]>) -> Result<usize, SaveError> {
        let key = Arc::as_ptr(s) as *const () as usize;
        if let Some((_, off)) = self.offsets.iter().find(|(k, _)| *k == key) {
            return Ok(*off);
        }
        let off = self.extra.len();
        let length = disk_string_len(s);
        wr_u32(&mut self.extra, 1);
        wr_u32(&mut self.extra, off as u32);
        wr_u32(&mut self.extra, length as u32);
        self.extra.extend_from_slice(s);
        if !s.ends_with(&[0]) {
            self.extra.push(0);
        }
        self.offsets.push((key, off));
        Ok(off)
    }
    fn ensure_array(&mut self, a: &Arc<ArrayPayload>) -> Result<usize, SaveError> {
        let key = Arc::as_ptr(a) as *const () as usize;
        if let Some((_, off)) = self.offsets.iter().find(|(k, _)| *k == key) {
            return Ok(*off);
        }
        let off = self.extra.len();
        wr_u32(&mut self.extra, 1);
        wr_u32(&mut self.extra, off as u32);
        let disk_dims = disk_dims_from_runtime(a.dims);
        wr_u32(&mut self.extra, disk_dims[0]);
        wr_u32(&mut self.extra, disk_dims[1]);
        wr_u32(&mut self.extra, disk_dims[2]);
        for &cell in &a.cells {
            wr_u32(&mut self.extra, cell as u32);
        }
        wr_u32(&mut self.extra, 0);
        debug_assert_eq!(
            self.extra.len() - off, ARRAY_OBJECT_BASE_SIZE + 4 * a.cells.len()
        );
        self.offsets.push((key, off));
        Ok(off)
    }
    fn ensure_complex(&mut self, c: &Arc<ComplexPayload>) -> Result<usize, SaveError> {
        let key = Arc::as_ptr(c) as *const () as usize;
        if let Some((_, off)) = self.offsets.iter().find(|(k, _)| *k == key) {
            return Ok(*off);
        }
        let off = self.extra.len();
        wr_u32(&mut self.extra, 1);
        wr_u32(&mut self.extra, off as u32);
        let disk_dims = disk_dims_from_runtime(c.dims);
        wr_u32(&mut self.extra, disk_dims[0]);
        wr_u32(&mut self.extra, disk_dims[1]);
        wr_u32(&mut self.extra, disk_dims[2]);
        wr_u32(&mut self.extra, 0);
        let n = c.entries.len();
        let table_pos = self.extra.len();
        debug_assert_eq!(table_pos, off + COMPLEX_HEADER_SIZE);
        self.extra.extend(std::iter::repeat_n(0u8, 4 * n));
        let mut child_offsets = Vec::with_capacity(n);
        for child in &c.entries {
            let entry_off = match child {
                None => 0xFFFF_FFFFu32,
                Some(cs) => self.ensure_string(cs)? as u32,
            };
            child_offsets.push(entry_off);
        }
        for (i, eo) in child_offsets.iter().enumerate() {
            let p = table_pos + i * 4;
            self.extra[p..p + 4].copy_from_slice(&eo.to_le_bytes());
        }
        self.offsets.push((key, off));
        Ok(off)
    }
}
type DeserializedPools = (Vec<SerValue>, Vec<SerValue>, Vec<SerValue>);
fn deserialize_three_pools_with_layout(
    global_slots: &[u8],
    thread_slots: &[u8],
    local_slots: &[u8],
    extra: &[u8],
    dimension_layout: DiskDimensionLayout,
) -> Result<DeserializedPools, SaveError> {
    let mut de = Deserializer {
        extra,
        seen: Vec::new(),
        dimension_layout,
    };
    let globals = de.read_pool(global_slots)?;
    let thread_globals = de.read_pool(thread_slots)?;
    let locals = de.read_pool(local_slots)?;
    Ok((globals, thread_globals, locals))
}
struct Deserializer<'a> {
    extra: &'a [u8],
    seen: Vec<(usize, BuiltObject)>,
    dimension_layout: DiskDimensionLayout,
}
enum BuiltObject {
    Str(Arc<[u8]>),
    Array(Arc<ArrayPayload>),
    Complex(Arc<ComplexPayload>),
}
impl<'a> Deserializer<'a> {
    fn read_pool(&mut self, slots: &[u8]) -> Result<Vec<SerValue>, SaveError> {
        if !slots.len().is_multiple_of(VALUE_SIZE) {
            return Err(SaveError::Truncated {
                needed: slots.len() + (VALUE_SIZE - slots.len() % VALUE_SIZE),
                have: slots.len(),
            });
        }
        let n = slots.len() / VALUE_SIZE;
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let base = i * VALUE_SIZE;
            let tag = rd_u32(slots, base + 4);
            let offset_marker = rd_u32(slots, base + 8);
            let data = rd_u32(slots, base + 12);
            let v = match tag {
                TAG_INT => SerValue::Int(data as i32),
                TAG_FLOAT => SerValue::Float(f32::from_bits(data)),
                TAG_STRING => self.read_ref(TAG_STRING, offset_marker as usize)?,
                TAG_ARRAY3D | TAG_ARRAY3D_ALT => {
                    self.read_ref(TAG_ARRAY3D, offset_marker as usize)?
                }
                TAG_COMPLEX => self.read_ref(TAG_COMPLEX, offset_marker as usize)?,
                other => return Err(SaveError::InvalidTypeTag(other)),
            };
            out.push(v);
        }
        Ok(out)
    }
    fn read_ref(&mut self, tag: u32, off: usize) -> Result<SerValue, SaveError> {
        if off >= self.extra.len() {
            return Err(SaveError::OffsetOutOfBounds {
                offset: off,
                extra_len: self.extra.len(),
            });
        }
        if let Some((_, b)) = self.seen.iter().find(|(k, _)| *k == off) {
            return Ok(
                match b {
                    BuiltObject::Str(a) => SerValue::String(a.clone()),
                    BuiltObject::Array(a) => SerValue::Array(a.clone()),
                    BuiltObject::Complex(a) => SerValue::Complex(a.clone()),
                },
            );
        }
        match tag {
            TAG_STRING => {
                let s = self.build_string(off)?;
                self.seen.push((off, BuiltObject::Str(s.clone())));
                Ok(SerValue::String(s))
            }
            TAG_ARRAY3D | TAG_ARRAY3D_ALT => {
                let a = self.build_array(off)?;
                self.seen.push((off, BuiltObject::Array(a.clone())));
                Ok(SerValue::Array(a))
            }
            TAG_COMPLEX => {
                let c = self.build_complex(off)?;
                self.seen.push((off, BuiltObject::Complex(c.clone())));
                Ok(SerValue::Complex(c))
            }
            other => Err(SaveError::InvalidTypeTag(other)),
        }
    }
    fn build_string(&self, off: usize) -> Result<Arc<[u8]>, SaveError> {
        if off + STRING_HEADER_SIZE > self.extra.len() {
            return Err(SaveError::BadObjectHeader {
                offset: off,
                detail: "string header OOB",
            });
        }
        let length = rd_u32(self.extra, off + 8) as usize;
        let data_start = off + STRING_HEADER_SIZE;
        if length == 0 {
            return Err(SaveError::BadObjectHeader {
                offset: off,
                detail: "string length excludes trailing NUL",
            });
        }
        let Some(data_end) = data_start.checked_add(length) else {
            return Err(SaveError::BadObjectHeader {
                offset: off,
                detail: "string length overflow",
            });
        };
        if data_end > self.extra.len() {
            return Err(SaveError::BadObjectHeader {
                offset: off,
                detail: "string length OOB",
            });
        }
        let bytes = &self.extra[data_start..data_end];
        if !bytes.ends_with(&[0]) {
            return Err(SaveError::BadObjectHeader {
                offset: off,
                detail: "string is not NUL-terminated",
            });
        }
        Ok(bytes.into())
    }
    fn build_array(&self, off: usize) -> Result<Arc<ArrayPayload>, SaveError> {
        if off + ARRAY_DATA_OFFSET > self.extra.len() {
            return Err(SaveError::BadObjectHeader {
                offset: off,
                detail: "array header OOB",
            });
        }
        let raw_dims = [
            rd_u32(self.extra, off + 8),
            rd_u32(self.extra, off + 12),
            rd_u32(self.extra, off + 16),
        ];
        let dims = runtime_dims_from_disk(raw_dims, self.dimension_layout);
        let n = dims[0] as usize * dims[1] as usize * dims[2] as usize;
        let data_start = off + ARRAY_DATA_OFFSET;
        let data_end = data_start + 4 * n;
        if data_end > self.extra.len() {
            return Err(SaveError::BadObjectHeader {
                offset: off,
                detail: "array data OOB",
            });
        }
        let mut cells = Vec::with_capacity(n);
        for i in 0..n {
            cells.push(rd_u32(self.extra, data_start + i * 4) as i32);
        }
        Ok(Arc::new(ArrayPayload { dims, cells }))
    }
    fn build_complex(&mut self, off: usize) -> Result<Arc<ComplexPayload>, SaveError> {
        if off + COMPLEX_HEADER_SIZE > self.extra.len() {
            return Err(SaveError::BadObjectHeader {
                offset: off,
                detail: "complex header OOB",
            });
        }
        let raw_dims = [
            rd_u32(self.extra, off + 8),
            rd_u32(self.extra, off + 12),
            rd_u32(self.extra, off + 16),
        ];
        let dims = runtime_dims_from_disk(raw_dims, self.dimension_layout);
        let n = dims[0] as usize * dims[1] as usize * dims[2] as usize;
        let table_start = off + COMPLEX_HEADER_SIZE;
        let table_end = table_start + 4 * n;
        if table_end > self.extra.len() {
            return Err(SaveError::BadObjectHeader {
                offset: off,
                detail: "complex table OOB",
            });
        }
        let mut entries = Vec::with_capacity(n);
        for i in 0..n {
            let ptr_off = rd_u32(self.extra, table_start + i * 4);
            if ptr_off == 0xFFFF_FFFF {
                entries.push(None);
            } else {
                let child_off = ptr_off as usize;
                match self.read_ref(TAG_STRING, child_off)? {
                    SerValue::String(s) => entries.push(Some(s)),
                    _ => {
                        return Err(SaveError::BadObjectHeader {
                            offset: child_off,
                            detail: "complex child not a string",
                        });
                    }
                }
            }
        }
        Ok(Arc::new(ComplexPayload { dims, entries }))
    }
}
#[derive(Debug, Clone, Default)]
pub struct MssFile {
    pub keys: Vec<u32>,
    pub values: Vec<SerValue>,
    pub header_tail: Vec<u8>,
}
impl MssFile {
    pub fn picture_hashes(&self) -> impl Iterator<Item = u32> + '_ {
        self.header_tail
            .get(MSS_PICTURE_HASHES_TAIL_OFFSET..MSS_PICTURE_HASHES_TAIL_END)
            .unwrap_or(&[])
            .chunks_exact(4)
            .map(|bytes| u32::from_le_bytes(
                bytes.try_into().expect("four-byte picture hash"),
            ))
            .filter(|hash| *hash != 0)
    }
    pub fn set_picture_hashes(&mut self, hashes: impl IntoIterator<Item = u32>) {
        if self.header_tail.len() != MSS_HEADER_TAIL_SIZE {
            self.header_tail.resize(MSS_HEADER_TAIL_SIZE, 0);
        }
        self.header_tail[MSS_PICTURE_HASHES_TAIL_OFFSET..MSS_PICTURE_HASHES_TAIL_END]
            .fill(0);
        for (slot, hash) in self
            .header_tail[MSS_PICTURE_HASHES_TAIL_OFFSET..MSS_PICTURE_HASHES_TAIL_END]
            .chunks_exact_mut(4)
            .zip(hashes.into_iter().filter(|hash| *hash != 0))
        {
            slot.copy_from_slice(&hash.to_le_bytes());
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadmarkRecord {
    pub name: [u8; SCRIPT_NAME_SIZE],
    pub bit_count: u32,
    pub data: Vec<u8>,
}
pub fn read_readmarks(data: &[u8]) -> Result<Vec<ReadmarkRecord>, SaveError> {
    let mut records = Vec::new();
    let mut offset = 0usize;
    while offset < data.len() {
        if data.len() - offset < READMARK_HEADER_SIZE {
            return Err(SaveError::Truncated {
                needed: offset + READMARK_HEADER_SIZE,
                have: data.len(),
            });
        }
        let header = &data[offset..offset + READMARK_HEADER_SIZE];
        let data_size = rd_u32(header, 0x80) as usize;
        let bit_count = rd_u32(header, 0x84);
        let data_start = offset + READMARK_HEADER_SIZE;
        let data_end = data_start
            .checked_add(data_size)
            .ok_or(SaveError::SectionSizeMismatch {
                claimed: usize::MAX,
                actual: data.len(),
            })?;
        if data_end > data.len() {
            return Err(SaveError::Truncated {
                needed: data_end,
                have: data.len(),
            });
        }
        let mut name = [0u8; SCRIPT_NAME_SIZE];
        name.copy_from_slice(&header[..SCRIPT_NAME_SIZE]);
        records
            .push(ReadmarkRecord {
                name,
                bit_count,
                data: data[data_start..data_end].to_vec(),
            });
        offset = data_end;
    }
    Ok(records)
}
pub fn write_readmarks(records: &[ReadmarkRecord]) -> Result<Vec<u8>, SaveError> {
    let mut out = Vec::new();
    for record in records {
        let data_size = u32::try_from(record.data.len())
            .map_err(|_| SaveError::SectionSizeMismatch {
                claimed: record.data.len(),
                actual: u32::MAX as usize,
            })?;
        let mut header = [0u8; READMARK_HEADER_SIZE];
        header[..SCRIPT_NAME_SIZE].copy_from_slice(&record.name);
        header[0x80..0x84].copy_from_slice(&data_size.to_le_bytes());
        header[0x84..0x88].copy_from_slice(&record.bit_count.to_le_bytes());
        out.extend_from_slice(&header);
        out.extend_from_slice(&record.data);
    }
    Ok(out)
}
const MSS_HEADER_TAIL_SIZE: usize = MSS_HEADER_SIZE - 16;
const MSS_OFF_VALUE_COUNT: usize = 0x10;
const MSS_OFF_EXTRA_SIZE: usize = 0x14;
pub const MSS_PICTURE_HASH_CAPACITY: usize = 10_000;
const MSS_PICTURE_HASHES_TAIL_OFFSET: usize = 0x18 - 16;
const MSS_PICTURE_HASHES_TAIL_END: usize = MSS_PICTURE_HASHES_TAIL_OFFSET
    + MSS_PICTURE_HASH_CAPACITY * 4;
pub fn read_mss(data: &[u8]) -> Result<MssFile, SaveError> {
    if data.len() < MSS_HEADER_SIZE {
        return Err(SaveError::Truncated {
            needed: MSS_HEADER_SIZE,
            have: data.len(),
        });
    }
    if &data[..15] != MAGIC_MSS {
        return Err(SaveError::BadMagic {
            expected: MAGIC_MSS,
            actual: data[..16].to_vec(),
        });
    }
    let value_count = rd_u32(data, MSS_OFF_VALUE_COUNT) as usize;
    let extra_size = rd_u32(data, MSS_OFF_EXTRA_SIZE) as usize;
    let values_size = VALUE_SIZE * value_count;
    let values_end = MSS_HEADER_SIZE + values_size;
    let extra_end = values_end + extra_size;
    if extra_end != data.len() {
        return Err(SaveError::SectionSizeMismatch {
            claimed: extra_end,
            actual: data.len(),
        });
    }
    let slots = &data[MSS_HEADER_SIZE..values_end];
    let keys = slots.chunks_exact(VALUE_SIZE).map(|slot| rd_u32(slot, 0)).collect();
    let mut de = Deserializer {
        extra: &data[values_end..extra_end],
        seen: Vec::new(),
        dimension_layout: DiskDimensionLayout::Auto,
    };
    let values = de.read_pool(slots)?;
    Ok(MssFile {
        keys,
        values,
        header_tail: data[16..MSS_HEADER_SIZE].to_vec(),
    })
}
pub fn write_mss(file: &mut MssFile) -> Result<Vec<u8>, SaveError> {
    if file.keys.len() != file.values.len() {
        return Err(SaveError::SectionSizeMismatch {
            claimed: file.values.len(),
            actual: file.keys.len(),
        });
    }
    let (extra, mut slots, _, _) = serialize_three_pools(&file.values, &[], &[])?;
    for (slot, key) in slots.chunks_exact_mut(VALUE_SIZE).zip(&file.keys) {
        slot[..4].copy_from_slice(&key.to_le_bytes());
    }
    let value_count = file.values.len() as u32;
    let extra_size = extra.len() as u32;
    let mut out = Vec::with_capacity(MSS_HEADER_SIZE + slots.len() + extra.len());
    let mut magic = [0u8; 16];
    magic[..15].copy_from_slice(MAGIC_MSS);
    out.extend_from_slice(&magic);
    if file.header_tail.len() != MSS_HEADER_TAIL_SIZE {
        file.header_tail.resize(MSS_HEADER_TAIL_SIZE, 0);
    }
    let vc_off = MSS_OFF_VALUE_COUNT - 16;
    let es_off = MSS_OFF_EXTRA_SIZE - 16;
    file.header_tail[vc_off..vc_off + 4].copy_from_slice(&value_count.to_le_bytes());
    file.header_tail[es_off..es_off + 4].copy_from_slice(&extra_size.to_le_bytes());
    out.extend_from_slice(&file.header_tail);
    out.extend_from_slice(&slots);
    out.extend_from_slice(&extra);
    Ok(out)
}

