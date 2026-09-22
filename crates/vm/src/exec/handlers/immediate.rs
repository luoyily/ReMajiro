use super::helpers::{c_string_concat, cur_f};
use crate::exec::{StepResult, VmError};
use crate::value::{Scope, Value};
impl crate::exec::Vm {
    pub(crate) fn exec_imm_arith(
        &mut self,
        opcode: u16,
        tag: u16,
        imm: u32,
        idx: usize,
        pop: bool,
    ) -> Result<StepResult, VmError> {
        use crate::value::{TAG_FLOAT, TAG_INT};
        let scope = Scope::from_tag(tag);
        let op_base = if opcode >= 0x210 { opcode - 0x60 } else { opcode };
        let rhs = self.stack.peek().cloned().unwrap_or_else(Value::null);
        let rhs_i = rhs.as_int().unwrap_or(rhs.bits as i32);
        let rhs_f = cur_f(&rhs);
        let cur = self.read_var(scope, imm, idx).unwrap_or_else(Value::null);
        let cur_i = cur.as_int().unwrap_or(cur.bits as i32);
        let cur_fv = cur_f(&cur);
        let result: Value = match op_base {
            0x1B0 => {
                let mut v = rhs.clone();
                if v.type_tag == TAG_FLOAT {
                    v.bits = (f32::from_bits(v.bits) as i32) as u32;
                    v.type_tag = TAG_INT;
                }
                v
            }
            0x1B1 => {
                let mut v = rhs.clone();
                if v.type_tag == TAG_INT {
                    v.bits = (v.bits as i32 as f32).to_bits();
                    v.type_tag = TAG_FLOAT;
                }
                v
            }
            0x1B2..=0x1B5 => rhs.clone(),
            0x1B8 => Value::int(cur_i.wrapping_mul(rhs_i)),
            0x1C0 => {
                Value::int(
                    if rhs_i == 0 { cur_i.wrapping_mul(10000) } else { cur_i / rhs_i },
                )
            }
            0x1C8 => Value::int(if rhs_i == 0 { 0 } else { cur_i % rhs_i }),
            0x1D0 => Value::int(cur_i.wrapping_add(rhs_i)),
            0x1D8 => Value::int(cur_i.wrapping_sub(rhs_i)),
            0x1E0 => Value::int(cur_i << (rhs_i & 0x1F)),
            0x1E8 => Value::int(cur_i >> (rhs_i & 0x1F)),
            0x1F0 => Value::int(cur_i & rhs_i),
            0x1F8 => Value::int(cur_i ^ rhs_i),
            0x200 => Value::int(cur_i | rhs_i),
            0x1B9 => Value::float((cur_fv * rhs_f) as f32),
            0x1C1 => {
                Value::float(
                    (cur_fv / if rhs_f == 0.0 { 0.00001 } else { rhs_f }) as f32,
                )
            }
            0x1D1 => Value::float((cur_fv + rhs_f) as f32),
            0x1D9 => Value::float((cur_fv - rhs_f) as f32),
            0x1D2 => {
                Value::string(
                    c_string_concat(
                        cur.as_str_bytes().unwrap_or(&[]),
                        rhs.as_str_bytes().unwrap_or(&[]),
                    ),
                )
            }
            _ => {
                return Err(VmError::Unimplemented {
                    opcode_or_hash: opcode as u32,
                    detail: format!("imm-arith opcode 0x{:X} not implemented", opcode),
                    ip: 0,
                });
            }
        };
        if scope == Scope::Stack {
            let abs = self.stack_scope_abs(idx);
            if crate::diag_log_enabled() && matches!(op_base, 0x1B0..= 0x1B5)
                && result.type_tag == crate::value::TAG_STRING
            {
                let bytes = result.as_str_bytes().unwrap_or(&[]);
                let preview = String::from_utf8_lossy(&bytes[..bytes.len().min(28)])
                    .into_owned();
                eprintln!(
                    "[STORE] op=0x{:03X} S:{:08X} i{} len={} val={:?}", op_base, imm, idx
                    as i16, bytes.len(), preview
                );
            }
            if let Some(slot) = self.stack.peek_slot_mut(abs) {
                *slot = result;
            }
        } else {
            if crate::diag_log_enabled() && matches!(op_base, 0x1B0..= 0x1B5)
                && result.type_tag == crate::value::TAG_STRING
            {
                let bytes = result.as_str_bytes().unwrap_or(&[]);
                let preview = String::from_utf8_lossy(&bytes[..bytes.len().min(28)])
                    .into_owned();
                eprintln!(
                    "[STORE] op=0x{:03X} {}:{:08X} len={} val={:?}", op_base, match scope
                    { Scope::Global => 'G', Scope::Local => 'L', Scope::Thread => 'T',
                    Scope::Stack => 'S', }, imm, bytes.len(), preview
                );
            }
            self.write_var(scope, imm, idx, result);
        }
        if pop {
            self.stack.pop();
        }
        Ok(StepResult::Continue)
    }
    pub(crate) fn read_var(&self, scope: Scope, key: u32, idx: usize) -> Option<Value> {
        if scope == Scope::Stack {
            let abs = self.stack_scope_abs(idx);
            return self.stack.slot(abs).cloned();
        }
        let slot = match scope {
            Scope::Global => self.global_keys.get(&key).copied(),
            Scope::Local => self.local_keys.get(&key).copied(),
            Scope::Thread => self.thread_keys.get(&key).copied(),
            Scope::Stack => unreachable!(),
        }?;
        match scope {
            Scope::Global => self.globals.get(slot).cloned(),
            Scope::Local => self.locals.get(slot).cloned(),
            Scope::Thread => self.threads.get(slot).cloned(),
            Scope::Stack => unreachable!(),
        }
    }
    pub(crate) fn write_var(&mut self, scope: Scope, key: u32, idx: usize, v: Value) {
        if scope == Scope::Stack {
            let abs = self.stack_scope_abs(idx);
            if abs < 0x10_0000 {
                while self.stack.len() <= abs {
                    self.stack.push(Value::null());
                }
            }
            if let Some(slot) = self.stack.peek_slot_mut(abs) {
                *slot = v;
            }
            return;
        }
        let slot = match scope {
            Scope::Global => {
                *self
                    .global_keys
                    .entry(key)
                    .or_insert_with(|| {
                        let slot = self.globals.len();
                        self.globals.push(Value::null());
                        slot
                    })
            }
            Scope::Local => {
                *self
                    .local_keys
                    .entry(key)
                    .or_insert_with(|| {
                        let slot = self.locals.len();
                        self.locals.push(Value::null());
                        slot
                    })
            }
            Scope::Thread => {
                *self
                    .thread_keys
                    .entry(key)
                    .or_insert_with(|| {
                        let slot = self.threads.len();
                        self.threads.push(Value::null());
                        slot
                    })
            }
            Scope::Stack => unreachable!(),
        };
        match scope {
            Scope::Global => self.globals[slot] = v,
            Scope::Local => self.locals[slot] = v,
            Scope::Thread => self.threads[slot] = v,
            Scope::Stack => unreachable!(),
        }
    }
}
