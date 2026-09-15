use super::helpers::{c_string_concat, write_array_cell_inner};
use crate::exec::{StepResult, VmError};
use crate::value::{Scope, Value, ValueData, TAG_STRING_ARRAY};
impl crate::exec::Vm {
    pub(crate) fn exec_ext_store(
        &mut self,
        opcode: u16,
        tag: u16,
        imm: u32,
        idx: usize,
    ) -> Result<StepResult, VmError> {
        use crate::value::TAG_FLOAT;
        let scope = Scope::from_tag(tag);
        let dims_code = (tag >> 11) as u8;
        let op_base = if opcode >= 0x2D0 { opcode - 0x60 } else { opcode };
        let pop = opcode >= 0x2D0;
        let linear = self.array_index_calc(scope, imm, idx, dims_code)?;
        if matches!(op_base, 0x272 | 0x292) {
            self.ensure_string_array(scope, imm, idx);
        }
        let top_bits = match self.stack.peek() {
            Some(v) => v.bits,
            None => return Err(VmError::StackUnderflow),
        };
        let top_int = top_bits as i32;
        let top_float = f32::from_bits(top_bits);
        let cur = self.read_array_cell(scope, imm, idx, linear);
        let cur_int = cur.as_int().unwrap_or(cur.bits as i32);
        let cur_float = if cur.type_tag == TAG_FLOAT {
            f32::from_bits(cur.bits)
        } else {
            cur.bits as i32 as f32
        };
        let new_val: Value = match op_base {
            0x270 | 0x271 => Value::int(top_int),
            0x272 => {
                let v = self.stack.peek().cloned().unwrap_or(Value::null());
                v
            }
            0x278 => {
                let r = cur_int.wrapping_mul(top_int);
                if let Some(t) = self.stack.peek_mut() {
                    t.bits = r as u32;
                }
                Value::int(r)
            }
            0x279 => Value::float((cur_float as f64 * top_int as f64) as f32),
            0x280 => {
                let r = if top_int == 0 {
                    cur_int.wrapping_mul(10000)
                } else {
                    cur_int / top_int
                };
                Value::int(r)
            }
            0x281 => {
                let r = if top_float == 0.0 {
                    cur_float * 10000.0
                } else {
                    cur_float / top_float
                };
                Value::float(r)
            }
            0x288 => {
                let r = if top_int == 0 { 0 } else { cur_int % top_int };
                Value::int(r)
            }
            0x290 => Value::int(cur_int.wrapping_add(top_int)),
            0x291 => Value::float(cur_float + top_float),
            0x292 => {
                Value::string(
                    c_string_concat(
                        cur.as_str_bytes().unwrap_or(&[]),
                        self.stack.peek().and_then(Value::as_str_bytes).unwrap_or(&[]),
                    ),
                )
            }
            0x298 => Value::int(cur_int.wrapping_sub(top_int)),
            0x299 => Value::float(cur_float - top_float),
            0x2A0 => Value::int(cur_int << (top_int & 0x1F)),
            0x2A8 => Value::int(cur_int >> (top_int & 0x1F)),
            0x2B0 => Value::int(cur_int & top_int),
            0x2B8 => Value::int(cur_int ^ top_int),
            0x2C0 => Value::int(cur_int | top_int),
            _ => {
                return Err(VmError::Unimplemented {
                    opcode_or_hash: opcode as u32,
                    detail: format!("ext_store opcode 0x{:X} not in C3 table", opcode),
                    ip: 0,
                });
            }
        };
        self.write_array_cell(scope, imm, idx, linear, new_val);
        if pop {
            self.stack.pop();
        }
        Ok(StepResult::Continue)
    }
    pub(crate) fn array_index_calc(
        &mut self,
        scope: Scope,
        key: u32,
        idx: usize,
        dims_code: u8,
    ) -> Result<usize, VmError> {
        if dims_code == 0 || dims_code > 3 {
            return Ok(0);
        }
        let dims: [u32; 3] = match self.read_var(scope, key, idx) {
            Some(v) => {
                match v.data.as_deref() {
                    Some(crate::value::ValueData::IntArray { dims, .. }) => *dims,
                    Some(crate::value::ValueData::StringArray { dims, .. }) => *dims,
                    _ => [1, 1, 1],
                }
            }
            None => [1, 1, 1],
        };
        let ndim = dims_code as usize;
        let mut popped = Vec::with_capacity(ndim);
        for _ in 0..ndim {
            let v = self.stack.pop().ok_or(VmError::StackUnderflow)?;
            popped.push(v.as_int().unwrap_or(v.bits as i32));
        }
        let wrap = |i: i32, d: u32| -> usize {
            let d = d.max(1) as i32;
            let mut r = i % d;
            if r < 0 {
                r += d;
            }
            r as usize
        };
        let linear = match ndim {
            1 => wrap(popped[0], dims[0]),
            2 => {
                wrap(popped[0], dims[0])
                    + wrap(popped[1], dims[1]) * dims[0].max(1) as usize
            }
            3 => {
                let d0 = dims[0].max(1) as usize;
                let d1 = dims[1].max(1) as usize;
                wrap(popped[0], dims[0])
                    + d0 * (wrap(popped[1], dims[1]) + wrap(popped[2], dims[2]) * d1)
            }
            _ => 0,
        };
        Ok(linear)
    }
    pub(crate) fn read_array_cell(
        &self,
        scope: Scope,
        key: u32,
        idx: usize,
        linear: usize,
    ) -> Value {
        let slot = match self.read_var(scope, key, idx) {
            Some(v) => v,
            None => return Value::null(),
        };
        match slot.data.as_deref() {
            Some(crate::value::ValueData::IntArray { cells, .. }) => {
                cells.get(linear).cloned().unwrap_or(Value::null())
            }
            Some(crate::value::ValueData::StringArray { cells, .. }) => {
                cells.get(linear).cloned().unwrap_or_else(|| Value::string(vec![0]))
            }
            _ => if linear == 0 { slot } else { Value::null() }
        }
    }
    pub(crate) fn ensure_string_array(&mut self, scope: Scope, key: u32, idx: usize) {
        let Some(slot) = self.read_var(scope, key, idx) else {
            return;
        };
        if matches!(slot.data.as_deref(), Some(ValueData::StringArray { .. })) {
            return;
        }
        let Some(ValueData::IntArray { dims, cells, .. }) = slot.data.as_deref() else {
            return;
        };
        let dims = *dims;
        let cells = cells
            .iter()
            .map(|cell| {
                cell.as_str_bytes()
                    .map(|bytes| Value::string(bytes.to_vec()))
                    .unwrap_or_else(|| Value::string(vec![0]))
            })
            .collect();
        eprintln!(
            "[SAVE] upgraded legacy type-3 string array scope={:?} key=0x{key:08X} dims={dims:?}; lost cells default to empty strings",
            scope
        );
        self.write_var(
            scope,
            key,
            idx,
            Value {
                scope: slot.scope,
                type_tag: TAG_STRING_ARRAY,
                bits: 0,
                data: Some(
                    std::rc::Rc::new(ValueData::StringArray {
                        dims,
                        cells,
                    }),
                ),
            },
        );
    }
    pub(crate) fn write_array_cell(
        &mut self,
        scope: Scope,
        key: u32,
        idx: usize,
        linear: usize,
        v: Value,
    ) {
        let cur = self.read_var(scope, key, idx).unwrap_or(Value::null());
        let is_array = matches!(
            cur.data.as_deref(), Some(ValueData::IntArray { .. } | ValueData::StringArray
            { .. })
        );
        if !is_array && linear == 0 {
            self.write_var(scope, key, idx, v);
            return;
        }
        if scope == Scope::Stack {
            let abs = self.stack_scope_abs(idx);
            if let Some(slot) = self.stack.peek_slot_mut(abs) {
                let needs_init = !matches!(
                    slot.data.as_deref(), Some(ValueData::IntArray { .. } |
                    ValueData::StringArray { .. })
                );
                if needs_init {
                    let prev = slot.clone();
                    *slot = Value::int_array(1, [linear.max(1) as u32 + 1, 0, 0]);
                    if linear == 0 {
                        write_array_cell_inner(&mut slot.data, linear, prev);
                    }
                }
                write_array_cell_inner(&mut slot.data, linear, v);
            }
            return;
        }
        let mut slot = cur;
        let needs_init = !matches!(
            slot.data.as_deref(), Some(ValueData::IntArray { .. } |
            ValueData::StringArray { .. })
        );
        if needs_init {
            let prev = slot.clone();
            slot = Value::int_array(1, [linear.max(1) as u32 + 1, 0, 0]);
            if linear == 0 {
                write_array_cell_inner(&mut slot.data, linear, prev);
            }
        }
        write_array_cell_inner(&mut slot.data, linear, v);
        self.write_var(scope, key, idx, slot);
    }
}
