use crate::crypto::CRC32_TABLE;
use std::fmt;
pub const MAJIRO_IMAGE_MAGIC: u32 = 0x9A92_5A98;
#[derive(Debug, Clone)]
pub struct MajiroImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    pub indexed_pixels: Option<Vec<u8>>,
    pub indexed_palette: Option<Vec<[u8; 4]>>,
    pub subtype: String,
    pub class_name: Option<String>,
}
#[derive(Debug)]
pub enum ImageError {
    TooSmall,
    BadMagic(u32),
    UnknownSubtype(String),
    InvalidDimensions { width: u32, height: u32 },
    DecompressError(String),
    RctNeedsFilename,
    Io(std::io::Error),
}
impl fmt::Display for ImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImageError::TooSmall => write!(f, "file too small for Majiro image header"),
            ImageError::BadMagic(m) => write!(f, "bad magic: 0x{:08X}", m),
            ImageError::UnknownSubtype(s) => write!(f, "unknown sub-type: {}", s),
            ImageError::InvalidDimensions { width, height } => {
                write!(f, "invalid dimensions: {}x{}", width, height)
            }
            ImageError::DecompressError(s) => write!(f, "decompression error: {}", s),
            ImageError::RctNeedsFilename => {
                write!(
                    f,
                    "RCT format requires a filename for key generation. Use parse_majiro_image_with_name()."
                )
            }
            ImageError::Io(e) => write!(f, "I/O error: {}", e),
        }
    }
}
impl std::error::Error for ImageError {}
impl From<std::io::Error> for ImageError {
    fn from(e: std::io::Error) -> Self {
        ImageError::Io(e)
    }
}
pub fn parse_majiro_image(data: &[u8]) -> Result<MajiroImage, ImageError> {
    parse_majiro_image_with_name(data, None)
}
pub fn parse_majiro_image_with_name(
    data: &[u8],
    filename: Option<&str>,
) -> Result<MajiroImage, ImageError> {
    let key = get_default_key();
    parse_majiro_image_with_key(data, filename, &key)
}
pub fn parse_majiro_image_with_key(
    data: &[u8],
    filename: Option<&str>,
    rct_key: &[u8; 1024],
) -> Result<MajiroImage, ImageError> {
    if data.len() < 20 {
        return Err(ImageError::TooSmall);
    }
    let magic = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    if magic != MAJIRO_IMAGE_MAGIC {
        return Err(ImageError::BadMagic(magic));
    }
    let subtype_bytes = [data[4], data[5], data[6], data[7]];
    let subtype = String::from_utf8_lossy(&subtype_bytes).to_string();
    let width = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
    let height = u32::from_le_bytes([data[12], data[13], data[14], data[15]]);
    let compressed_size = u32::from_le_bytes([data[16], data[17], data[18], data[19]])
        as usize;
    if width == 0 || height == 0 || width > 16384 || height > 16384 {
        return Err(ImageError::InvalidDimensions {
            width,
            height,
        });
    }
    match subtype.as_str() {
        "8_00" => parse_rc8(data, width, height, compressed_size),
        "TS00" | "TS01" => {
            let _ = filename;
            parse_rct_ts(data, width, height, compressed_size, &subtype, rct_key)
        }
        "TC00" | "TC01" => parse_rct_tc(data, width, height, compressed_size, &subtype),
        _ => Err(ImageError::UnknownSubtype(subtype)),
    }
}
pub fn get_default_key() -> [u8; 1024] {
    build_key_from_hash(OSTB_DEFAULT_KEY_HASH)
}
const OSTB_DEFAULT_KEY_HASH: u32 = 0x9CACE44B;
pub fn build_key_from_hash(hash: u32) -> [u8; 1024] {
    let lo = (hash & 0xFF) as usize;
    let mut key = [0u8; 1024];
    for i in 0..256 {
        let val = hash ^ CRC32_TABLE[(i + lo) % 256];
        let off = i * 4;
        key[off..off + 4].copy_from_slice(&val.to_le_bytes());
    }
    key
}
pub fn build_key_from_bytes(key_string: &[u8]) -> [u8; 1024] {
    let bytes = key_string.split(|byte| *byte == 0).next().unwrap_or_default();
    build_key_from_hash(crate::crypto::crc32(bytes))
}
pub fn decrypt_rct_data(data: &mut [u8], key: &[u8; 1024]) {
    for chunk in data.chunks_mut(1024) {
        let len = chunk.len().min(1024);
        for i in 0..len {
            chunk[i] ^= key[i];
        }
    }
}
fn parse_rc8(
    data: &[u8],
    width: u32,
    height: u32,
    compressed_size: usize,
) -> Result<MajiroImage, ImageError> {
    let palette_offset = 0x14;
    let compressed_offset = 0x314;
    if data.len() < compressed_offset + compressed_size {
        return Err(ImageError::TooSmall);
    }
    let palette_rgb = &data[palette_offset..palette_offset + 768];
    let compressed_data = &data[compressed_offset..compressed_offset + compressed_size];
    let pixel_count = (width as usize) * (height as usize);
    let indices = majiro_decompress_rc8(compressed_data, width as usize, pixel_count)?;
    let palette = (0..256)
        .map(|index| {
            let offset = index * 3;
            [palette_rgb[offset + 2], palette_rgb[offset + 1], palette_rgb[offset], 0xFF]
        })
        .collect::<Vec<_>>();
    let mut pixels = vec![0u8; pixel_count * 4];
    for (i, &idx) in indices.iter().enumerate() {
        let out_off = i * 4;
        pixels[out_off..out_off + 4].copy_from_slice(&palette[idx as usize]);
    }
    Ok(MajiroImage {
        width,
        height,
        pixels,
        indexed_pixels: Some(indices),
        indexed_palette: Some(palette),
        subtype: "8_00".to_string(),
        class_name: None,
    })
}
pub fn majiro_decompress_rc8(
    compressed: &[u8],
    width: usize,
    pixel_count: usize,
) -> Result<Vec<u8>, ImageError> {
    if pixel_count == 0 || compressed.is_empty() {
        return Ok(Vec::new());
    }
    let output_size = pixel_count;
    let mut output = vec![0u8; output_size];
    let mut in_pos: usize = 0;
    let w = width as isize;
    let offsets: [isize; 16] = [
        -1,
        -2,
        -3,
        -4,
        3 - w,
        2 - w,
        1 - w,
        -w,
        -1 - w,
        -2 - w,
        -3 - w,
        2 - 2 * w,
        1 - 2 * w,
        -2 * w,
        -1 - 2 * w,
        2 * (-1 - w),
    ];
    if in_pos >= compressed.len() {
        return Err(ImageError::DecompressError("empty input".into()));
    }
    output[0] = compressed[0];
    let mut out_pos: usize = 1;
    in_pos = 1;
    while out_pos < output_size {
        if in_pos >= compressed.len() {
            return Err(ImageError::DecompressError("premature end of input".into()));
        }
        let cmd = compressed[in_pos];
        in_pos += 1;
        if cmd < 0x80 {
            let len: usize = if cmd == 0x7F {
                if in_pos + 2 > compressed.len() {
                    return Err(
                        ImageError::DecompressError("extended literal EOF".into()),
                    );
                }
                let ext = u16::from_le_bytes([
                    compressed[in_pos],
                    compressed[in_pos + 1],
                ]) as usize;
                in_pos += 2;
                ext + 128
            } else {
                cmd as usize + 1
            };
            if in_pos + len > compressed.len() || out_pos + len > output_size {
                return Err(ImageError::DecompressError("literal run overflow".into()));
            }
            output[out_pos..out_pos + len]
                .copy_from_slice(&compressed[in_pos..in_pos + len]);
            out_pos += len;
            in_pos += len;
        } else {
            let offset_idx = ((cmd >> 3) & 0xF) as usize;
            let len_code = cmd & 7;
            let len: usize = if len_code == 7 {
                if in_pos + 2 > compressed.len() {
                    return Err(
                        ImageError::DecompressError("extended back-ref EOF".into()),
                    );
                }
                let ext = u16::from_le_bytes([
                    compressed[in_pos],
                    compressed[in_pos + 1],
                ]) as usize;
                in_pos += 2;
                ext + 10
            } else {
                len_code as usize + 3
            };
            let offset = offsets[offset_idx];
            let src_pos = out_pos as isize + offset;
            if src_pos < 0 {
                return Err(
                    ImageError::DecompressError(
                        format!(
                            "back-ref underflow: out_pos={}, idx={}, off={}", out_pos,
                            offset_idx, offset
                        ),
                    ),
                );
            }
            let src = src_pos as usize;
            if out_pos + len > output_size {
                return Err(ImageError::DecompressError("back-ref overflow".into()));
            }
            for i in 0..len {
                output[out_pos + i] = output[src + i];
            }
            out_pos += len;
        }
    }
    Ok(output)
}
fn parse_rct_ts(
    data: &[u8],
    width: u32,
    height: u32,
    compressed_size: usize,
    subtype: &str,
    key: &[u8; 1024],
) -> Result<MajiroImage, ImageError> {
    let is_ts01 = subtype == "TS01";
    let (compressed_data, class_name) = extract_rct_payload(
        data,
        compressed_size,
        is_ts01,
    )?;
    let mut comp = compressed_data.to_vec();
    decrypt_rct_data(&mut comp, key);
    if comp.len() >= 2 && comp[comp.len() - 2] == b'T' && comp[comp.len() - 1] == b'S' {
        comp.truncate(comp.len() - 2);
        let pixel_count = (width as usize) * (height as usize);
        if let Ok(bgr) = majiro_decompress_rct(&comp, width as usize, pixel_count) {
            let mut pixels = vec![0u8; pixel_count * 4];
            for (i, chunk) in bgr.chunks(3).enumerate() {
                if i >= pixel_count || chunk.len() < 3 {
                    break;
                }
                let out_off = i * 4;
                pixels[out_off] = chunk[2];
                pixels[out_off + 1] = chunk[1];
                pixels[out_off + 2] = chunk[0];
                pixels[out_off + 3] = 0xFF;
            }
            return Ok(MajiroImage {
                width,
                height,
                pixels,
                indexed_pixels: None,
                indexed_palette: None,
                subtype: subtype.to_string(),
                class_name,
            });
        }
    }
    Err(
        ImageError::DecompressError(
            format!(
                "{subtype} decryption marker or compressed payload is invalid for the active key"
            ),
        ),
    )
}
fn parse_rct_tc(
    data: &[u8],
    width: u32,
    height: u32,
    compressed_size: usize,
    subtype: &str,
) -> Result<MajiroImage, ImageError> {
    let is_tc01 = subtype == "TC01";
    let (compressed_data, class_name) = extract_rct_payload(
        data,
        compressed_size,
        is_tc01,
    )?;
    let pixel_count = (width as usize) * (height as usize);
    let bgr = majiro_decompress_rct(compressed_data, width as usize, pixel_count)?;
    let mut pixels = vec![0u8; pixel_count * 4];
    for (i, chunk) in bgr.chunks(3).enumerate() {
        if i >= pixel_count || chunk.len() < 3 {
            break;
        }
        let out_off = i * 4;
        pixels[out_off] = chunk[2];
        pixels[out_off + 1] = chunk[1];
        pixels[out_off + 2] = chunk[0];
        pixels[out_off + 3] = 0xFF;
    }
    Ok(MajiroImage {
        width,
        height,
        pixels,
        indexed_pixels: None,
        indexed_palette: None,
        subtype: subtype.to_string(),
        class_name,
    })
}
fn extract_rct_payload(
    data: &[u8],
    compressed_size: usize,
    has_class_name: bool,
) -> Result<(&[u8], Option<String>), ImageError> {
    let mut offset = 0x14;
    let mut class_name = None;
    if has_class_name {
        if offset + 2 > data.len() {
            return Err(ImageError::TooSmall);
        }
        let str_len = u16::from_le_bytes([data[offset], data[offset + 1]]) as usize;
        offset += 2;
        if offset + str_len > data.len() {
            return Err(ImageError::TooSmall);
        }
        let name_bytes = &data[offset..offset + str_len];
        let (cow, _, _) = encoding_rs::SHIFT_JIS.decode(name_bytes);
        class_name = Some(cow.into_owned().trim_end_matches('\0').to_string());
        offset += str_len;
    }
    if offset + compressed_size > data.len() {
        return Err(ImageError::TooSmall);
    }
    Ok((&data[offset..offset + compressed_size], class_name))
}
pub fn majiro_decompress_rct(
    compressed: &[u8],
    width: usize,
    pixel_count: usize,
) -> Result<Vec<u8>, ImageError> {
    if pixel_count == 0 || compressed.is_empty() {
        return Ok(Vec::new());
    }
    let output_size = pixel_count * 3;
    let mut output = vec![0u8; output_size];
    if compressed.len() < 3 {
        return Err(ImageError::DecompressError("input too small".into()));
    }
    output[0] = compressed[0];
    output[1] = compressed[1];
    output[2] = compressed[2];
    let mut out_pos: usize = 3;
    let mut in_pos: usize = 3;
    let w = width as isize;
    let offsets: [isize; 32] = [
        -3,
        -6,
        -9,
        -12,
        -15,
        -18,
        3 * (3 - w),
        3 * (2 - w),
        3 - 3 * w,
        -3 * w,
        3 * (-1 - w),
        3 * (-2 - w),
        3 * (-3 - w),
        9 - 6 * w,
        6 - 6 * w,
        3 - 6 * w,
        -6 * w,
        -3 - 6 * w,
        6 * (-1 - w),
        -9 - 6 * w,
        9 - 9 * w,
        6 - 9 * w,
        3 - 9 * w,
        -9 * w,
        -3 - 9 * w,
        -6 - 9 * w,
        9 * (-1 - w),
        6 - 12 * w,
        3 - 12 * w,
        -12 * w,
        -3 - 12 * w,
        -6 - 12 * w,
    ];
    while out_pos < output_size {
        if in_pos >= compressed.len() {
            return Err(ImageError::DecompressError("premature end of input".into()));
        }
        let cmd = compressed[in_pos];
        in_pos += 1;
        if cmd < 0x80 {
            let byte_len: usize = if cmd == 0x7F {
                if in_pos + 2 > compressed.len() {
                    return Err(
                        ImageError::DecompressError("extended literal EOF".into()),
                    );
                }
                let ext = u16::from_le_bytes([
                    compressed[in_pos],
                    compressed[in_pos + 1],
                ]) as usize;
                in_pos += 2;
                3 * (ext + 128)
            } else {
                3 * cmd as usize + 3
            };
            if in_pos + byte_len > compressed.len() {
                return Err(ImageError::DecompressError("literal run overflow".into()));
            }
            if out_pos + byte_len > output_size {
                return Err(
                    ImageError::DecompressError("literal run overflows output".into()),
                );
            }
            output[out_pos..out_pos + byte_len]
                .copy_from_slice(&compressed[in_pos..in_pos + byte_len]);
            out_pos += byte_len;
            in_pos += byte_len;
        } else {
            let offset_idx = ((cmd >> 2) & 0x1F) as usize;
            let len_code = cmd & 3;
            let byte_len: usize = if len_code == 3 {
                if in_pos + 2 > compressed.len() {
                    return Err(
                        ImageError::DecompressError("extended back-ref EOF".into()),
                    );
                }
                let ext = u16::from_le_bytes([
                    compressed[in_pos],
                    compressed[in_pos + 1],
                ]) as usize;
                in_pos += 2;
                3 * (ext + 4)
            } else {
                3 * len_code as usize + 3
            };
            let offset = offsets[offset_idx];
            let src_pos = out_pos as isize + offset;
            if src_pos < 0 {
                return Err(
                    ImageError::DecompressError(
                        format!(
                            "back-ref underflow: out_pos={}, idx={}, off={}", out_pos,
                            offset_idx, offset
                        ),
                    ),
                );
            }
            let src = src_pos as usize;
            if out_pos + byte_len > output_size {
                return Err(ImageError::DecompressError("back-ref overflow".into()));
            }
            for i in 0..byte_len {
                output[out_pos + i] = output[src + i];
            }
            out_pos += byte_len;
        }
    }
    Ok(output)
}

