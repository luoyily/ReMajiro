pub const CRC32_TABLE: [u32; 256] = generate_table();
const fn generate_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u32;
        let mut j = 0;
        while j < 8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB8_8320;
            } else {
                crc >>= 1;
            }
            j += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
}
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in data {
        let idx = ((crc as u8) ^ byte) as usize;
        crc = CRC32_TABLE[idx] ^ (crc >> 8);
    }
    !crc
}
pub const CRC64_TABLE: [u64; 256] = generate_crc64_table();
const fn generate_crc64_table() -> [u64; 256] {
    let poly = 0x85E1_C3D7_53D4_6D27;
    let mut table = [0u64; 256];
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u64;
        let mut j = 0;
        while j < 8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ poly;
            } else {
                crc >>= 1;
            }
            j += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
}
pub fn crc64(data: &[u8]) -> u64 {
    let mut crc: u64 = 0xFFFF_FFFF_FFFF_FFFF;
    for &byte in data {
        let idx = ((crc as u8) ^ byte) as usize;
        crc = CRC64_TABLE[idx] ^ (crc >> 8);
    }
    !crc
}
const CRC32_TABLE_BYTES: [u8; 1024] = table_to_bytes();
const fn table_to_bytes() -> [u8; 1024] {
    let mut out = [0u8; 1024];
    let mut i = 0;
    while i < 256 {
        let val = CRC32_TABLE[i].to_le_bytes();
        let off = i * 4;
        out[off] = val[0];
        out[off + 1] = val[1];
        out[off + 2] = val[2];
        out[off + 3] = val[3];
        i += 1;
    }
    out
}
pub fn x1_crypt(data: &mut [u8]) {
    for (i, byte) in data.iter_mut().enumerate() {
        *byte ^= CRC32_TABLE_BYTES[i % 1024];
    }
}

