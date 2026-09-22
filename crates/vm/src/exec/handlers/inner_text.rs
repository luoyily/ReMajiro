use crate::host::{Host, TextRenderAction};
use crate::text::{tokenize, ControlCode, ControlCodeKind, TextToken};
use crate::value::Value;
use super::inner::InnerOutcome;
pub(super) const HASHES: &[u32] = &[
    0x06B3C8AC, 0x403960F8, 0x307F28BC, 0x59180BBB, 0x5FDCCCEE, 0xA2743878, 0x979D1F16,
    0xB5A1E3C9, 0x5B87A41D, 0x30FA2A29, 0x5C9ED743, 0xD4066E31, 0x4133D7A9, 0xF1F8A206,
    0x82FD765B, 0x2CB1BC41, 0x6621F84D, 0x88B4B26C, 0xB0CE081A, 0x507F22C6, 0x824D124F,
    0x25D92502, 0xBBF35806, 0x8CFC4573, 0xC32E286F, 0xBA81EF8D, 0x32D0236C,
];
impl crate::exec::Vm {
    pub(super) fn handle_inner_text<H: Host>(
        &mut self,
        hash: u32,
        count: usize,
        _has_retval: bool,
        host: &mut H,
        args: &[Value],
    ) -> Option<Result<InnerOutcome, crate::exec::VmError>> {
        match hash {
            0x32D0236C => {
                let name = arg_from_top(args, count, 0)
                    .and_then(Value::as_str_bytes)
                    .unwrap_or(&[]);
                let channel = arg_int_from_top(args, count, 1) as u8;
                let name = name.split(|byte| *byte == 0).next().unwrap_or(name);
                let mut history = Vec::with_capacity(name.len() + 3);
                history.extend_from_slice(b"\\v");
                history.push(b'0'.saturating_add(channel.min(9)));
                history.extend_from_slice(name);
                self.text.append_history_record(&history);
                self.text.record_voice(name);
                host.text_voice_play(channel, name, 0);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xBA81EF8D => {
                let x = arg_int_from_top(args, count, 0);
                let y = arg_int_from_top(args, count, 1);
                self.text
                    .append_history_record(
                        format!("\\l0x{:08x}0x{:08x}", x as u32, y as u32).as_bytes(),
                    );
                host.text_set_position(x, y);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x8CFC4573 => {
                let mut bytes = self.text.accumulator.clone();
                bytes.push(0);
                self.stack.push(Value::string(bytes));
                Some(Ok(InnerOutcome::Normal))
            }
            0xC32E286F => {
                self.stack.push(Value::string(vec![0]));
                Some(Ok(InnerOutcome::Normal))
            }
            0x2CB1BC41 => {
                let steps = arg_int_from_top(args, count, 0);
                let mut position = self.text_pos_backtrack(-2);
                for _ in 1..steps.max(1) {
                    position = self.text_pos_backtrack(position);
                }
                self.stack.push(Value::int(position));
                Some(Ok(InnerOutcome::Normal))
            }
            0x6621F84D => {
                let position = arg_int_from_top(args, count, 0);
                self.stack.push(Value::int(self.text_pos_backtrack(position)));
                Some(Ok(InnerOutcome::Normal))
            }
            0x88B4B26C => {
                let mut total = 0i32;
                let mut position = self.text_pos_backtrack(-1);
                loop {
                    let previous = position;
                    position = self.text_pos_backtrack_from(position, true);
                    total = total.saturating_add(1);
                    if position == previous {
                        break;
                    }
                }
                self.stack.push(Value::int(total));
                Some(Ok(InnerOutcome::Normal))
            }
            0xB0CE081A => {
                self.stack.push(Value::int(self.text.char_id));
                Some(Ok(InnerOutcome::Normal))
            }
            0x507F22C6 | 0x824D124F => {
                let index = arg_int_from_top(args, count, 0);
                let value = usize::try_from(index)
                    .ok()
                    .and_then(|index| self.text.history_records.get(index))
                    .map(|record| {
                        if hash == 0x507F22C6 { record.value_a } else { record.value_b }
                    })
                    .unwrap_or(0);
                self.stack.push(Value::int(value));
                Some(Ok(InnerOutcome::Normal))
            }
            0x25D92502 | 0xBBF35806 => {
                let index = arg_int_from_top(args, count, 0);
                let bytes = usize::try_from(index)
                    .ok()
                    .and_then(|index| self.text.history_records.get(index))
                    .map(|record| {
                        if hash == 0x25D92502 {
                            record.text.clone()
                        } else {
                            record.voice.clone()
                        }
                    })
                    .unwrap_or_default();
                self.stack.push(Value::string(bytes));
                Some(Ok(InnerOutcome::Normal))
            }
            0x82FD765B => {
                self.stack.push(Value::int(host.text_font_size()));
                Some(Ok(InnerOutcome::Normal))
            }
            0x403960F8 => {
                host.text_configure(
                    arg_int_from_top(args, count, 0) as u32,
                    arg_int_from_top(args, count, 1),
                    arg_int_from_top(args, count, 2),
                    arg_int_from_top(args, count, 3),
                    arg_int_from_top(args, count, 4),
                );
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x06B3C8AC => {
                let face = arg_from_top(args, count, 0)
                    .and_then(Value::as_str_bytes)
                    .unwrap_or(&[]);
                let size = if count > 1 { arg_int_from_top(args, count, 1) } else { -1 };
                let width = if count > 2 {
                    arg_int_from_top(args, count, 2)
                } else {
                    -1
                };
                let line_height = if count > 3 {
                    arg_int_from_top(args, count, 3)
                } else {
                    -1
                };
                let flags = if count > 4 { arg_int_from_top(args, count, 4) } else { 0 };
                let mut history = format!(
                    "\\f0x{:08x}0x{:08x}0x{:08x}0x{:08x}", size as u32, width as u32,
                    line_height as u32, flags as u32
                )
                    .into_bytes();
                history
                    .extend_from_slice(
                        face
                            .get(
                                ..face
                                    .iter()
                                    .position(|byte| *byte == 0)
                                    .unwrap_or(face.len()),
                            )
                            .unwrap_or(face),
                    );
                self.text.append_history_record(&history);
                host.text_set_font(size, width, line_height, flags, face);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x5FDCCCEE => {
                let foreground = arg_int_from_top(args, count, 0) as u32;
                let background = if count > 1 {
                    arg_int_from_top(args, count, 1)
                } else {
                    -1
                };
                self.text
                    .append_history_record(
                        format!("\\c0x{foreground:08x}0x{:08x}", background as u32)
                            .as_bytes(),
                    );
                host.text_set_colors(foreground, background);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xF1F8A206 => {
                host.text_set_position(
                    arg_int_from_top(args, count, 0),
                    arg_int_from_top(args, count, 1),
                );
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x307F28BC => {
                self.text.render_state = 1;
                let position = arg_int_from_top(args, count, 0);
                let outcome = self.replay_text_history_step(position, host);
                if outcome == InnerOutcome::Normal {
                    self.stack.push(Value::int(0));
                }
                Some(Ok(outcome))
            }
            0x59180BBB => {
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xA2743878 => {
                let before = self.text.first_render;
                if !before {
                    let top = arg_int_from_top(args, count, 0);
                    let below = arg_int_from_top(args, count, 1);
                    self.defer_inner_host_bridge(
                        0xF7A4_C8D8,
                        vec![Value::int(below), Value::int(top)],
                        false,
                    );
                }
                self.text.first_render = true;
                crate::text_trace!(
                    "[VM] TEXT_RENDER_SIMPLE first_render={before}->true pending_bridges={}",
                    self.pending_inner_host_bridges.len()
                );
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x979D1F16 => {
                let before = self.text.first_render;
                if before {
                    let top = arg_int_from_top(args, count, 0);
                    let below = arg_int_from_top(args, count, 1);
                    self.defer_inner_host_bridge(
                        0x0D9B_7F0E,
                        vec![Value::int(below), Value::int(top)],
                        false,
                    );
                }
                self.text.first_render = false;
                crate::text_trace!(
                    "[VM] TEXT_RENDER_EVENT first_render={before}->false pending_bridges={}",
                    self.pending_inner_host_bridges.len()
                );
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xB5A1E3C9 => {
                self.text.append_history_record(b"\\w");
                host.text_control(
                    &ControlCode {
                        kind: ControlCodeKind::Wait,
                        payload: Vec::new(),
                    },
                );
                self.text.reset_for_wait();
                if let Some(frame) = self
                    .frames
                    .iter_mut()
                    .rev()
                    .find(|frame| frame.text_line_pending)
                {
                    frame.text_line_pending = false;
                }
                self.defer_inner_host_bridge(0x93B3_8A0B, Vec::new(), false);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x5B87A41D => {
                crate::text_trace!(
                    "[VM] TEXT_FIRST_RENDER_QUERY ctx=0x{:08X} value={}", self
                    .active_function_id, self.text.first_render
                );
                eprintln!(
                    "[PAUSE-DIAG] text_first_render={}", i32::from(self.text
                    .first_render)
                );
                self.stack.push(Value::int(i32::from(self.text.first_render)));
                Some(Ok(InnerOutcome::Normal))
            }
            0x30FA2A29 => {
                self.text.render_state = 1;
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x5C9ED743 => {
                self.text.render_state = 0;
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xD4066E31 => {
                self.text.skip_depth = self.text.skip_depth.wrapping_add(1);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x4133D7A9 => {
                self.text.skip_depth = self.text.skip_depth.wrapping_sub(1);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            _ => None,
        }
    }
    fn text_pos_backtrack(&self, position: i32) -> i32 {
        match position {
            -2 => self.text_pos_backtrack_from(40_000, false),
            -1 => self.text_pos_backtrack_from(40_000, true),
            position => self.text_pos_backtrack_from(position, false),
        }
    }
    fn text_pos_backtrack_from(&self, start: i32, force_newline: bool) -> i32 {
        let mut cursor = start;
        let mut saw_content = false;
        let Some(mut length) = read_history_u16(&self.text.history_buf, cursor - 2) else {
            return start;
        };
        while length != 0 {
            cursor = cursor.saturating_sub(i32::from(length));
            if cursor <= 2 {
                break;
            }
            let byte = history_byte(&self.text.history_buf, cursor);
            if byte == b'\\' {
                if history_byte(&self.text.history_buf, cursor + 1) == b'p' {
                    saw_content = true;
                }
            } else {
                saw_content = true;
            }
            if text_pos_is_valid(&self.text.history_buf, cursor, force_newline)
                && (saw_content || start == 40_000)
            {
                return cursor;
            }
            let Some(next) = read_history_u16(&self.text.history_buf, cursor - 2) else {
                break;
            };
            length = next;
        }
        start
    }
    fn replay_text_history_step<H: Host>(
        &mut self,
        position: i32,
        host: &mut H,
    ) -> InnerOutcome {
        if self.text.replay.is_none() {
            let cursor = match position {
                -2 => self.text_pos_backtrack(-2),
                -1 => self.text_pos_backtrack(-1),
                position => position,
            };
            if !(0..40_000).contains(&cursor) {
                crate::text_trace!(
                    "[VM] TEXT_HISTORY_REPLAY position={position} cursor={cursor} records=0"
                );
                self.text.render_state = 0;
                return InnerOutcome::Normal;
            }
            self.text.display_state = if position < 0 { -1 } else { 1 };
            self.text.char_id = 0;
            self.text.history_records.fill(Default::default());
            self.text.replay = Some(crate::exec::TextReplayState {
                cursor,
                first_record: true,
                page_pause: false,
                force_newline: position == -1,
                rendering: false,
            });
        }
        let mut state = self.text.replay.take().expect("replay initialized");
        for _ in 0..20_000 {
            if state.rendering || !self.text.pending_render_line.is_empty() {
                if !state.rendering {
                    if let Some((name, remainder)) = crate::exec::split_name_popup(
                        &self.text.pending_render_line,
                    ) {
                        self.text.pending_render_line = remainder;
                        let name_value = if self.use_localized_first_line(host) {
                            crate::exec::popup_name_value(host, &name)
                        } else {
                            Value::string(name.clone())
                        };
                        self.defer_inner_host_bridge(
                            0x44A4_FF72,
                            vec![name_value],
                            false,
                        );
                        self.text.replay = Some(state);
                        return InnerOutcome::Reenter;
                    }
                    let bytes = std::mem::take(&mut self.text.pending_render_line);
                    let site = self
                        .frames
                        .last()
                        .and_then(|frame| self.scripts.get(frame.script_idx))
                        .map(|script| crate::host::TextSite {
                            script_name: script.name.clone().unwrap_or_default(),
                            render_offset: usize::MAX,
                            code_crc32: script.code_crc32,
                        })
                        .unwrap_or(crate::host::TextSite {
                            script_name: Vec::new(),
                            render_offset: usize::MAX,
                            code_crc32: 0,
                        });
                    let start_y = host.get_text_pos_y();
                    let markers = crate::exec::inline_record_markers(&bytes);
                    host.text_line(&site, &bytes);
                    self.text
                        .update_inline_records(
                            markers,
                            start_y,
                            host.get_text_pos_y(),
                            host.text_line_height(),
                        );
                    state.rendering = true;
                }
                if let Some(mut info) = host.text_render() {
                    info.render_state = self.text.render_state;
                    match info.action {
                        TextRenderAction::PageOverflow => {
                            self.defer_inner_host_bridge(0x93B3_8A0B, Vec::new(), false);
                        }
                        TextRenderAction::Continue => {}
                        TextRenderAction::Complete => state.rendering = false,
                    }
                    if info.action != TextRenderAction::PageOverflow {
                        self.defer_inner_host_bridge(
                            0x47BB_540C,
                            vec![
                                Value::int(info.render_state), Value::int(info.rect[3]),
                                Value::int(info.rect[2]), Value::int(info.rect[1]),
                                Value::int(info.rect[0]), Value::int(info.page as i32),
                            ],
                            false,
                        );
                    }
                    self.text.replay = Some(state);
                    return InnerOutcome::Reenter;
                }
                state.rendering = false;
            }
            let Some((payload, next)) = history_record_at(
                    &self.text.history_buf,
                    state.cursor,
                )
                .map(|(payload, next)| (payload.to_vec(), next as i32)) else {
                return self.finish_text_history_replay(state, host);
            };
            if payload.starts_with(b"\\w") && !state.first_record {
                return self.finish_text_history_replay(state, host);
            }
            if payload.first().copied() != Some(b'\\') {
                self.text.pending_render_line = payload;
                state.cursor = next;
                if state.first_record {
                    state.first_record = false;
                    self.replay_wait(host);
                    self.text.replay = Some(state);
                    return InnerOutcome::Reenter;
                }
                state.first_record = false;
                continue;
            }
            let control = tokenize(&payload)
                .into_iter()
                .find_map(|token| match token {
                    TextToken::Control(control) => Some(control),
                    TextToken::Text(_) => None,
                });
            let Some(control) = control else {
                state.cursor = next;
                state.first_record = false;
                self.text.replay = Some(state);
                return InnerOutcome::Reenter;
            };
            if control.kind == ControlCodeKind::Newline && !state.force_newline {
                if state.first_record {
                    state.cursor = next;
                    state.first_record = false;
                    self.replay_wait(host);
                    self.text.replay = Some(state);
                    return InnerOutcome::Reenter;
                }
                if state.page_pause {
                    return self.finish_text_history_replay(state, host);
                }
            }
            state.cursor = next;
            match control.kind {
                ControlCodeKind::PagePause => {
                    state.page_pause = true;
                    self.replay_apply_control(
                        ControlCode {
                            kind: ControlCodeKind::PageClear,
                            payload: Vec::new(),
                        },
                        host,
                    );
                }
                ControlCodeKind::Color => {}
                ControlCodeKind::Voice => {
                    if let Some((_, name)) = control.voice() {
                        self.text.record_voice(name);
                    }
                }
                kind => {
                    self.replay_apply_control(control, host);
                    if state.first_record && kind != ControlCodeKind::Wait {
                        self.replay_wait(host);
                    }
                }
            }
            state.first_record = false;
            self.text.replay = Some(state);
            return InnerOutcome::Reenter;
        }
        self.finish_text_history_replay(state, host)
    }
    fn replay_wait<H: Host>(&mut self, host: &mut H) {
        host.text_control(
            &ControlCode {
                kind: ControlCodeKind::Wait,
                payload: Vec::new(),
            },
        );
        self.text.reset_for_replay_wait();
        self.defer_inner_host_bridge(0x93B3_8A0B, Vec::new(), false);
    }
    fn replay_apply_control<H: Host>(&mut self, control: ControlCode, host: &mut H) {
        let kind = control.kind;
        host.text_control(&control);
        match kind {
            ControlCodeKind::Newline | ControlCodeKind::NewlineRelative => {
                if kind == ControlCodeKind::Newline {
                    self.text.accumulator.clear();
                }
                self.text.page_capture_enabled = false;
                self.text.localized_page_capture_enabled = false;
                self.defer_inner_host_bridge(0x04AE_36BD, Vec::new(), false);
            }
            ControlCodeKind::Wait => {
                self.text.reset_for_replay_wait();
                self.defer_inner_host_bridge(0x93B3_8A0B, Vec::new(), false);
            }
            ControlCodeKind::PageClear => {
                self.text.render_state = 0;
                self.defer_inner_host_bridge(0x7EE6_7053, Vec::new(), false);
            }
            ControlCodeKind::PagePause => {
                self.text.render_state = 0;
                self.defer_inner_host_bridge(0x20CE_505D, Vec::new(), false);
            }
            ControlCodeKind::ExecScript => {
                self.defer_inner_host_bridge(
                    crate::exec::X_CONTROL_NAME_HASH,
                    vec![Value::string(control.payload)],
                    false,
                );
            }
            ControlCodeKind::Graphics => {
                if let Some(fields) = control.graphics() {
                    let args = fields
                        .into_iter()
                        .rev()
                        .flatten()
                        .map(Value::int)
                        .collect();
                    self.defer_inner_host_bridge(0xD3CB_0EE5, args, false);
                }
            }
            ControlCodeKind::Continue => self.text.render_state = 1,
            ControlCodeKind::Color
            | ControlCodeKind::Font
            | ControlCodeKind::Position
            | ControlCodeKind::Speed
            | ControlCodeKind::Delay
            | ControlCodeKind::Voice
            | ControlCodeKind::OffsetSave
            | ControlCodeKind::Restore => {}
        }
    }
    fn finish_text_history_replay<H: Host>(
        &mut self,
        state: crate::exec::TextReplayState,
        host: &mut H,
    ) -> InnerOutcome {
        if self.text.char_id > 0 {
            let index = (self.text.char_id - 1) as usize;
            if let Some(record) = self.text.history_records.get_mut(index) {
                if record.value_b == 0 {
                    record.value_b = host
                        .get_text_pos_y()
                        .saturating_add(host.text_line_height())
                        .saturating_sub(record.value_a);
                }
            }
        }
        self.text.pending_render_line.clear();
        self.text.replay = None;
        self.text.display_state = 0;
        self.text.render_state = 0;
        crate::text_trace!(
            "[VM] TEXT_HISTORY_REPLAY_DONE cursor={} char_records={} page_pause={}",
            state.cursor, self.text.char_id, state.page_pause
        );
        InnerOutcome::Normal
    }
}
fn history_byte(buffer: &[u8], position: i32) -> u8 {
    usize::try_from(position)
        .ok()
        .and_then(|position| buffer.get(position).copied())
        .unwrap_or(0)
}
fn read_history_u16(buffer: &[u8], position: i32) -> Option<u16> {
    let position = usize::try_from(position).ok()?;
    Some(u16::from_le_bytes([*buffer.get(position)?, *buffer.get(position + 1)?]))
}
fn history_record_at(buffer: &[u8], position: i32) -> Option<(&[u8], usize)> {
    let start = usize::try_from(position).ok()?;
    let tail = buffer.get(start..)?;
    let payload_len = tail.iter().position(|byte| *byte == 0)?;
    let nul = start.checked_add(payload_len)?;
    let footer_end = nul.checked_add(3)?;
    let footer = buffer.get(nul + 1..footer_end)?;
    let total = usize::from(u16::from_le_bytes([footer[0], footer[1]]));
    if total != payload_len + 3 {
        return None;
    }
    Some((buffer.get(start..nul)?, footer_end))
}
fn text_pos_is_valid(buffer: &[u8], position: i32, force_newline: bool) -> bool {
    if !(0..40_000).contains(&position) {
        return true;
    }
    let byte = history_byte(buffer, position);
    let control = history_byte(buffer, position + 1);
    if byte == b'\\' && control == b'w' {
        return true;
    }
    if force_newline || position < 5 || byte != b'\\' || control != b'n' {
        return false;
    }
    history_byte(buffer, position - 5) == b'\\'
        && matches!(history_byte(buffer, position - 4), b'p' | b'n')
}
fn arg_from_top(args: &[Value], count: usize, n: usize) -> Option<&Value> {
    let real = count.min(args.len());
    (n < real).then(|| &args[real - 1 - n])
}
fn arg_int_from_top(args: &[Value], count: usize, n: usize) -> i32 {
    arg_from_top(args, count, n).and_then(Value::as_int).unwrap_or(0)
}

