use crate::exec::{CtxState, StepResult, Vm, VmError};
use crate::host::Host;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedSignal {
    Waiting,
    WaitingNoPresent,
    LoadRequested,
    QuitRequested,
    BudgetExhausted,
    TimeSliced,
    RootExited,
    Terminated,
}
impl Vm {
    pub fn set_schedule_wall_slice(&mut self, slice: Option<std::time::Duration>) {
        self.schedule_wall_slice = slice;
    }
    pub fn schedule<H: Host>(
        &mut self,
        host: &mut H,
        budget: usize,
    ) -> Result<SchedSignal, VmError> {
        if self.pending_load.is_some() {
            return Ok(SchedSignal::LoadRequested);
        }
        if self.quit_requested {
            return Ok(SchedSignal::QuitRequested);
        }
        if !self.scheduler_no_present {
            host.begin_scheduler_tick();
        }
        self.wake_waiting_contexts(host.get_timestamp());
        self.process_pending_context_phases();
        self.propagate_parent_transitions();
        let snapshot_len = self.suspended_ctxs.len();
        let mut snapshot_cursor = snapshot_len;
        let mut active_slice = None;
        let slice = self.schedule_wall_slice;
        let pass_started = slice.map(|_| web_time::Instant::now());
        for step in 0..budget {
            if step & 0x3FF == 0x3FF {
                if let (Some(deadline), Some(start)) = (slice, pass_started) {
                    if start.elapsed() >= deadline {
                        return Ok(SchedSignal::TimeSliced);
                    }
                }
            }
            if active_slice.is_some_and(|idx| !self.is_ctx_runnable(idx)) {
                active_slice = None;
            }
            if active_slice.is_none() {
                while snapshot_cursor > 0 {
                    snapshot_cursor -= 1;
                    let candidate = snapshot_cursor;
                    if candidate != self.current_ctx
                        && self.suspended_ctxs[candidate].is_none()
                    {
                        continue;
                    }
                    if self.is_ctx_runnable(candidate) {
                        active_slice = Some(candidate);
                        break;
                    }
                }
            }
            let Some(ctx_idx) = active_slice else {
                return Ok(self.scheduler_turn_complete_signal());
            };
            if ctx_idx != self.current_ctx {
                self.switch_ctx(ctx_idx);
            }
            if self.active_state == CtxState::Idle {
                self.active_state = CtxState::Running;
            }
            match self.step(host)? {
                StepResult::Continue => {}
                StepResult::Waiting => {
                    self.active_state = CtxState::Waiting;
                    active_slice = None;
                }
                StepResult::YieldNoPresent => {
                    self.active_state = CtxState::Waiting;
                    active_slice = None;
                }
                StepResult::Returned => {
                    if self.frames.is_empty() {
                        if self.is_root_ctx() && self.active_state != CtxState::Inactive
                        {
                            return Ok(SchedSignal::RootExited);
                        } else if self.active_state != CtxState::Inactive {
                            self.mark_ctx_finished();
                            active_slice = None;
                        }
                    }
                }
                StepResult::Exited => {
                    if self.is_root_ctx() && self.active_state != CtxState::Inactive {
                        return Ok(SchedSignal::RootExited);
                    } else if self.active_state != CtxState::Inactive {
                        self.mark_ctx_finished();
                        active_slice = None;
                    }
                }
            }
            if self.pending_load.is_some() {
                return Ok(SchedSignal::LoadRequested);
            }
            if self.quit_requested {
                return Ok(SchedSignal::QuitRequested);
            }
        }
        Ok(SchedSignal::BudgetExhausted)
    }
    fn is_ctx_runnable(&self, idx: usize) -> bool {
        if idx == self.current_ctx {
            !self.frames.is_empty()
                && matches!(self.active_state, CtxState::Idle | CtxState::Running)
        } else {
            self.suspended_ctxs
                .get(idx)
                .and_then(|c| c.as_ref())
                .map(|c| {
                    !c.frames.is_empty()
                        && matches!(c.state, CtxState::Idle | CtxState::Running)
                })
                .unwrap_or(false)
        }
    }
    fn wake_waiting_contexts(&mut self, now_ms: i32) {
        fn deadline_is_still_pending(deadline: i32, now_ms: i32) -> bool {
            now_ms < deadline
                && now_ms.wrapping_sub(deadline).wrapping_abs() <= 0x0293_2E00
        }
        fn should_wake(frames: &[crate::exec::CallFrame], now_ms: i32) -> bool {
            match frames.last().map(|frame| frame.wait_deadline) {
                Some(crate::exec::WaitDeadline::At(deadline)) => {
                    !deadline_is_still_pending(deadline, now_ms)
                }
                Some(crate::exec::WaitDeadline::Never) => false,
                Some(crate::exec::WaitDeadline::None) | None => true,
            }
        }
        if self.active_state == CtxState::Waiting && should_wake(&self.frames, now_ms) {
            self.active_state = CtxState::Running;
        }
        for ctx in self.suspended_ctxs.iter_mut().flatten() {
            if ctx.state == CtxState::Waiting && should_wake(&ctx.frames, now_ms) {
                ctx.state = CtxState::Running;
            }
        }
    }
    fn has_waiting_contexts(&self) -> bool {
        self.active_state == CtxState::Waiting
            || self
                .suspended_ctxs
                .iter()
                .flatten()
                .any(|ctx| ctx.state == CtxState::Waiting)
    }
    fn has_runnable_contexts(&self) -> bool {
        (0..self.suspended_ctxs.len()).any(|idx| self.is_ctx_runnable(idx))
    }
    fn scheduler_turn_complete_signal(&self) -> SchedSignal {
        if self.has_waiting_contexts() || self.has_runnable_contexts() {
            if self.scheduler_no_present {
                SchedSignal::WaitingNoPresent
            } else {
                SchedSignal::Waiting
            }
        } else {
            SchedSignal::Terminated
        }
    }
    pub fn consume_scheduler_input_boundary<H: Host>(&mut self, host: &mut H) -> bool {
        let fast_forward = self.native_fast_forward_active();
        self.apply_scheduler_button_targets(host, fast_forward)
    }
    fn apply_scheduler_button_targets<H: Host>(
        &mut self,
        host: &mut H,
        fast_forward: bool,
    ) -> bool {
        fn apply(
            frames: &mut [crate::exec::CallFrame],
            mask: u8,
            fast_forward: bool,
        ) -> (bool, bool) {
            let top = frames.len().checked_sub(1);
            let mut jumped = false;
            let mut jumped_top = false;
            for (index, frame) in frames.iter_mut().enumerate() {
                let target = [
                    (fast_forward || mask & 0b001 != 0, frame.jump_target_a),
                    (mask & 0b010 != 0, frame.jump_target_b),
                    (mask & 0b100 != 0, frame.jump_target_c),
                ]
                    .into_iter()
                    .find_map(|(active, target)| {
                        active
                            .then_some(target)
                            .flatten()
                            .filter(|&target| target > frame.cursor.ip)
                    });
                let Some(target) = target else {
                    continue;
                };
                frame.cursor.ip = target;
                frame.jump_target_a = None;
                frame.jump_target_b = None;
                frame.jump_target_c = None;
                frame.wait_deadline = crate::exec::WaitDeadline::None;
                jumped = true;
                jumped_top |= Some(index) == top;
            }
            (jumped, jumped_top)
        }
        let active_mask = host.scheduler_button_latches(self.active_function_id) & 0b111;
        let (mut jumped, active_top) = if active_mask != 0 || fast_forward {
            apply(&mut self.frames, active_mask, fast_forward)
        } else {
            (false, false)
        };
        if active_mask != 0 || active_top {
            crate::text_trace!(
                "[VM] SCHED_INPUT ctx=0x{:08X} mask=0b{active_mask:03b} fast_forward={fast_forward} jumped_top={active_top}",
                self.active_function_id
            );
        }
        if active_top {
            host.consume_scheduler_button_latches(self.active_function_id, 0b111);
            crate::text_trace!(
                "[VM] SCHED_INPUT_CONSUME ctx=0x{:08X} mask=0b111", self
                .active_function_id
            );
        }
        if jumped && self.active_state == CtxState::Waiting {
            self.active_state = CtxState::Running;
        }
        for ctx in self.suspended_ctxs.iter_mut().flatten() {
            let mask = host.scheduler_button_latches(ctx.function_id) & 0b111;
            let (ctx_jumped, ctx_top) = if mask != 0 || fast_forward {
                apply(&mut ctx.frames, mask, fast_forward)
            } else {
                (false, false)
            };
            if mask != 0 || ctx_top {
                crate::text_trace!(
                    "[VM] SCHED_INPUT ctx=0x{:08X} mask=0b{mask:03b} fast_forward={fast_forward} jumped_top={ctx_top}",
                    ctx.function_id
                );
            }
            if ctx_top {
                host.consume_scheduler_button_latches(ctx.function_id, 0b111);
                crate::text_trace!(
                    "[VM] SCHED_INPUT_CONSUME ctx=0x{:08X} mask=0b111", ctx.function_id
                );
            }
            if ctx_jumped && ctx.state == CtxState::Waiting {
                ctx.state = CtxState::Running;
            }
            jumped |= ctx_jumped;
        }
        jumped
    }
    fn mark_ctx_finished(&mut self) {
        if self.current_ctx != 0 {
            self.active_state = CtxState::Inactive;
            self.frames.clear();
        }
    }
    pub fn start_scene(
        &mut self,
        script_idx: usize,
        entry_offset: usize,
        label: impl Into<String>,
    ) -> Result<(), VmError> {
        if self.current_ctx != 0
            && self.suspended_ctxs.first().and_then(Option::as_ref).is_some()
        {
            self.switch_ctx(0);
        }
        if self.frames.is_empty() {
            self.active_state = CtxState::Running;
            self.active_phase = crate::exec::CtxPhase::Initial;
            self.active_finalizer_target = None;
            self.push_frame(script_idx, entry_offset, label)?;
        } else {
            let new_idx = self.create_ctx(self.suspended_ctxs.len() as u32);
            self.switch_ctx(new_idx);
            self.push_frame(script_idx, entry_offset, label)?;
        }
        Ok(())
    }
}

