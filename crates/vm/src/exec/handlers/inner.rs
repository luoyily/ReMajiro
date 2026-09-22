use crate::exec::{StepResult, VmError};
use crate::host::Host;
use crate::value::Value;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InnerOutcome {
    Normal,
    Reenter,
    SelfManaged,
    Waiting,
    YieldNoPresent,
}
impl crate::exec::Vm {
    pub(crate) fn dispatch_inner<H: Host>(
        &mut self,
        hash: u32,
        count: usize,
        has_retval: bool,
        host: &mut H,
        opcode_start_ip: usize,
    ) -> Result<StepResult, VmError> {
        let sp_before = self.stack.len();
        let base_sp = sp_before.saturating_sub(count);
        let outcome = self
            .run_inner_handler(hash, count, has_retval, host, opcode_start_ip)?;
        let step_result = match outcome {
            InnerOutcome::SelfManaged => return Ok(StepResult::Continue),
            InnerOutcome::Reenter => {
                if let Some(frame) = self.frames.last_mut() {
                    frame.cursor.ip = opcode_start_ip;
                }
                self.schedule_pending_inner_host_bridges()?;
                return Ok(StepResult::Continue);
            }
            InnerOutcome::Waiting => {
                if let Some(frame) = self.frames.last_mut() {
                    frame.cursor.ip = opcode_start_ip;
                }
                return Ok(StepResult::Waiting);
            }
            InnerOutcome::YieldNoPresent => {
                if let Some(frame) = self.frames.last_mut() {
                    frame.cursor.ip = opcode_start_ip;
                }
                return Ok(StepResult::YieldNoPresent);
            }
            InnerOutcome::Normal => StepResult::Continue,
        };
        let sp_after = self.stack.len();
        if has_retval {
            if sp_after > base_sp {
                self.stack.truncate(base_sp);
            }
        } else {
            if sp_after > base_sp {
                let ret = self.stack.pop().expect("sp_after > base_sp");
                self.stack.truncate(base_sp);
                self.stack.push(ret);
            }
        }
        self.schedule_pending_hotspot_callback()?;
        self.schedule_pending_inner_host_bridges()?;
        Ok(step_result)
    }
    pub(crate) fn defer_inner_host_bridge(
        &mut self,
        name_hash: u32,
        args: Vec<Value>,
        has_retval: bool,
    ) {
        self.pending_inner_host_bridges.push((name_hash, args, has_retval));
    }
    fn schedule_pending_inner_host_bridges(&mut self) -> Result<(), VmError> {
        let pending = std::mem::take(&mut self.pending_inner_host_bridges);
        for (name_hash, args, has_retval) in pending {
            self.invoke_host_func(name_hash, &args, has_retval)?;
        }
        Ok(())
    }
    fn schedule_pending_hotspot_callback(&mut self) -> Result<(), VmError> {
        let Some(event) = self.pending_hotspot_callback.take() else {
            return Ok(());
        };
        if event.callback_hash == 0 {
            return Ok(());
        }
        let Some((script_idx, target_offset)) = self
            .resolve_entry_global(event.callback_hash) else {
            eprintln!("[HOTSPOT] callback 0x{:08X} unresolved", event.callback_hash);
            return Ok(());
        };
        for &value in &event.args {
            self.stack.push(Value::int(value));
        }
        let caller_sp = self.stack.len();
        self.exec_call(
            script_idx,
            self.current_ip().unwrap_or(0),
            target_offset,
            caller_sp,
            1,
            event.args.len() as i16,
        )?;
        eprintln!(
            "[HOTSPOT] callback 0x{:08X} id={} → entry 0x{:X}", event.callback_hash,
            event.args.last().copied().unwrap_or(- 1), target_offset
        );
        Ok(())
    }
    fn run_inner_handler<H: Host>(
        &mut self,
        hash: u32,
        count: usize,
        has_retval: bool,
        host: &mut H,
        opcode_start_ip: usize,
    ) -> Result<InnerOutcome, VmError> {
        let sp = self.stack.len();
        let base = sp.saturating_sub(count);
        let args: Vec<Value> = (0..count)
            .map(|i| self.stack.slot(base + i).cloned().unwrap_or(Value::null()))
            .collect();
        macro_rules! dispatch_handler {
            ($call:expr) => {
                if let Some(result) = $call { return result; }
            };
        }
        dispatch_handler!(self.handle_inner_host(hash, count, has_retval, host));
        dispatch_handler!(self.handle_inner_scene(hash, count, has_retval, host));
        dispatch_handler!(self.handle_inner_ctx(hash, count, has_retval, host, & args));
        dispatch_handler!(
            self.handle_inner_render_at(hash, count, has_retval, host, & args,
            opcode_start_ip,)
        );
        dispatch_handler!(self.handle_inner_text(hash, count, has_retval, host, & args));
        dispatch_handler!(
            self.handle_inner_audio(hash, count, has_retval, host, & args)
        );
        dispatch_handler!(
            self.handle_inner_scene2(hash, count, has_retval, host, & args)
        );
        dispatch_handler!(self.handle_inner_save(hash, count, has_retval, host, & args));
        dispatch_handler!(
            self.handle_inner_input(hash, count, has_retval, host, & args)
        );
        dispatch_handler!(self.handle_text_first_line_localized(hash, host));
        dispatch_handler!(self.handle_inner_strmath(hash, count, has_retval, & args));
        dispatch_handler!(self.handle_inner_file(hash, count, has_retval, host, & args));
        dispatch_handler!(self.handle_inner_gameplay(hash, count, & args));
        if let Some(r) = self.handle_inner_init(hash, count, has_retval, host, &args) {
            return Ok(r);
        }
        let name = crate::opcode::lookup_inner_opcode(hash).unwrap_or("?");
        host.unimplemented_handler(hash, name, &args);
        self.stack.push(Value::int(0));
        Ok(InnerOutcome::Normal)
    }
}

