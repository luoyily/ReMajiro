use super::helpers::values_compare;
use crate::exec::{StepResult, VmError};
use crate::value::{Scope, Value, ValueData};
use std::rc::Rc;
impl crate::exec::Vm {
    pub(crate) fn exec_sys_02(
        &mut self,
        count: usize,
        type_bytes: &[u8],
    ) -> Result<(), VmError> {
        let frame = match self.frames.last() {
            Some(f) => f,
            None => return Ok(()),
        };
        let check_idx = frame.stack_base.saturating_sub(1);
        let have_enough = if let Some(check_val) = self.stack.slot(check_idx) {
            let data = check_val.as_int().unwrap_or(check_val.bits as i32);
            (count as i32) <= data
        } else {
            false
        };
        if have_enough {
            for i in 0..count {
                let slot_idx = frame.stack_base.saturating_sub(2 + i);
                let expected_tag = type_bytes.get(i).copied().unwrap_or(0);
                if let Some(val) = self.stack.slot(slot_idx) {
                    if val.type_tag != expected_tag as u32 {
                        eprintln!(
                            "[SYS_02] type mismatch at Stack[-{}]: expected tag {}, got {}",
                            i + 2, expected_tag, val.type_tag
                        );
                    }
                }
            }
        } else {
            for i in 0..count {
                let tag = type_bytes.get(count - 1 - i).copied().unwrap_or(0);
                let default = default_value_for_tag(tag);
                self.stack.push(default);
            }
            self.stack.push(Value::int(0));
            if let Some(frame) = self.frames.last_mut() {
                frame.stack_base = self.stack.len();
            }
            eprintln!(
                "[SYS_02] error recovery: pushed {} defaults, stack_base → {}", count +
                1, self.frames.last().map(| f | f.stack_base).unwrap_or(0)
            );
        }
        Ok(())
    }
    pub(crate) fn exec_sys_array_atomic(
        &mut self,
        tag: u16,
        imm: u32,
        idx: usize,
    ) -> Result<StepResult, VmError> {
        let scope = Scope::from_tag(tag);
        let dims_code = (tag >> 11) as u8;
        let sub = (((tag >> 8) & 7) + 3) as u8;
        let op = (tag & 7) as u8;
        let post = ((tag >> 3) & 3) as u8;
        let linear = self.array_index_calc(scope, imm, idx, dims_code)?;
        if sub == 5 {
            self.ensure_string_array(scope, imm, idx);
        }
        let cur = self.read_array_cell(scope, imm, idx, linear);
        let cur_int = cur.bits as i32;
        let cur_float = f32::from_bits(cur.bits);
        if sub == 5 {
            self.stack
                .push(Value {
                    scope: Scope::Stack,
                    type_tag: crate::value::TAG_STRING,
                    bits: cur.bits,
                    data: cur.data.clone(),
                });
            return Ok(StepResult::Continue);
        }
        let (new_cell, result): (Value, Value) = match sub {
            3 => {
                let (nc, r) = match op {
                    0 => (cur_int, cur_int),
                    1 => {
                        let n = cur_int.wrapping_add(1);
                        (n, n)
                    }
                    2 => {
                        let n = cur_int.wrapping_sub(1);
                        (n, n)
                    }
                    3 => (cur_int.wrapping_add(1), cur_int),
                    4 => (cur_int.wrapping_sub(1), cur_int),
                    _ => (cur_int, cur_int),
                };
                (Value::int(nc), Value::int(r))
            }
            4 => {
                let (nc, r) = match op {
                    0 => (cur_float, cur_float),
                    1 => {
                        let n = cur_float + 1.0;
                        (n, n)
                    }
                    2 => {
                        let n = cur_float - 1.0;
                        (n, n)
                    }
                    3 => (cur_float + 1.0, cur_float),
                    4 => (cur_float - 1.0, cur_float),
                    _ => (cur_float, cur_float),
                };
                (Value::float(nc), Value::float(r))
            }
            _ => (Value::int(cur_int), Value::int(cur_int)),
        };
        let result = match sub {
            3 => {
                match post {
                    1 => Value::int(-(result.bits as i32)),
                    2 => Value::int((result.bits == 0) as i32),
                    3 => Value::int(!(result.bits as i32)),
                    _ => result,
                }
            }
            4 => {
                match post {
                    1 => Value::float(-f32::from_bits(result.bits)),
                    _ => result,
                }
            }
            _ => result,
        };
        self.write_array_cell(scope, imm, idx, linear, new_cell);
        self.stack.push(result);
        Ok(StepResult::Continue)
    }
    pub(crate) fn exec_sys_cmp_sysvar(
        &mut self,
        opcode: u16,
        off: i32,
        from_ip: usize,
    ) -> Result<StepResult, VmError> {
        let sysvar = self
            .read_var(Scope::Thread, crate::exec::MARK_SYSVAR_HASH, 0)
            .unwrap_or_else(Value::null);
        let top = match self.stack.peek() {
            Some(v) => v.clone(),
            None => return Err(VmError::StackUnderflow),
        };
        let cond = match opcode {
            0x838 => values_compare(&top, &sysvar) <= 0,
            0x839 => values_compare(&top, &sysvar) >= 0,
            _ => unreachable!(),
        };
        if !cond {
            let frame = self.frames.last_mut().expect("frame exists");
            let target = (frame.cursor.ip as isize + off as isize) as usize;
            let len = frame.cursor.code.len();
            if target > len {
                return Err(VmError::InvalidJump {
                    from: from_ip,
                    target,
                    len,
                });
            }
            frame.cursor.ip = target;
        }
        Ok(StepResult::Continue)
    }
    pub(crate) fn exec_push_op(
        &mut self,
        tag: u16,
        imm: u32,
        idx: usize,
    ) -> Result<StepResult, VmError> {
        let scope = Scope::from_tag(tag);
        let sub = (tag >> 8) & 7;
        let op = (tag & 7) as u8;
        let post = ((tag >> 3) & 3) as u8;
        let cur = self.read_var(scope, imm, idx).unwrap_or(Value::null());
        let cur_int = cur.bits as i32;
        let cur_float = f32::from_bits(cur.bits);
        if sub == 2 || sub >= 3 {
            let data = cur.data.clone();
            let result = Value {
                scope,
                type_tag: sub as u32,
                bits: imm,
                data,
            };
            self.stack.push(result);
            return Ok(StepResult::Continue);
        }
        let (new_cell, result): (Value, Value) = match sub {
            0 => {
                let (nc, r) = match op {
                    0 => (cur_int, cur_int),
                    1 => {
                        let n = cur_int.wrapping_add(1);
                        (n, n)
                    }
                    2 => {
                        let n = cur_int.wrapping_sub(1);
                        (n, n)
                    }
                    3 => (cur_int.wrapping_add(1), cur_int),
                    4 => (cur_int.wrapping_sub(1), cur_int),
                    _ => (cur_int, cur_int),
                };
                (Value::int(nc), Value::int(r))
            }
            1 => {
                let (nc, r) = match op {
                    0 => (cur_float, cur_float),
                    1 => {
                        let n = cur_float + 1.0;
                        (n, n)
                    }
                    2 => {
                        let n = cur_float - 1.0;
                        (n, n)
                    }
                    3 => (cur_float + 1.0, cur_float),
                    4 => (cur_float - 1.0, cur_float),
                    _ => (cur_float, cur_float),
                };
                (Value::float(nc), Value::float(r))
            }
            _ => unreachable!("sub>=2 handled above"),
        };
        let result = match sub {
            0 => {
                match post {
                    1 => Value::int(-(result.bits as i32)),
                    2 => Value::int((result.bits == 0) as i32),
                    3 => Value::int(!(result.bits as i32)),
                    _ => result,
                }
            }
            1 => {
                match post {
                    1 => Value::float(-f32::from_bits(result.bits)),
                    _ => result,
                }
            }
            _ => result,
        };
        self.write_var(scope, imm, idx, new_cell);
        self.stack.push(result);
        Ok(StepResult::Continue)
    }
    pub(crate) fn exec_push_bytes(
        &mut self,
        bytes: &[u8],
    ) -> Result<StepResult, VmError> {
        for &b in bytes {
            let v = default_value_for_tag(b);
            self.stack.push(v);
        }
        Ok(StepResult::Continue)
    }
}
fn default_value_for_tag(tag: u8) -> Value {
    match tag {
        0 => Value::int(0),
        1 => Value::float(0.0f32),
        2 => {
            Value {
                scope: Scope::Stack,
                type_tag: 2,
                bits: 0,
                data: Some(Rc::new(ValueData::Str(vec![0]))),
            }
        }
        3 | 4 => {
            Value {
                scope: Scope::Stack,
                type_tag: tag as u32,
                bits: 0,
                data: Some(
                    Rc::new(ValueData::IntArray {
                        ndim: 1,
                        dims: [1, 0, 0],
                        cells: Vec::new(),
                    }),
                ),
            }
        }
        5 => Value::string_array_empty(),
        _ => Value::int(0),
    }
}
