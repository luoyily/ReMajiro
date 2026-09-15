use crate::exec::VmError;
pub struct Cursor<'a> {
    pub code: &'a [u8],
    pub ip: usize,
}
impl<'a> Cursor<'a> {
    pub fn new(code: &'a [u8], ip: usize) -> Self {
        Self { code, ip }
    }
    pub fn remaining(&self) -> usize {
        self.code.len().saturating_sub(self.ip)
    }
    #[inline]
    pub fn read_u16(&mut self) -> Result<u16, VmError> {
        if self.ip + 2 > self.code.len() {
            return Err(VmError::CodeOverread {
                ip: self.ip,
                need: 2,
                have: self.remaining(),
            });
        }
        let v = u16::from_le_bytes([self.code[self.ip], self.code[self.ip + 1]]);
        self.ip += 2;
        Ok(v)
    }
    #[inline]
    pub fn read_u32(&mut self) -> Result<u32, VmError> {
        if self.ip + 4 > self.code.len() {
            return Err(VmError::CodeOverread {
                ip: self.ip,
                need: 4,
                have: self.remaining(),
            });
        }
        let v = u32::from_le_bytes([
            self.code[self.ip],
            self.code[self.ip + 1],
            self.code[self.ip + 2],
            self.code[self.ip + 3],
        ]);
        self.ip += 4;
        Ok(v)
    }
    #[inline]
    pub fn read_i32(&mut self) -> Result<i32, VmError> {
        Ok(self.read_u32()? as i32)
    }
    #[inline]
    pub fn read_i16(&mut self) -> Result<i16, VmError> {
        Ok(self.read_u16()? as i16)
    }
    #[inline]
    pub fn read_f32(&mut self) -> Result<f32, VmError> {
        Ok(f32::from_bits(self.read_u32()?))
    }
    pub fn read_bytes(&mut self, n: usize) -> Result<&'a [u8], VmError> {
        if self.ip + n > self.code.len() {
            return Err(VmError::CodeOverread {
                ip: self.ip,
                need: n,
                have: self.remaining(),
            });
        }
        let slice = &self.code[self.ip..self.ip + n];
        self.ip += n;
        Ok(slice)
    }
    pub fn peek_u16(&self) -> Result<u16, VmError> {
        if self.ip + 2 > self.code.len() {
            return Err(VmError::CodeOverread {
                ip: self.ip,
                need: 2,
                have: self.remaining(),
            });
        }
        Ok(u16::from_le_bytes([self.code[self.ip], self.code[self.ip + 1]]))
    }
    pub fn read_i32_at(&self, pos: usize) -> Result<i32, VmError> {
        if pos + 4 > self.code.len() {
            return Err(VmError::CodeOverread {
                ip: pos,
                need: 4,
                have: self.code.len() - pos.min(self.code.len()),
            });
        }
        Ok(
            i32::from_le_bytes([
                self.code[pos],
                self.code[pos + 1],
                self.code[pos + 2],
                self.code[pos + 3],
            ]),
        )
    }
}

