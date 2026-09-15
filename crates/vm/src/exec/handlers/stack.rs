use super::helpers::sjis_mbsicmp;
use crate::exec::{StepResult, VmError};
use crate::value::Value;
impl crate::exec::Vm {
    pub(crate) fn exec_stack_binop(
        &mut self,
        opcode: u16,
    ) -> Result<StepResult, VmError> {
        use crate::value::{TAG_FLOAT, TAG_INT};
        let is_str_variant = matches!(
            opcode, 0x11A | 0x13A | 0x142 | 0x14A | 0x152 | 0x15A | 0x162
        );
        if is_str_variant {
            return self.exec_stack_binop_str(opcode);
        }
        let a = self.stack.pop().ok_or(VmError::StackUnderflow)?;
        if self.stack.is_empty() {
            return Err(VmError::StackUnderflow);
        }
        let to_f64 = |v: &Value| -> f64 {
            match v.type_tag {
                TAG_FLOAT => f32::from_bits(v.bits) as f64,
                _ => v.bits as i32 as f64,
            }
        };
        let b_idx = self.stack.len() - 1;
        let b_tag = self.stack.as_slice()[b_idx].type_tag;
        let b_bits = self.stack.as_slice()[b_idx].bits;
        let b_int = b_bits as i32;
        let a_int = a.bits as i32;
        let b_f = to_f64(&self.stack.as_slice()[b_idx]);
        let a_f = to_f64(&a);
        let (res_tag, res_bits): (u32, u32) = match opcode {
            0x100 => (TAG_INT, b_int.wrapping_mul(a_int) as u32),
            0x108 => {
                let r = if a_int == 0 {
                    b_int.wrapping_mul(10000)
                } else {
                    b_int / a_int
                };
                (TAG_INT, r as u32)
            }
            0x110 => {
                let r = if a_int == 0 { 0 } else { b_int % a_int };
                (TAG_INT, r as u32)
            }
            0x118 => (TAG_INT, b_int.wrapping_add(a_int) as u32),
            0x120 => (TAG_INT, b_int.wrapping_sub(a_int) as u32),
            0x128 => (TAG_INT, (b_int >> (a_int & 0x1F)) as u32),
            0x130 => (TAG_INT, (b_int << (a_int & 0x1F)) as u32),
            0x168 => (TAG_INT, (b_int ^ a_int) as u32),
            0x180 => (TAG_INT, (b_int & a_int) as u32),
            0x188 => (TAG_INT, (b_int | a_int) as u32),
            0x101 => (TAG_FLOAT, ((b_f * a_f) as f32).to_bits()),
            0x119 => (TAG_FLOAT, ((b_f + a_f) as f32).to_bits()),
            0x121 => (TAG_FLOAT, ((b_f - a_f) as f32).to_bits()),
            0x109 => {
                let av = if a_f == 0.0 { 0.00001 } else { a_f };
                (TAG_FLOAT, ((b_f / av) as f32).to_bits())
            }
            0x138 => (TAG_INT, (b_int <= a_int) as u32),
            0x140 => (TAG_INT, (b_int < a_int) as u32),
            0x148 => (TAG_INT, ((b_int < a_int) as i32 - 1) as u32),
            0x150 => (TAG_INT, (b_int > a_int) as u32),
            0x139 => (TAG_INT, (b_f <= a_f) as u32),
            0x141 => (TAG_INT, if b_f < a_f { (-1i32) as u32 } else { 0 }),
            0x149 => (TAG_INT, if b_f < a_f { 0 } else { (-1i32) as u32 }),
            0x151 => (TAG_INT, (b_f > a_f) as u32),
            0x159 => (TAG_INT, (b_f == a_f) as u32),
            0x161 => (TAG_INT, (b_f != a_f) as u32),
            0x158 => (TAG_INT, (a.bits == b_bits) as u32),
            0x160 => (TAG_INT, (a.bits != b_bits) as u32),
            0x15B..=0x15D => {
                let eq = a.bits == b_bits;
                self.stack.pop();
                self.stack.push(Value::int(eq as i32));
                return Ok(StepResult::Continue);
            }
            0x163..=0x165 => {
                let ne = a.bits != b_bits;
                self.stack.pop();
                self.stack.push(Value::int(ne as i32));
                return Ok(StepResult::Continue);
            }
            0x170 => (TAG_INT, ((b_bits != 0) && (a.bits != 0)) as u32),
            0x178 => (TAG_INT, ((b_bits != 0) || (a.bits != 0)) as u32),
            _ => {
                return Err(VmError::Unimplemented {
                    opcode_or_hash: opcode as u32,
                    detail: format!("stack binop opcode 0x{:X} not in C1 table", opcode),
                    ip: 0,
                });
            }
        };
        let b = &mut self.stack.peek_slot_mut(b_idx).expect("b slot exists");
        b.type_tag = res_tag;
        b.bits = res_bits;
        b.data = None;
        let _ = b_tag;
        Ok(StepResult::Continue)
    }
    pub(crate) fn exec_stack_binop_str(
        &mut self,
        opcode: u16,
    ) -> Result<StepResult, VmError> {
        let a = self.stack.pop().ok_or(VmError::StackUnderflow)?;
        let b = self.stack.pop().ok_or(VmError::StackUnderflow)?;
        let a_bytes = a.as_str_bytes().unwrap_or(&[]).to_vec();
        let b_bytes = b.as_str_bytes().unwrap_or(&[]).to_vec();
        if opcode == 0x162 {
            crate::text_trace!(
                "[VM] STRCMP_NE left={:?} right={:?}", String::from_utf8_lossy(&
                b_bytes), String::from_utf8_lossy(& a_bytes)
            );
        }
        match opcode {
            0x11A => {
                let cat = super::helpers::c_string_concat(&b_bytes, &a_bytes);
                self.stack.push(Value::string(cat));
            }
            _ => {
                let cmp = sjis_mbsicmp(&b_bytes, &a_bytes);
                let truthy = match opcode {
                    0x13A => cmp <= 0,
                    0x142 => cmp < 0,
                    0x14A => cmp >= 0,
                    0x152 => cmp > 0,
                    0x15A => cmp == 0,
                    0x162 => cmp != 0,
                    _ => unreachable!(),
                };
                self.stack.push(Value::int(truthy as i32));
            }
        }
        Ok(StepResult::Continue)
    }
    pub(crate) fn exec_stack_unop(
        &mut self,
        opcode: u16,
    ) -> Result<StepResult, VmError> {
        use crate::value::{TAG_FLOAT, TAG_INT};
        let top = self.stack.peek_mut().ok_or(VmError::StackUnderflow)?;
        let old_bits = top.bits;
        match opcode {
            0x190 => {
                top.type_tag = TAG_INT;
                top.bits = (old_bits == 0) as u32;
                top.data = None;
            }
            0x191 | 0x1A8 | 0x1A9 => {}
            0x198 => {
                top.type_tag = TAG_INT;
                top.bits = !(old_bits as i32) as u32;
                top.data = None;
            }
            0x1A0 => {
                top.type_tag = TAG_INT;
                top.bits = (-(old_bits as i32)) as u32;
                top.data = None;
            }
            0x1A1 => {
                let f = if top.type_tag == TAG_FLOAT {
                    f32::from_bits(old_bits)
                } else {
                    old_bits as i32 as f32
                };
                top.type_tag = TAG_FLOAT;
                top.bits = (-f).to_bits();
                top.data = None;
            }
            _ => {
                return Err(VmError::Unimplemented {
                    opcode_or_hash: opcode as u32,
                    detail: format!("unary opcode 0x{:X} not in C1 unary table", opcode),
                    ip: 0,
                });
            }
        }
        Ok(StepResult::Continue)
    }
}
