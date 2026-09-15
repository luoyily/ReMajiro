use crate::exec::WaitDeadline;
use crate::host::Host;
use crate::host::{FrameInputCallbacks, Hotspot};
use crate::value::Value;
use super::inner::InnerOutcome;
pub(super) const HASHES: &[u32] = &[
    0x008ACBC0, 0x1204D7E8, 0xF63CB36C, 0xD6C3BBAC, 0x171AEE22, 0x2075C300, 0x892E9B20,
    0xB7E3E81C, 0x3670B283, 0x3C84073C, 0x72DB7ABF, 0x929A367D, 0xD4590BD3, 0xEFD49BC9,
    0x3A3BFB29, 0xA7C6E918, 0xD538F7B5, 0xF8D18340, 0xA909A32A, 0xAF6571AB, 0x18423EAA,
    0x6F5689E4, 0x78A31C03, 0xA79731A2, 0xD14A4D9C,
];
impl crate::exec::Vm {
    pub(super) fn handle_inner_input<H: Host>(
        &mut self,
        hash: u32,
        count: usize,
        has_retval: bool,
        host: &mut H,
        args: &[Value],
    ) -> Option<Result<InnerOutcome, crate::exec::VmError>> {
        match hash {
            0xB7E3E81C => {
                let mask = top_int(args, count, 0).unwrap_or(1);
                host.set_click_suppression_mask(mask);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x008ACBC0 => {
                let now = host.get_timestamp();
                let Some(frame) = self.frames.last_mut() else {
                    return Some(Err(crate::exec::VmError::Exit));
                };
                let newly_armed = matches!(frame.wait_deadline, WaitDeadline::None);
                if let WaitDeadline::At(deadline) = frame.wait_deadline {
                    let wrapped_distance = now.wrapping_sub(deadline).unsigned_abs();
                    if now < deadline && wrapped_distance <= 0x0293_2E00 {
                        return Some(Ok(InnerOutcome::Waiting));
                    }
                }
                let mask = if self.input_suppressed && !host.input_suppression_bypassed()
                {
                    0
                } else {
                    host.recent_key_event_mask(self.active_function_id) & 0x3f
                };
                if mask == 0 {
                    if newly_armed {
                        crate::text_trace!(
                            "[VM] INPUT_WAIT_MASK_ARM ctx=0x{:08X} now={now}", self
                            .active_function_id
                        );
                    }
                    frame.wait_deadline = WaitDeadline::At(now.wrapping_add(1));
                    return Some(Ok(InnerOutcome::Waiting));
                }
                crate::text_trace!(
                    "[VM] INPUT_WAIT_MASK_DONE ctx=0x{:08X} now={now} mask=0b{mask:06b}",
                    self.active_function_id
                );
                frame.wait_deadline = WaitDeadline::None;
                self.stack.push(Value::int(mask as i32));
                Some(Ok(InnerOutcome::Normal))
            }
            0x1204D7E8 => {
                let now = host.get_timestamp();
                let Some(frame) = self.frames.last_mut() else {
                    return Some(Err(crate::exec::VmError::Exit));
                };
                let newly_armed = matches!(frame.wait_deadline, WaitDeadline::None);
                if let WaitDeadline::At(deadline) = frame.wait_deadline {
                    let wrapped_distance = now.wrapping_sub(deadline).unsigned_abs();
                    if now < deadline && wrapped_distance <= 0x0293_2E00 {
                        return Some(Ok(InnerOutcome::Waiting));
                    }
                }
                let active = !self.input_suppressed || host.input_suppression_bypassed();
                if active && host.four_key_wait_active() {
                    if newly_armed {
                        crate::text_trace!(
                            "[VM] INPUT_WAIT_RELEASE_ARM ctx=0x{:08X} now={now}", self
                            .active_function_id
                        );
                    }
                    frame.wait_deadline = WaitDeadline::At(now.wrapping_add(1));
                    return Some(Ok(InnerOutcome::Waiting));
                }
                crate::text_trace!(
                    "[VM] INPUT_WAIT_RELEASE_DONE ctx=0x{:08X} now={now}", self
                    .active_function_id
                );
                frame.wait_deadline = WaitDeadline::None;
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xD6C3BBAC => {
                let playing = host.movie_is_playing();
                self.stack.push(Value::int(i32::from(playing)));
                Some(Ok(InnerOutcome::Normal))
            }
            0xF63CB36C => {
                let now = host.get_timestamp();
                let context_id = self.active_function_id;
                let Some(frame) = self.frames.last_mut() else {
                    return Some(Err(crate::exec::VmError::Exit));
                };
                if let WaitDeadline::At(deadline) = frame.wait_deadline {
                    let wrapped_distance = now.wrapping_sub(deadline).unsigned_abs();
                    if now < deadline && wrapped_distance <= 0x0293_2E00 {
                        return Some(Ok(InnerOutcome::Waiting));
                    }
                }
                let wait_for_event = if count >= 1 {
                    args
                        .get(count.min(args.len()).saturating_sub(1))
                        .and_then(Value::as_int)
                        .unwrap_or(0) != 0
                } else {
                    true
                };
                let done = !host.movie_is_playing()
                    || (wait_for_event && host.movie_wait_event_flags(context_id) != 0)
                    || host.movie_wait_cancel_pressed();
                if done {
                    host.clear_movie_wait_input(context_id);
                    frame.wait_deadline = WaitDeadline::None;
                    self.stack.push(Value::int(0));
                    Some(Ok(InnerOutcome::Normal))
                } else {
                    frame.wait_deadline = WaitDeadline::At(now.wrapping_add(10));
                    Some(Ok(InnerOutcome::Waiting))
                }
            }
            0x2075C300 => {
                let mask = if self.input_suppressed && !host.input_suppression_bypassed()
                {
                    0
                } else {
                    host.key_modifier_mask()
                };
                eprintln!("[INPUT] get_key_state → mask=0x{:X}", mask);
                self.stack.push(Value::int(mask as i32));
                Some(Ok(InnerOutcome::Normal))
            }
            0x892E9B20 => {
                let mask = if self.input_suppressed && !host.input_suppression_bypassed()
                {
                    0
                } else {
                    host.recent_key_event_mask(self.active_function_id) & 0x3f
                };
                self.stack.push(Value::int(mask as i32));
                Some(Ok(InnerOutcome::Normal))
            }
            0x929A367D => {
                let on = (!self.input_suppressed || host.input_suppression_bypassed())
                    && host.key_capslock_on();
                eprintln!("[INPUT] get_key_state_flags (CapsLock) → {}", on);
                self.stack.push(Value::int(on as i32));
                Some(Ok(InnerOutcome::Normal))
            }
            0x3C84073C => {
                let held = host.key_shift_pressed();
                eprintln!("[INPUT] win_GetKeyState(VK_SHIFT) → held={}", held);
                let mask = if held {
                    0
                } else {
                    i32::from(self.native_mode_bits & 0b111)
                };
                self.stack.push(Value::int(mask));
                Some(Ok(InnerOutcome::Normal))
            }
            0xEFD49BC9 => {
                let x = host.mouse_x();
                eprintln!("[INPUT] get_mouse_x → {}", x);
                self.stack.push(Value::int(x));
                Some(Ok(InnerOutcome::Normal))
            }
            0x72DB7ABF => {
                let y = host.mouse_y();
                eprintln!("[INPUT] get_mouse_y → {}", y);
                self.stack.push(Value::int(y));
                Some(Ok(InnerOutcome::Normal))
            }
            0x171AEE22 => {
                let x = top_int(args, count, 0).unwrap_or(0);
                let y = top_int(args, count, 1).unwrap_or(0);
                host.set_viewport_offset(x, y);
                eprintln!("[INPUT] set_viewport_offset ({},{})", x, y);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xD4590BD3 => {
                let x = top_int(args, count, 0).unwrap_or(0);
                let y = top_int(args, count, 1).unwrap_or(0);
                host.warp_cursor(x, y);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x3670B283 => {
                let inside = host.cursor_inside();
                eprintln!("[INPUT] get_cursor_screen_pos → inside={inside}");
                self.stack.push(Value::int(inside as i32));
                Some(Ok(InnerOutcome::Normal))
            }
            0xF8D18340 => {
                let id = top_int(args, count, 0).unwrap_or(0);
                let x = top_int(args, count, 1).unwrap_or(0);
                let y = top_int(args, count, 2).unwrap_or(0);
                let w = top_int(args, count, 3).unwrap_or(0);
                let h = top_int(args, count, 4).unwrap_or(0);
                host.add_hotspot(
                    self.active_function_id,
                    self.frames.len(),
                    Hotspot {
                        id,
                        x,
                        y,
                        width: w,
                        height: h,
                        on_enter: top_int(args, count, 5).unwrap_or(0) as u32,
                        on_leave: top_int(args, count, 6).unwrap_or(0) as u32,
                        on_click: top_int(args, count, 7).unwrap_or(0) as u32,
                        group: top_int(args, count, 8).unwrap_or(0),
                    },
                );
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xA7C6E918 => {
                host.set_frame_input_callbacks(
                    self.active_function_id,
                    self.frames.len(),
                    FrameInputCallbacks {
                        event_a: top_int(args, count, 0).unwrap_or(0) as u32,
                        event_b: top_int(args, count, 1).unwrap_or(0) as u32,
                        event_c: top_int(args, count, 2).unwrap_or(0) as u32,
                        key_event: top_int(args, count, 3).unwrap_or(0) as u32,
                    },
                );
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xD538F7B5 => {
                let x = top_int(args, count, 0).unwrap_or(0);
                let y = top_int(args, count, 1).unwrap_or(0);
                host.set_hotspot_origin(self.active_function_id, x, y);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x3A3BFB29 => {
                if let Some(event) = host
                    .hotspot_process(self.active_function_id, self.frames.len())
                {
                    self.pending_hotspot_callback = Some(event);
                }
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xA909A32A => {
                let a0 = top_int(args, count, 0).unwrap_or(0);
                let a1 = top_int(args, count, 1).unwrap_or(0);
                let a2 = top_int(args, count, 2).unwrap_or(0);
                host.drag_init(a0, a1, a2);
                eprintln!("[DRAG] init ({},{},{})", a0, a1, a2);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xAF6571AB => {
                host.drag_end();
                eprintln!("[DRAG] end");
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xA79731A2 => {
                let name = top_str(args, count, 0);
                let visible = top_int(args, count, 1).unwrap_or(0) != 0;
                let x = top_int(args, count, 2).unwrap_or(0);
                let y = top_int(args, count, 3).unwrap_or(0);
                let w = top_int(args, count, 4).unwrap_or(0);
                let h = top_int(args, count, 5).unwrap_or(0);
                let skip = has_retval && self.native_effect_skip_active(host);
                if !skip {
                    host.movie_play(name, visible, x, y, w, h);
                }
                eprintln!(
                    "[VIDEO] play_a {:?} visible={} rect=({},{},{},{}) skip={}",
                    String::from_utf8_lossy(name), visible, x, y, w, h, skip
                );
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xD14A4D9C => {
                let name = top_str(args, count, 0);
                let visible = top_int(args, count, 1).unwrap_or(0) != 0;
                let x = top_int(args, count, 2).unwrap_or(0);
                let y = top_int(args, count, 3).unwrap_or(0);
                let w = top_int(args, count, 4).unwrap_or(0);
                let h = top_int(args, count, 5).unwrap_or(0);
                let skip = has_retval && self.native_effect_skip_active(host);
                if !skip {
                    host.movie_play(name, visible, x, y, w, h);
                }
                eprintln!(
                    "[VIDEO] play_b {:?} visible={} rect=({},{},{},{}) skip={}",
                    String::from_utf8_lossy(name), visible, x, y, w, h, skip
                );
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x18423EAA => {
                let name = top_str(args, count, 0);
                let w = top_int(args, count, 1).unwrap_or(0);
                let h = top_int(args, count, 2).unwrap_or(0);
                let visible = top_int(args, count, 3).unwrap_or(0) != 0;
                let x = top_int(args, count, 4).unwrap_or(0);
                let y = top_int(args, count, 5).unwrap_or(0);
                host.movie_play(name, visible, x, y, w, h);
                eprintln!(
                    "[VIDEO] play_c {:?} visible={} rect=({},{},{},{})",
                    String::from_utf8_lossy(name), visible, x, y, w, h
                );
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x6F5689E4 => {
                let name = top_str(args, count, 0);
                let loop_ = top_int(args, count, 1).unwrap_or(0) != 0;
                let start_pos = top_int(args, count, 2).unwrap_or(0).max(0) as u32;
                let duration = top_int(args, count, 3).unwrap_or(-1);
                let skip = has_retval && self.native_effect_skip_active(host);
                if !skip {
                    host.movie_texture_play(name, loop_, start_pos, duration);
                }
                eprintln!(
                    "[VIDEO] texture_play {:?} loop={} start={} dur={} skip={}",
                    String::from_utf8_lossy(name), loop_, start_pos, duration, skip
                );
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x78A31C03 => {
                let page = host.movie_frame_update();
                eprintln!("[VIDEO] frame_update → page={}", page);
                self.stack.push(Value::int(page as i32));
                Some(Ok(InnerOutcome::Normal))
            }
            _ => None,
        }
    }
}
fn from_top(args: &[Value], count: usize, n: usize) -> Option<&Value> {
    let real = count.min(args.len());
    if n >= real {
        return None;
    }
    args.get(real - 1 - n)
}
fn top_int(args: &[Value], count: usize, n: usize) -> Option<i32> {
    from_top(args, count, n).and_then(|v| v.as_int())
}
fn top_str(args: &[Value], count: usize, n: usize) -> &[u8] {
    from_top(args, count, n).and_then(|v| v.as_str_bytes()).unwrap_or(&[])
}

