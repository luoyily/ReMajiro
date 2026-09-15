use crate::cursor::Cursor;
use crate::exec::{CallFrame, VmError};
use crate::host::Host;
use crate::value::Value;
use super::inner::InnerOutcome;
pub(super) const HASHES: &[u32] = &[0xF2F9DAA8, 0xAE497F1A, 0x9FB53FBE, 0x4EF903F3];
impl crate::exec::Vm {
    pub(super) fn handle_inner_ctx<H: Host>(
        &mut self,
        hash: u32,
        count: usize,
        _has_retval: bool,
        _host: &mut H,
        args: &[Value],
    ) -> Option<Result<InnerOutcome, VmError>> {
        match hash {
            0xF2F9DAA8 => Some(self.inner_ctx_create(count, args, false)),
            0xAE497F1A => Some(self.inner_ctx_create(count, args, true)),
            0x9FB53FBE => {
                let function_id = args
                    .get(count.saturating_sub(1))
                    .and_then(Value::as_int)
                    .unwrap_or(0) as u32;
                self.stack
                    .push(
                        Value::int(
                            self.find_ctx_by_function_id(function_id).is_some() as i32,
                        ),
                    );
                Some(Ok(InnerOutcome::Normal))
            }
            0x4EF903F3 => {
                let function_id = if count == 0 {
                    self.current_function_id()
                } else {
                    args.get(count - 1).and_then(Value::as_int).unwrap_or(0) as u32
                };
                if function_id != 0
                    && self.find_ctx_by_function_id(function_id).is_some()
                {
                    self.request_context_termination(function_id);
                }
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            _ => None,
        }
    }
    fn inner_ctx_create(
        &mut self,
        count: usize,
        args: &[Value],
        link_parent: bool,
    ) -> Result<InnerOutcome, VmError> {
        if count == 0 {
            return Err(VmError::StackUnderflow);
        }
        let entry_hash = args.get(count - 1).and_then(|v| v.as_int()).unwrap_or(0)
            as u32;
        let (script_idx, entry_offset) = self
            .resolve_entry_global(entry_hash)
            .ok_or_else(|| VmError::UnknownEntryHash {
                hash: entry_hash,
                from_script: self.frames.last().map(|f| f.script_idx).unwrap_or(0),
                ip: self.current_ip().unwrap_or(0),
            })?;
        let forwarded_count = count.saturating_sub(1);
        let forwarded_args: Vec<Value> = if forwarded_count > 0 {
            args[0..forwarded_count].to_vec()
        } else {
            Vec::new()
        };
        for _ in 0..count {
            self.stack.pop();
        }
        let parent_function_id = link_parent.then(|| self.current_function_id());
        let (new_func_id, new_ctx_idx) = self.create_ctx_auto(parent_function_id);
        eprintln!(
            "[CTX] create_ctx entry_hash=0x{:08X} → ctx_idx={} func_id=0x{:08X} script={} offset=0x{:X} forwarded={}",
            entry_hash, new_ctx_idx, new_func_id, script_idx, entry_offset,
            forwarded_count
        );
        let prev_ctx = self.current_ctx;
        self.switch_ctx(new_ctx_idx);
        for arg in &forwarded_args {
            self.stack.push(arg.clone());
        }
        let param_count = forwarded_count as i16;
        let marker = Value::int(param_count as i32);
        self.stack.push(marker);
        let entry_sp = self.stack.len();
        let code_ptr: *const [u8] = self.scripts[script_idx].code.as_ref();
        let code: &'static [u8] = unsafe { &*code_ptr };
        self.frames
            .push(CallFrame {
                cursor: Cursor::new(code, entry_offset),
                script_idx,
                entry_sp,
                stack_base: entry_sp,
                cleanup_mode: 0,
                script_marker: 0,
                param_count,
                label: format!("ctx_create@0x{:06X}", entry_offset),
                jump_target_a: None,
                jump_target_b: None,
                jump_target_c: None,
                frame_advance_target: None,
                transition_target: None,
                force_return: false,
                wait_deadline: crate::exec::WaitDeadline::None,
                wait_present_epoch: None,
                text_line_pending: false,
            });
        self.switch_ctx(prev_ctx);
        self.stack.push(Value::int(new_func_id as i32));
        Ok(InnerOutcome::SelfManaged)
    }
}

