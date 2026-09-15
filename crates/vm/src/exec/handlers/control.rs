use crate::cursor::Cursor;
use crate::exec::{CallFrame, CtxPhase, StepResult, VmError};
use crate::value::Value;
impl crate::exec::Vm {
    pub(crate) fn resolve_entry_global(&self, hash: u32) -> Option<(usize, usize)> {
        let caller_script = self.frames.last().map(|frame| frame.script_idx);
        if let Some(idx) = caller_script {
            if let Some(off) = self
                .scripts[idx]
                .entries
                .iter()
                .find(|entry| entry.name_hash == hash)
                .map(|entry| entry.offset as usize)
            {
                return Some((idx, off));
            }
        }
        for (idx, script) in self.scripts.iter().enumerate() {
            if Some(idx) == caller_script {
                continue;
            }
            if let Some(off) = script
                .entries
                .iter()
                .find(|e| e.name_hash == hash)
                .map(|e| e.offset as usize)
            {
                return Some((idx, off));
            }
        }
        None
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn exec_call(
        &mut self,
        script_idx: usize,
        caller_return_ip: usize,
        target_offset: usize,
        caller_sp: usize,
        cleanup_mode: u32,
        param_count: i16,
    ) -> Result<(), VmError> {
        let code_ptr: *const [u8] = self.scripts[script_idx].code.as_ref();
        let code: &'static [u8] = unsafe { &*code_ptr };
        if target_offset > code.len() {
            return Err(
                VmError::Other(
                    format!(
                        "CALL target offset 0x{:X} beyond code len 0x{:X}",
                        target_offset, code.len()
                    ),
                ),
            );
        }
        let marker = Value::int(param_count as i32);
        self.stack.push(marker);
        let entry_sp = caller_sp + 1;
        let stack_base = entry_sp;
        self.frames
            .push(CallFrame {
                cursor: Cursor::new(code, target_offset),
                script_idx,
                entry_sp,
                stack_base,
                cleanup_mode,
                script_marker: 0,
                param_count,
                label: format!("call@0x{:06X}", target_offset),
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
        let _ = caller_return_ip;
        Ok(())
    }
    pub(crate) fn exec_ret(&mut self) -> Result<StepResult, VmError> {
        let callee = match self.frames.pop() {
            Some(f) => f,
            None => return Ok(StepResult::Exited),
        };
        let cleanup_mode = callee.cleanup_mode;
        if self.frames.is_empty() {
            if !self.is_root_ctx() || self.active_phase != CtxPhase::Initial {
                match self.active_phase {
                    CtxPhase::Initial | CtxPhase::TransitionRequested => {
                        self.frames.push(callee);
                        self.active_phase = CtxPhase::TransitionRequested;
                    }
                    CtxPhase::Cleanup => {
                        self.active_phase = CtxPhase::CleanupReturned;
                    }
                    CtxPhase::Finalizer => {
                        self.active_phase = CtxPhase::Dead;
                    }
                    CtxPhase::CleanupReturned | CtxPhase::Dead => {}
                }
                self.process_active_context_phase();
                return Ok(StepResult::Returned);
            }
            let marker_slot = callee.entry_sp;
            while self.stack.len() > marker_slot {
                self.stack.pop();
            }
            return Ok(StepResult::Returned);
        }
        let marker_slot = callee.entry_sp.saturating_sub(1);
        let caller_slot = marker_slot.saturating_sub(callee.param_count.max(0) as usize);
        if cleanup_mode == 0 {
            if self.stack.len() > callee.entry_sp {
                let return_value = self.stack.pop().expect("len > entry_sp");
                while self.stack.len() > caller_slot {
                    self.stack.pop();
                }
                self.stack.push(return_value);
            } else {
                while self.stack.len() > caller_slot {
                    self.stack.pop();
                }
                self.stack.push(Value::int(0));
            }
        } else {
            while self.stack.len() > caller_slot {
                self.stack.pop();
            }
        }
        Ok(StepResult::Continue)
    }
}

