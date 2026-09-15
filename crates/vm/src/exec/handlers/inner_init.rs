use super::helpers::InnerArgs;
use super::inner::InnerOutcome;
use crate::exec::WaitDeadline;
use crate::host::Host;
use crate::value::Value;
pub(super) const HASHES: &[u32] = &[
    0x0C93FCB4, 0x111BD910, 0x265E07D7, 0x42F6FDDF, 0x4662E95D, 0x52819912, 0x57EFA275,
    0x57F6310D, 0x6DE6CDFC, 0x7BFFC5B1, 0x83D81F59, 0x8D84B83F, 0x90816C6E, 0xA62AA5EB,
    0xB84C9953, 0xC21F8B49, 0xCF2B1E50, 0xDA675785, 0xDCC706EF, 0xE119D5BA, 0xE186D19A,
    0xF3679C34, 0xF80BC9B6, 0xF94B3586, 0xFF75CD29, 0xFF7C52E6,
];
impl crate::exec::Vm {
    pub(super) fn handle_inner_init<H: Host>(
        &mut self,
        hash: u32,
        count: usize,
        _has_retval: bool,
        host: &mut H,
        args: &[Value],
    ) -> Option<InnerOutcome> {
        match hash {
            0xE119D5BA => {
                let args = InnerArgs::new(args, count);
                let sprite = args.int_or(0, 0) as u32;
                let x = args.int_or(1, 0);
                let y = args.int_or(2, 0);
                let timing = args.int(3);
                host.sprite_move(sprite, x, y, timing);
                self.stack.push(Value::int(0));
                Some(InnerOutcome::Normal)
            }
            0xE186D19A => {
                let args = InnerArgs::new(args, count);
                let face = args.bytes_or_empty(0);
                let size = args.int_or(1, 20);
                let width = args.int_or(2, -1);
                let line_height = args.int_or(3, -1);
                let flags = args.int_or(4, -1);
                host.fontout_set_style(
                    self.active_function_id,
                    face,
                    size,
                    width,
                    line_height,
                    flags,
                );
                self.stack.push(Value::int(0));
                Some(InnerOutcome::Normal)
            }
            0x265E07D7 => {
                let sprite = InnerArgs::new(args, count).int_or(0, 0) as u32;
                self.stack.push(Value::int(host.sprite_get_page(sprite) as i32));
                Some(InnerOutcome::Normal)
            }
            0x7BFFC5B1 => {
                let args = InnerArgs::new(args, count);
                let sprite = args.int_or(0, 0) as u32;
                let reference = args.int(1).map(|value| value as u32);
                host.sprite_priority_low_single(sprite, reference);
                self.stack.push(Value::int(0));
                Some(InnerOutcome::Normal)
            }
            0xFF75CD29 => {
                let duration_ms = args.first().and_then(|v| v.as_int()).unwrap_or(0);
                let base_timestamp = args.get(1).and_then(|v| v.as_int()).unwrap_or(0);
                if !host.cooperative_timers() {
                    self.stack
                        .push(Value::int(base_timestamp.wrapping_add(duration_ms)));
                    return Some(InnerOutcome::Normal);
                }
                let now = host.get_timestamp();
                let frame = self.frames.last_mut()?;
                match frame.wait_deadline {
                    WaitDeadline::At(
                        deadline,
                    ) if now < deadline
                        && now.wrapping_sub(deadline).wrapping_abs() <= 0x0293_2E00 => {
                        Some(InnerOutcome::Waiting)
                    }
                    WaitDeadline::At(deadline) => {
                        frame.wait_deadline = WaitDeadline::None;
                        self.stack.push(Value::int(deadline.max(now.wrapping_sub(10))));
                        Some(InnerOutcome::Normal)
                    }
                    WaitDeadline::Never => Some(InnerOutcome::Waiting),
                    WaitDeadline::None => {
                        let reset_threshold = duration_ms.wrapping_mul(10);
                        let base = if now.wrapping_sub(base_timestamp).wrapping_abs()
                            >= reset_threshold
                        {
                            now
                        } else {
                            base_timestamp
                        };
                        frame.wait_deadline = WaitDeadline::At(
                            base.wrapping_add(duration_ms),
                        );
                        Some(InnerOutcome::Waiting)
                    }
                }
            }
            0xDA675785 => {
                let get = |i: usize| args.get(i).and_then(|v| v.as_int()).unwrap_or(0);
                let src_page = get(9) as u32;
                let src_x = get(8);
                let src_y = get(7);
                let src_w = get(6);
                let src_h = get(5);
                let dst_page = get(4) as u32;
                let dst_x = get(3);
                let dst_y = get(2);
                let dst_w = get(1);
                let dst_h = get(0);
                host.grp_modcopy(
                    dst_page,
                    dst_x,
                    dst_y,
                    dst_w,
                    dst_h,
                    src_page,
                    src_x,
                    src_y,
                    src_w,
                    src_h,
                );
                self.stack.push(Value::int(0));
                Some(InnerOutcome::Normal)
            }
            0x83D81F59 => {
                let (w, h, indexed) = decode_page_create_args(args, count);
                let page = host.page_create(w, h, indexed);
                self.stack.push(Value::int(page as i32));
                Some(InnerOutcome::Normal)
            }
            0xF80BC9B6 => {
                let (sprite, keyframes, _) = decode_sprite_keyframes(args, count);
                host.sprite_alfa_define(sprite, &keyframes);
                self.stack.push(Value::int(0));
                Some(InnerOutcome::Normal)
            }
            0x52819912 => {
                let (sprite, keyframes, total_duration) = decode_sprite_keyframes(
                    args,
                    count,
                );
                host.sprite_animate_define_aligned(sprite, &keyframes, total_duration);
                self.stack.push(Value::int(0));
                Some(InnerOutcome::Normal)
            }
            0x57F6310D => {
                let args = InnerArgs::new(args, count);
                host.font_locate(
                    self.active_function_id,
                    args.int_or(0, 0) as u32,
                    args.int_or(1, 0),
                    args.int_or(2, 0),
                );
                self.stack.push(Value::int(0));
                Some(InnerOutcome::Normal)
            }
            0x90816C6E => {
                let args = InnerArgs::new(args, count);
                host.fontout_set_colors(
                    self.active_function_id,
                    args.int_or(0, 0) as u32,
                    args.int_or(1, -1),
                );
                self.stack.push(Value::int(0));
                Some(InnerOutcome::Normal)
            }
            0xF3679C34 => {
                let line = format_debug_stack(args, count);
                host.debug_log_stack(&line);
                self.stack.push(Value::int(0));
                Some(InnerOutcome::Normal)
            }
            0x111BD910 => {
                self.stack.push(Value::int(host.get_effect_speed()));
                Some(InnerOutcome::Normal)
            }
            0x0C93FCB4 => {
                let speed = InnerArgs::new(args, count).int_or(0, 1000);
                host.set_effect_speed(speed);
                self.stack.push(Value::int(0));
                Some(InnerOutcome::Normal)
            }
            0xDCC706EF => {
                let handle = InnerArgs::new(args, count).int_or(0, 0) as u32;
                let exists = host.sprite_exists(handle);
                self.stack.push(Value::int(if exists { 1 } else { 0 }));
                Some(InnerOutcome::Normal)
            }
            0xFF7C52E6 => {
                let args = InnerArgs::new(args, count);
                host.sprite_alfa_set(
                    args.int_or(0, 0) as u32,
                    args.int_or(1, 255),
                    args.int_or(2, 0),
                );
                self.stack.push(Value::int(0));
                Some(InnerOutcome::Normal)
            }
            0xF94B3586 => {
                let (w, h, indexed) = decode_page_create_args(args, count);
                let page = host.page_create_with_antidata(w, h, indexed);
                self.stack.push(Value::int(page as i32));
                Some(InnerOutcome::Normal)
            }
            0x8D84B83F => {
                let page = InnerArgs::new(args, count).int_or(0, 0) as u32;
                let invalid = host.page_is_invalid(page);
                self.stack.push(Value::int(if invalid { 1 } else { 0 }));
                Some(InnerOutcome::Normal)
            }
            0x57EFA275 => {
                let filename = InnerArgs::new(args, count).bytes_or_empty(0);
                let page = host.page_create_file_alpha(filename);
                self.stack.push(Value::int(page as i32));
                Some(InnerOutcome::Normal)
            }
            0x4662E95D => {
                let now = host.get_timestamp();
                let present_epoch = host.present_epoch();
                let frozen = host.scene_freeze_active();
                let frame = self.frames.last_mut()?;
                if let Some(armed_epoch) = frame.wait_present_epoch {
                    if frozen {
                        frame.wait_deadline = WaitDeadline::None;
                        frame.wait_present_epoch = None;
                        self.stack.push(Value::int(0));
                        return Some(InnerOutcome::Normal);
                    }
                    let target = match frame.wait_deadline {
                        WaitDeadline::At(target) => target,
                        _ => now,
                    };
                    if now <= target {
                        return Some(InnerOutcome::Waiting);
                    }
                    if present_epoch != armed_epoch {
                        frame.wait_deadline = WaitDeadline::None;
                        frame.wait_present_epoch = None;
                        self.stack.push(Value::int(0));
                        return Some(InnerOutcome::Normal);
                    }
                    frame.wait_deadline = WaitDeadline::At(now.wrapping_add(1));
                    return Some(InnerOutcome::Waiting);
                }
                frame.wait_deadline = WaitDeadline::At(now.wrapping_add(1));
                frame.wait_present_epoch = Some(present_epoch);
                if InnerArgs::new(args, count).int_or(0, 0) == 0 {
                    host.request_present();
                }
                Some(InnerOutcome::Waiting)
            }
            0x42F6FDDF => {
                host.normalize_scheduler_delta();
                self.stack.push(Value::int(0));
                Some(InnerOutcome::Normal)
            }
            0xCF2B1E50 => {
                if crate::frame_probe_enabled() {
                    crate::text_trace!(
                        "[FRAME-PROBE] VM_FREEZE_BEGIN ctx=0x{:08X} script={:?} ip={:?}",
                        self.current_function_id(), self.current_script_idx(), self
                        .current_ip()
                    );
                }
                host.scene_freeze_begin();
                self.stack.push(Value::int(0));
                Some(InnerOutcome::Normal)
            }
            0x6DE6CDFC => {
                if crate::frame_probe_enabled() {
                    crate::text_trace!(
                        "[FRAME-PROBE] VM_FREEZE_END ctx=0x{:08X} script={:?} ip={:?}",
                        self.current_function_id(), self.current_script_idx(), self
                        .current_ip()
                    );
                }
                host.scene_freeze_end();
                self.stack.push(Value::int(0));
                Some(InnerOutcome::Normal)
            }
            0xB84C9953 => {
                let args = InnerArgs::new(args, count);
                host.scene_set_origin(args.int_or(1, 0), args.int_or(0, 0));
                self.stack.push(Value::int(0));
                Some(InnerOutcome::Normal)
            }
            0xC21F8B49 => {
                let caps_auto = host.config_load(b"CAPSMODE", 0) == 1
                    && host.key_capslock_on();
                let script_auto = self.native_mode_bits & 0b100 != 0;
                self.stack.push(Value::int((caps_auto || script_auto) as i32));
                Some(InnerOutcome::Normal)
            }
            0xA62AA5EB => {
                let pending = self.native_mode_pending_exact(host);
                self.stack.push(Value::int(i32::from(pending)));
                Some(InnerOutcome::Normal)
            }
            _ => None,
        }
    }
}
fn format_debug_stack(args: &[Value], count: usize) -> Vec<u8> {
    use crate::value::{
        TAG_FIXED_INT_ARRAY, TAG_FLOAT, TAG_INT, TAG_INT_ARRAY, TAG_STRING,
        TAG_STRING_ARRAY,
    };
    let mut output = Vec::new();
    for value in args.iter().take(count.min(args.len())).rev() {
        let fragment = match value.type_tag {
            TAG_INT => format!("{} ", value.as_int().unwrap_or_default()).into_bytes(),
            TAG_FLOAT => format!("{:.6} ", f32::from_bits(value.bits)).into_bytes(),
            TAG_STRING => {
                let mut bytes = value.as_str_bytes().unwrap_or_default().to_vec();
                if let Some(nul) = bytes.iter().position(|byte| *byte == 0) {
                    bytes.truncate(nul);
                }
                bytes.push(b' ');
                bytes
            }
            TAG_INT_ARRAY => b"[INT_ARRAY] ".to_vec(),
            TAG_FIXED_INT_ARRAY => b"[FIXED_ARRAY] ".to_vec(),
            TAG_STRING_ARRAY => b"[STRING_ARRAY] ".to_vec(),
            _ => Vec::new(),
        };
        output.extend_from_slice(&fragment);
    }
    output
}
fn decode_sprite_keyframes(args: &[Value], count: usize) -> (u32, Vec<(i32, i32)>, i32) {
    let real = count.min(args.len());
    let sprite = args.get(real.saturating_sub(1)).and_then(Value::as_int).unwrap_or(0)
        as u32;
    let mut keyframes = Vec::new();
    let mut total_duration = 0_i32;
    let mut remaining = real.saturating_sub(1);
    let mut cursor = real.saturating_sub(2);
    while remaining >= 2 {
        let target = args.get(cursor).and_then(Value::as_int).unwrap_or(0);
        let duration = args
            .get(cursor.saturating_sub(1))
            .and_then(Value::as_int)
            .unwrap_or(0);
        keyframes.push((target, duration));
        total_duration = total_duration.wrapping_add(duration);
        remaining -= 2;
        cursor = cursor.saturating_sub(2);
    }
    if remaining == 1 {
        let target = args.get(cursor).and_then(Value::as_int).unwrap_or(0);
        keyframes.push((target, -1));
    }
    (sprite, keyframes, total_duration)
}
fn decode_page_create_args(args: &[Value], count: usize) -> (i32, i32, bool) {
    let real = count.min(args.len());
    let from_top = |n: usize| (n < real).then(|| &args[real - 1 - n]);
    let width = from_top(0).and_then(Value::as_int).unwrap_or(1);
    let height = from_top(1).and_then(Value::as_int).unwrap_or(1);
    let format_flag = from_top(2).and_then(Value::as_int).unwrap_or(1);
    (width, height, format_flag == 0)
}

