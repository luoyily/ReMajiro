use crate::exec::{HostFuncReg, VmError};
use crate::host::Host;
use crate::value::Value;
use super::inner::InnerOutcome;
pub(super) const HASHES: &[u32] = &[
    0x078A756E, 0x1295BBDA, 0xA93C9856, 0x76EE6C90, 0x29F1AC40,
];
impl crate::exec::Vm {
    pub(super) fn handle_inner_host<H: Host>(
        &mut self,
        hash: u32,
        count: usize,
        has_retval: bool,
        host: &mut H,
    ) -> Option<Result<InnerOutcome, VmError>> {
        match hash {
            0x078A756E => {
                let name_val = self.stack.peek().cloned().unwrap_or(Value::null());
                let cb_hash = self
                    .stack
                    .peek_from_top(1)
                    .map(|v| v.bits as i32 as u32)
                    .unwrap_or(0);
                let name_str = name_val.as_str_bytes().unwrap_or(&[]).to_vec();
                eprintln!(
                    "[HOST] register_host_func name={:?} callback_hash=0x{:08X}",
                    String::from_utf8_lossy(& name_str), cb_hash
                );
                self.host_func_register(name_str, cb_hash);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x1295BBDA => {
                let name_val = self.stack.peek().cloned().unwrap_or(Value::null());
                let cb_hash = self
                    .stack
                    .peek_from_top(1)
                    .map(|v| v.bits as i32 as u32)
                    .unwrap_or(0);
                let name_str = name_val.as_str_bytes().unwrap_or(&[]).to_vec();
                eprintln!(
                    "[HOST] register_host_func_head name={:?} callback_hash=0x{:08X}",
                    String::from_utf8_lossy(& name_str), cb_hash
                );
                self.host_func_register_head(name_str, cb_hash);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xA93C9856 => {
                let name_val = self.stack.peek().cloned().unwrap_or(Value::null());
                let callback_hash = self
                    .stack
                    .peek_from_top(1)
                    .map(|v| v.bits as i32 as u32)
                    .unwrap_or(0);
                let name_str = name_val.as_str_bytes().unwrap_or(&[]).to_vec();
                let crc = crate::formats_crc32(&name_str);
                let removed = self.host_func_remove(crc, callback_hash);
                eprintln!(
                    "[HOST] remove name={:?} crc=0x{:08X} callback=0x{:08X} removed={}",
                    String::from_utf8_lossy(& name_str), crc, callback_hash, removed
                );
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x76EE6C90 => Some(self.inner_call_by_name(count, has_retval, host)),
            0x29F1AC40 => Some(self.inner_traverse_host_func(count, has_retval)),
            _ => None,
        }
    }
    fn inner_call_by_name<H: Host>(
        &mut self,
        count: usize,
        has_retval: bool,
        _host: &mut H,
    ) -> Result<InnerOutcome, VmError> {
        let name_val = match self.stack.pop() {
            Some(v) => v,
            None => return Err(VmError::StackUnderflow),
        };
        let name_bytes = name_val.as_str_bytes().unwrap_or(&[]).to_vec();
        let crc = crate::formats_crc32(&name_bytes);
        let remaining = count.saturating_sub(1);
        eprintln!(
            "[HOST] call_by_name name={:?} crc=0x{:08X} forwarded_args={}",
            String::from_utf8_lossy(& name_bytes), crc, remaining
        );
        let forwarded_args: Vec<Value> = if remaining > 0 {
            let base = self.stack.len().saturating_sub(remaining);
            (0..remaining)
                .map(|i| self.stack.slot(base + i).cloned().unwrap_or(Value::null()))
                .collect()
        } else {
            Vec::new()
        };
        for _ in 0..remaining {
            self.stack.pop();
        }
        let scheduled = self.invoke_host_func(crc, &forwarded_args, has_retval)?;
        if scheduled == 0 {
            eprintln!(
                "[HOST] call_by_name {:?} → not registered", String::from_utf8_lossy(&
                name_bytes)
            );
        } else {
            eprintln!(
                "[HOST] call_by_name {:?} → scheduled {} callback(s)",
                String::from_utf8_lossy(& name_bytes), scheduled
            );
        }
        Ok(InnerOutcome::SelfManaged)
    }
    fn inner_traverse_host_func(
        &mut self,
        count: usize,
        has_retval: bool,
    ) -> Result<InnerOutcome, VmError> {
        let ctx_id = self
            .stack
            .pop()
            .ok_or(VmError::StackUnderflow)?
            .as_int()
            .unwrap_or(-1);
        let name = self
            .stack
            .pop()
            .ok_or(VmError::StackUnderflow)?
            .as_str_bytes()
            .unwrap_or(&[])
            .to_vec();
        let forwarded_count = count.saturating_sub(2);
        let forwarded_base = self.stack.len().saturating_sub(forwarded_count);
        let forwarded: Vec<Value> = (0..forwarded_count)
            .map(|i| {
                self.stack.slot(forwarded_base + i).cloned().unwrap_or(Value::null())
            })
            .collect();
        for _ in 0..forwarded_count {
            self.stack.pop();
        }
        if has_retval {
            self.stack.push(Value::int(0));
        }
        let name_hash = crate::formats_crc32(&name);
        let target_function_id = if ctx_id == -1 {
            self.current_function_id()
        } else {
            ctx_id as u32
        };
        let regs = self.host_funcs.get(&name_hash).cloned().unwrap_or_default();
        let Some(target_ctx) = self.find_ctx_by_function_id(target_function_id) else {
            eprintln!(
                "[HOST] traverse {:?} -> ctx 0x{:08X} missing", String::from_utf8_lossy(&
                name), target_function_id
            );
            return Ok(InnerOutcome::SelfManaged);
        };
        let original_ctx = self.current_ctx;
        self.switch_ctx(target_ctx);
        let mut scheduled = 0usize;
        for reg in regs.into_iter().rev() {
            let Some((script_idx, target_offset)) = reg
                .callback_target
                .or_else(|| self.resolve_entry_global(reg.callback_hash)) else {
                continue;
            };
            for arg in &forwarded {
                self.stack.push(arg.clone());
            }
            let caller_sp = self.stack.len();
            self.exec_call(
                script_idx,
                0,
                target_offset,
                caller_sp,
                if has_retval { 1 } else { 0 },
                forwarded_count as i16,
            )?;
            scheduled += 1;
        }
        self.switch_ctx(original_ctx);
        eprintln!(
            "[HOST] traverse {:?} -> ctx 0x{:08X}, scheduled {} callbacks",
            String::from_utf8_lossy(& name), target_function_id, scheduled
        );
        Ok(InnerOutcome::SelfManaged)
    }
    pub(crate) fn host_func_register(&mut self, name: Vec<u8>, callback_hash: u32) {
        let crc = crate::formats_crc32(&name);
        let ctx = self.current_function_id();
        let callback_target = self.resolve_entry_global(callback_hash);
        self.host_funcs
            .entry(crc)
            .or_default()
            .push(HostFuncReg {
                ctx,
                callback_hash,
                callback_target,
                pinned: false,
            });
    }
    pub(crate) fn host_func_register_head(&mut self, name: Vec<u8>, callback_hash: u32) {
        let crc = crate::formats_crc32(&name);
        let ctx = self.current_function_id();
        let callback_target = self.resolve_entry_global(callback_hash);
        self.host_funcs
            .entry(crc)
            .or_default()
            .insert(
                0,
                HostFuncReg {
                    ctx,
                    callback_hash,
                    callback_target,
                    pinned: false,
                },
            );
    }
    pub fn pin_host_funcs(&mut self) {
        for regs in self.host_funcs.values_mut() {
            for reg in regs {
                reg.pinned = true;
            }
        }
    }
    pub(crate) fn cleanup_unpinned_host_funcs(&mut self) -> usize {
        let before = self.host_funcs.values().map(Vec::len).sum::<usize>();
        self.host_funcs
            .retain(|_, regs| {
                regs.retain(|reg| reg.pinned);
                !regs.is_empty()
            });
        before - self.host_funcs.values().map(Vec::len).sum::<usize>()
    }
    pub(crate) fn host_func_remove(
        &mut self,
        name_hash: u32,
        callback_hash: u32,
    ) -> usize {
        let current_ctx = self.current_function_id();
        let callback_target = self.resolve_entry_global(callback_hash);
        let Some(regs) = self.host_funcs.get_mut(&name_hash) else {
            return 0;
        };
        let before = regs.len();
        regs.retain(|reg| {
            let same_callback = match (reg.callback_target, callback_target) {
                (Some(registered), Some(requested)) => registered == requested,
                _ => reg.callback_hash == callback_hash,
            };
            !(reg.ctx == current_ctx && same_callback && !reg.pinned)
        });
        let removed = before - regs.len();
        if regs.is_empty() {
            self.host_funcs.remove(&name_hash);
        }
        removed
    }
    pub fn invoke_host_func(
        &mut self,
        name_hash: u32,
        args: &[Value],
        has_retval: bool,
    ) -> Result<usize, VmError> {
        let regs = self.host_funcs.get(&name_hash).cloned().unwrap_or_default();
        let original_ctx = self.current_ctx;
        let mut scheduled = 0usize;
        if matches!(
            name_hash, 0xF7A4_C8D8 | 0x47BB_540C | 0x20CE_505D | 0x93B3_8A0B |
            0x7EE6_7053 | 0x0D9B_7F0E | 0x44A4_FF72 | crate ::exec::X_CONTROL_NAME_HASH
        ) {
            crate::text_trace!(
                "[VM] HOST_BRIDGE_BEGIN hash=0x{name_hash:08X} regs={} ctx=0x{:08X} depth={} args={}",
                regs.len(), self.active_function_id, self.frames.len(), args.len()
            );
        }
        if has_retval {
            self.stack.push(Value::int(0));
        }
        for reg in regs {
            let Some((script_idx, target_offset)) = reg
                .callback_target
                .or_else(|| self.resolve_entry_global(reg.callback_hash)) else {
                eprintln!(
                    "[HOST] bridge hash=0x{:08X}: callback 0x{:08X} unresolved",
                    name_hash, reg.callback_hash
                );
                continue;
            };
            let Some(ctx_idx) = self.find_ctx_by_function_id(reg.ctx) else {
                eprintln!(
                    "[HOST] bridge hash=0x{:08X}: ctx func_id=0x{:08X} missing",
                    name_hash, reg.ctx
                );
                continue;
            };
            self.switch_ctx(ctx_idx);
            for arg in args {
                self.stack.push(arg.clone());
            }
            let caller_sp = self.stack.len();
            let call_result = self
                .exec_call(
                    script_idx,
                    0,
                    target_offset,
                    caller_sp,
                    1,
                    args.len() as i16,
                );
            self.switch_ctx(original_ctx);
            call_result?;
            scheduled += 1;
            if matches!(
                name_hash, 0xF7A4_C8D8 | 0x47BB_540C | 0x20CE_505D | 0x93B3_8A0B |
                0x7EE6_7053 | 0x0D9B_7F0E | 0x44A4_FF72 | crate
                ::exec::X_CONTROL_NAME_HASH
            ) {
                crate::text_trace!(
                    "[VM] HOST_BRIDGE_PUSH hash=0x{name_hash:08X} ctx=0x{:08X} callback=0x{:08X} script={} offset=0x{target_offset:X}",
                    reg.ctx, reg.callback_hash, script_idx
                );
            }
            eprintln!(
                "[HOST] bridge hash=0x{:08X} → ctx {} func_id=0x{:08X} callback=0x{:08X} offset=0x{:X}",
                name_hash, ctx_idx, reg.ctx, reg.callback_hash, target_offset
            );
        }
        Ok(scheduled)
    }
}

