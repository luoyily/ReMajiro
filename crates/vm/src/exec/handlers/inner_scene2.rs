use crate::exec::{CtxPhase, WaitDeadline};
use crate::host::Host;
use crate::value::{Scope, Value};
use super::inner::InnerOutcome;
pub(super) const HASHES: &[u32] = &[
    0x38721A26, 0x32530606, 0x3704C919, 0xCF35F0E3, 0xAB13BE44, 0x4E1EB9D0, 0x461EA516,
    0x29098EC0, 0x8C283F7F, 0xE06200C4, 0xF9A3B675, 0x6E83677A, 0x4B5AC64B, 0x619DE833,
    0xE5C70196, 0xEE154520, 0x6584F13E, 0x62374297, 0x80177286, 0x806261CE, 0x4FFAB7C4,
    0xC5270429, 0x1EC3C6BB, 0xF8D8925B, 0x3EABF498, 0xC1D2B85A, 0x6501CC30, 0x072E738C,
    0x6EAEC2A0, 0x0C7A0E97, 0x86E933D1, 0x0EBC053B, 0x818A4B92, 0xCA30043A, 0xEB5CC468,
    0x99A5DE25, 0xEE6B328D, 0xACCF4004, 0xDF9F7749, 0x6E6C641A, 0x3E11F2C5, 0x0E42730D,
    0x934D927B, 0x97BD691D, 0x13C305DA, 0x0542E1B7, 0xF65AA4C0, 0x5C9761C5, 0x125CFBDB,
    0x6ED38886, 0xD2219A75, 0xCD111B0E, 0xDDD25909, 0xDAA321E8, 0x68BA0964, 0x6FCB7185,
    0xD8279A9B, 0x8D8FB32E, 0xDF644F85, 0x098399F2, 0x44BC7555, 0x5F7FFD3B, 0x60085FA4,
    0x95FF28F2, 0xDD2056F6, 0x221C2CC2, 0x42D7C922, 0x4D849AA6, 0xC438BA98, 0xDACA10AF,
    0xE3605CFA, 0x9A1ACECA, 0xCD290AEF, 0x4D2D4277, 0x4B306C5C, 0x81CE0485, 0x8C7F4C1E,
    0x9EF1DDC3, 0xBC42CA9A, 0xC102BCC3, 0xC68C011A, 0xF3D276F1, 0xFAC4F361,
];
impl crate::exec::Vm {
    pub(super) fn handle_inner_scene2<H: Host>(
        &mut self,
        hash: u32,
        count: usize,
        _has_retval: bool,
        host: &mut H,
        args: &[Value],
    ) -> Option<Result<InnerOutcome, crate::exec::VmError>> {
        match hash {
            0x806261CE => {
                let scale = args
                    .get(count.min(args.len()).saturating_sub(1))
                    .and_then(Value::as_float)
                    .unwrap_or(1.0);
                host.set_native_time_scale(scale);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x4D2D4277 => {
                set_native_mode(self, host, 0);
                let message = format_native_values(args, count);
                host.show_message_ok(&message);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x4B306C5C | 0x9EF1DDC3 | 0xFAC4F361 => {
                let mode = match hash {
                    0x4B306C5C => 4,
                    0x9EF1DDC3 => 1,
                    _ => 3,
                };
                set_native_mode(self, host, mode);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xBC42CA9A => {
                self.native_mode_bits = top_int(args) as u8 & 0b111;
                host.set_native_mode_flags(
                    self.native_mode_bits,
                    self.native_mode_special,
                );
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x81CE0485 => {
                let was_zero = self.native_mode_counter == 0;
                self.native_mode_counter = self.native_mode_counter.wrapping_add(1);
                self.stack.push(Value::int(was_zero as i32));
                Some(Ok(InnerOutcome::Normal))
            }
            0x8C7F4C1E => {
                self.native_mode_counter = 0;
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xF3D276F1 => {
                let was_special = self.native_mode_special;
                if was_special {
                    set_native_mode(self, host, 7);
                }
                self.stack.push(Value::int(was_special as i32));
                Some(Ok(InnerOutcome::Normal))
            }
            0xC102BCC3 => {
                if host.native_mode_pending() {
                    host.trigger_middle_input_pulse();
                    set_native_mode(self, host, 6);
                }
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xC68C011A => {
                host.refresh_platform_menu();
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x221C2CC2 => {
                host.movie_cleanup_present();
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x42D7C922 => {
                host.set_file_drop_accept(top_int(args) != 0);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x4D849AA6 => {
                host.save_set_dialog_available(true);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xC438BA98 => {
                host.save_finish_command();
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xDACA10AF => {
                let suffix = args.last().and_then(Value::as_str_bytes).unwrap_or(&[]);
                host.open_http(suffix);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xE3605CFA => {
                host.movie_apply_config(top_int(args));
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xDD2056F6 => {
                let requested = args
                    .last()
                    .and_then(Value::as_str_bytes)
                    .map(native_mjo_basename)
                    .unwrap_or_default();
                self.stack.push(Value::int(self.has_readmark_script(&requested) as i32));
                Some(Ok(InnerOutcome::Normal))
            }
            0x95FF28F2 => {
                let current = args
                    .last()
                    .and_then(Value::as_str_bytes)
                    .map(cstr_bytes)
                    .unwrap_or(&[]);
                let mode = args
                    .get(count.saturating_sub(2))
                    .and_then(Value::as_int)
                    .unwrap_or(0);
                let selected = host.select_font_face(current, mode);
                host.set_selected_font_face(&selected, mode);
                self.stack.push(Value::string(selected));
                Some(Ok(InnerOutcome::Normal))
            }
            0x5F7FFD3B => {
                let message = args
                    .last()
                    .and_then(Value::as_str_bytes)
                    .map(cstr_bytes)
                    .unwrap_or(&[]);
                self.stack.push(Value::int(host.confirm_yes_no(message) as i32));
                Some(Ok(InnerOutcome::Normal))
            }
            0x60085FA4 => {
                let was_suppressed = self.input_suppressed
                    && !host.input_suppression_bypassed();
                let result = if was_suppressed { 0 } else { -1 };
                self.input_suppressed = true;
                self.stack.push(Value::int(result));
                Some(Ok(InnerOutcome::Normal))
            }
            0x44BC7555 => {
                self.input_suppressed = false;
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x4B5AC64B => {
                self.stack.push(Value::int(host.config_load(b"LogNoPic", 1)));
                Some(Ok(InnerOutcome::Normal))
            }
            0x619DE833 => {
                self.stack.push(Value::int(host.config_load(b"ReprotNoVoice", 0)));
                Some(Ok(InnerOutcome::Normal))
            }
            0xE5C70196 => {
                let speed = host.config_load(b"MojiSpeed", 1000);
                crate::text_trace!("[SPEED] get MojiSpeed -> {speed}");
                self.stack.push(Value::int(speed));
                Some(Ok(InnerOutcome::Normal))
            }
            0xEE154520 => {
                self.stack.push(Value::int(host.config_load(b"AutoSpeed", 1000)));
                Some(Ok(InnerOutcome::Normal))
            }
            0x80177286 => {
                let name = args
                    .last()
                    .and_then(Value::as_str_bytes)
                    .map(cstr_bytes)
                    .unwrap_or(&[]);
                let default = if count >= 2 {
                    args.get(count - 2).and_then(Value::as_int).unwrap_or(0)
                } else {
                    0
                };
                let value = host.config_load(name, default);
                self.stack.push(Value::int(value));
                Some(Ok(InnerOutcome::Normal))
            }
            0x62374297 => {
                if count >= 1 {
                    host.set_fullscreen(top_int(args) != 0);
                }
                self.stack.push(Value::int(i32::from(host.is_fullscreen())));
                Some(Ok(InnerOutcome::Normal))
            }
            0x4FFAB7C4 => {
                self.stack.push(Value::int(host.save_dialog_available() as i32));
                Some(Ok(InnerOutcome::Normal))
            }
            0xC5270429 => {
                let mut face = host.selected_font_face();
                if face.is_empty() {
                    let mirrored = self
                        .read_var(crate::value::Scope::Local, 0x296D_AD73, usize::MAX)
                        .and_then(|value| value.as_str_bytes().map(<[u8]>::to_vec))
                        .unwrap_or_default();
                    if !mirrored.is_empty() {
                        let mode = self
                            .read_var(
                                crate::value::Scope::Local,
                                0xBDD6_7FCB,
                                usize::MAX,
                            )
                            .and_then(|value| value.as_int())
                            .unwrap_or(0);
                        host.set_selected_font_face(&mirrored, mode);
                        face = mirrored;
                    }
                }
                crate::text_trace!(
                    "[VM] SELFONT_QUERY face={:?}", String::from_utf8_lossy(& face)
                );
                self.stack.push(Value::string(face));
                Some(Ok(InnerOutcome::Normal))
            }
            0x1EC3C6BB => {
                let face = args
                    .last()
                    .and_then(Value::as_str_bytes)
                    .map(cstr_bytes)
                    .unwrap_or(&[]);
                let mode = args
                    .get(count.saturating_sub(2))
                    .and_then(Value::as_int)
                    .unwrap_or(0);
                host.set_selected_font_face(face, mode);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xF8D8925B => {
                let status = host.music_get_status();
                crate::text_trace!("[AUDIO] MUSIC_GET_STATUS -> {}", status);
                self.stack.push(Value::int(status));
                Some(Ok(InnerOutcome::Normal))
            }
            0x3EABF498 => {
                let value = top_int(args);
                host.config_store(b"FastModeFlg_AR", value);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xC1D2B85A => {
                let value = top_int(args);
                host.config_store(b"FastModeFlg_AR", value);
                self.stack.push(Value::int(value));
                Some(Ok(InnerOutcome::Normal))
            }
            0x6501CC30 => {
                let value = top_int(args);
                let previous = host.config_load(b"MojiSpeed", 1000);
                crate::text_trace!("[SPEED] MojiSpeed {previous} -> {value}");
                host.config_store(b"MojiSpeed", value);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x072E738C => {
                let value = top_int(args);
                host.set_sound_master_volume(value);
                host.config_store(b"SoundVolume", value);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x6EAEC2A0 => {
                let value = top_int(args);
                host.set_voice_master_volume(value);
                host.config_store(b"VoiceVolume", value);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x0C7A0E97 => {
                let value = top_int(args);
                host.set_music_master_volume(value);
                host.config_store(b"MusicVolume", value);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x9A1ACECA => {
                let real = count.min(args.len());
                let name = real
                    .checked_sub(1)
                    .and_then(|index| args.get(index))
                    .and_then(Value::as_str_bytes)
                    .unwrap_or(&[]);
                let value = real
                    .checked_sub(2)
                    .and_then(|index| args.get(index))
                    .and_then(Value::as_int)
                    .unwrap_or(0);
                host.config_store(name, value);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xCD290AEF => {
                let real = count.min(args.len());
                let mut name = real
                    .checked_sub(1)
                    .and_then(|index| args.get(index))
                    .and_then(Value::as_str_bytes)
                    .unwrap_or(&[])
                    .to_vec();
                if !name.iter().skip(1).any(|byte| *byte == b'@') {
                    name.extend_from_slice(b"@GLOBAL");
                }
                let scope = match name.first().copied() {
                    Some(b'#') => Some(Scope::Global),
                    Some(b'@') => Some(Scope::Local),
                    Some(b'%') => Some(Scope::Thread),
                    _ => None,
                };
                if let Some(scope) = scope {
                    let key = crate::formats_crc32(&name);
                    let mut value = real
                        .checked_sub(2)
                        .and_then(|index| args.get(index))
                        .cloned()
                        .unwrap_or_else(Value::null);
                    value.scope = scope;
                    self.write_var(scope, key, 0, value);
                }
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x86E933D1 => {
                let previous = host.take_display_mode();
                self.stack.push(Value::int(previous));
                Some(Ok(InnerOutcome::Normal))
            }
            0x0EBC053B => {
                host.set_input_dispatch_mode(top_int(args));
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x818A4B92 => {
                set_native_mode(self, host, 0);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xCA30043A => {
                let mode = top_int(args);
                self.stack.push(Value::int(host.display_sync(mode)));
                Some(Ok(InnerOutcome::Normal))
            }
            0xEB5CC468 => {
                let raw = args
                    .last()
                    .and_then(Value::as_str_bytes)
                    .map(cstr_bytes)
                    .unwrap_or(&[]);
                let normalized = normalize_picture_name(raw);
                let inserted = host
                    .register_picture_hash(crate::formats_crc32(&normalized));
                self.stack.push(Value::int(if inserted { 0 } else { -1 }));
                Some(Ok(InnerOutcome::Normal))
            }
            0x99A5DE25 => {
                let raw = args
                    .last()
                    .and_then(Value::as_str_bytes)
                    .map(cstr_bytes)
                    .unwrap_or(&[]);
                let normalized = normalize_picture_name(raw);
                let registered = host
                    .picture_hash_registered(crate::formats_crc32(&normalized));
                self.stack.push(Value::int(registered as i32));
                Some(Ok(InnerOutcome::Normal))
            }
            0x38721A26 => {
                self.stack.push(Value::int(self.native_mode_special as i32));
                Some(Ok(InnerOutcome::Normal))
            }
            0x32530606 => {
                let name = top_str(args);
                host.scene_transition(&name);
                self.request_scene_transition();
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x3704C919 => {
                if count >= 1 && top_int(args) != 0 {
                    self.scene_cycle_preserve_audio = true;
                }
                self.request_scene_transition();
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xCF35F0E3 => {
                if !host.cooperative_timers() {
                    self.stack.push(Value::int(0));
                    return Some(Ok(InnerOutcome::Normal));
                }
                let duration = if count >= 1 { top_int(args) } else { -1 };
                let now = host.get_timestamp();
                let probe_ctx = self.current_function_id();
                let probe_script = self.current_script_idx();
                let probe_ip = self.current_ip();
                let Some(frame) = self.frames.last_mut() else {
                    return Some(Err(crate::exec::VmError::Exit));
                };
                if self.active_phase != CtxPhase::Initial {
                    frame.wait_deadline = WaitDeadline::None;
                    self.scheduler_no_present = false;
                    if crate::frame_probe_enabled() {
                        crate::text_trace!(
                            "[FRAME-PROBE] CF35_PHASE_BYPASS ctx=0x{probe_ctx:08X} script={probe_script:?} ip={probe_ip:?} duration={duration} now={now}"
                        );
                    }
                    self.stack.push(Value::int(0));
                    return Some(Ok(InnerOutcome::Normal));
                }
                match frame.wait_deadline {
                    WaitDeadline::At(deadline) if now < deadline => {
                        Some(Ok(InnerOutcome::Waiting))
                    }
                    WaitDeadline::At(deadline) => {
                        frame.wait_deadline = WaitDeadline::None;
                        self.scheduler_no_present = false;
                        if crate::frame_probe_enabled() {
                            crate::text_trace!(
                                "[FRAME-PROBE] CF35_DONE ctx=0x{probe_ctx:08X} script={probe_script:?} ip={probe_ip:?} duration={duration} deadline={deadline} now={now} no_present=false"
                            );
                        }
                        self.stack.push(Value::int(0));
                        Some(Ok(InnerOutcome::Normal))
                    }
                    WaitDeadline::Never => Some(Ok(InnerOutcome::Waiting)),
                    WaitDeadline::None => {
                        if duration == -2 {
                            let deadline = now.saturating_sub(10);
                            frame.wait_deadline = WaitDeadline::At(deadline);
                            self.scheduler_no_present = true;
                            if crate::frame_probe_enabled() {
                                crate::text_trace!(
                                    "[FRAME-PROBE] CF35_NO_PRESENT ctx=0x{probe_ctx:08X} script={probe_script:?} ip={probe_ip:?} deadline={deadline} now={now}"
                                );
                            }
                            Some(Ok(InnerOutcome::YieldNoPresent))
                        } else {
                            let deadline = if duration >= 0 {
                                WaitDeadline::At(now.saturating_add(duration))
                            } else {
                                WaitDeadline::Never
                            };
                            frame.wait_deadline = deadline;
                            if crate::frame_probe_enabled() {
                                crate::text_trace!(
                                    "[FRAME-PROBE] CF35_ARM ctx=0x{probe_ctx:08X} script={probe_script:?} ip={probe_ip:?} duration={duration} deadline={deadline:?} now={now}"
                                );
                            }
                            Some(Ok(InnerOutcome::Waiting))
                        }
                    }
                }
            }
            0x8D8FB32E => {
                host.scene_unload_and_exit();
                self.quit_requested = true;
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x4E1EB9D0 => {
                host.script_reset(self.active_function_id, self.frames.len());
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x461EA516 => {
                let entry_hash = top_int(args) as u32;
                let target = self.resolve_entry_global(entry_hash);
                if target.is_none() {
                    return Some(
                        Err(crate::exec::VmError::UnknownEntryHash {
                            hash: entry_hash,
                            from_script: self
                                .frames
                                .last()
                                .map(|frame| frame.script_idx)
                                .unwrap_or(0),
                            ip: self.current_ip().unwrap_or(0),
                        }),
                    );
                }
                self.set_context_finalizer(target);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xAB13BE44 => {
                let mode = top_int(args);
                let was_visible = host.cursor_show_or_hide(mode != 0);
                self.stack.push(Value::int(if was_visible { 1 } else { 0 }));
                Some(Ok(InnerOutcome::Normal))
            }
            0x125CFBDB => {
                let desired = top_int(args) != 0;
                host.set_capslock_state(desired);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x6584F13E => {
                self.stack.push(Value::int(host.get_timestamp()));
                Some(Ok(InnerOutcome::Normal))
            }
            0xEE6B328D => {
                let (start_ts, duration, end) = three_ints(args);
                let now = host.get_timestamp();
                let elapsed = (now - start_ts).max(0);
                let value = if duration > 0 {
                    let clamped = elapsed.min(duration);
                    (clamped as i64 * end as i64 / duration as i64) as i32
                } else {
                    end
                };
                host.timer_lerp(end, start_ts, duration, value);
                self.stack.push(Value::int(value));
                Some(Ok(InnerOutcome::Normal))
            }
            0xD8279A9B => {
                let now = host.get_timestamp();
                let Some(frame) = self.frames.last_mut() else {
                    return Some(Err(crate::exec::VmError::Exit));
                };
                if let WaitDeadline::At(deadline) = frame.wait_deadline {
                    let wrapped_distance = now.wrapping_sub(deadline).unsigned_abs();
                    if now < deadline && wrapped_distance <= 0x0293_2E00 {
                        return Some(Ok(InnerOutcome::Waiting));
                    }
                }
                let playing = if self.native_effect_skip_active(host) {
                    host.native_music_stop();
                    false
                } else {
                    host.music_get_status() == 1
                };
                let Some(frame) = self.frames.last_mut() else {
                    return Some(Err(crate::exec::VmError::Exit));
                };
                if playing {
                    frame.wait_deadline = WaitDeadline::At(now.wrapping_add(10));
                    Some(Ok(InnerOutcome::Waiting))
                } else {
                    frame.wait_deadline = WaitDeadline::None;
                    self.stack.push(Value::int(0));
                    Some(Ok(InnerOutcome::Normal))
                }
            }
            0x29098EC0 => {
                let w = args
                    .get(count.saturating_sub(1))
                    .and_then(|v| v.as_int())
                    .unwrap_or(0);
                let h = args
                    .get(count.saturating_sub(2))
                    .and_then(|v| v.as_int())
                    .unwrap_or(0);
                let scale_x = if count >= 3 {
                    args.get(count.saturating_sub(3))
                        .and_then(|v| v.as_int())
                        .unwrap_or(1000)
                } else {
                    1000
                };
                let scale_y = if count >= 4 {
                    args.get(count.saturating_sub(4))
                        .and_then(|v| v.as_int())
                        .unwrap_or(1000)
                } else {
                    1000
                };
                host.set_render_mode(w, h, scale_x, scale_y);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x8C283F7F => {
                self.stack.push(Value::int(host.get_render_height()));
                Some(Ok(InnerOutcome::Normal))
            }
            0xE06200C4 => {
                self.stack.push(Value::int(host.get_render_width()));
                Some(Ok(InnerOutcome::Normal))
            }
            0x3E11F2C5 => {
                self.stack.push(Value::int(host.get_render_page()));
                Some(Ok(InnerOutcome::Normal))
            }
            0x0E42730D => {
                let value = host.get_text_pos_x();
                eprintln!("[PAUSE-DIAG] text_pos_x={value}");
                self.stack.push(Value::int(value));
                Some(Ok(InnerOutcome::Normal))
            }
            0x934D927B => {
                let value = host.get_text_pos_y();
                eprintln!("[PAUSE-DIAG] text_pos_y={value}");
                self.stack.push(Value::int(value));
                Some(Ok(InnerOutcome::Normal))
            }
            0x97BD691D => {
                self.stack
                    .push(Value::int(if host.get_voice_enable_flag() { 1 } else { 0 }));
                Some(Ok(InnerOutcome::Normal))
            }
            0xF65AA4C0 => {
                self.stack.push(Value::int(host.get_global_d()));
                Some(Ok(InnerOutcome::Normal))
            }
            0x13C305DA => {
                self.stack.push(Value::int(host.get_global_f()));
                Some(Ok(InnerOutcome::Normal))
            }
            0x0542E1B7 => {
                host.set_text_redraw_flag();
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xACCF4004 => {
                host.set_voice_ts_flag();
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xDF9F7749 => {
                host.set_voice_flag();
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x6E6C641A => {
                self.stack.push(Value::int(if host.get_error_state() { 1 } else { 0 }));
                Some(Ok(InnerOutcome::Normal))
            }
            0x5C9761C5 => {
                self.stack.push(Value::int(if host.check_skip_flag() { 1 } else { 0 }));
                Some(Ok(InnerOutcome::Normal))
            }
            0xF9A3B675 | 0x6E83677A => {
                let raw_name = args
                    .last()
                    .and_then(Value::as_str_bytes)
                    .map(cstr_bytes)
                    .unwrap_or(&[]);
                let name = normalize_global_name(raw_name);
                let hash = crate::formats_crc32(&name);
                let value = self
                    .global_keys
                    .get(&hash)
                    .and_then(|&slot| self.globals.get(slot))
                    .and_then(Value::as_int)
                    .unwrap_or(0);
                self.stack.push(Value::int(value));
                Some(Ok(InnerOutcome::Normal))
            }
            0xCD111B0E => {
                let name = top_str(args);
                host.config_name_stash(&name);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x6ED38886 => {
                let v = top_int(args);
                crate::text_trace!("[SPEED] AutoSpeed -> {v}");
                host.config_set_autospeed(v);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xD2219A75 => {
                let write = count != 0;
                let new_val = if write { Some(top_int(args)) } else { None };
                let old = host.config_getset_scale(new_val);
                self.stack.push(Value::int(old));
                Some(Ok(InnerOutcome::Normal))
            }
            0xDDD25909 => {
                self.stack.push(Value::int(host.config_get_quicksave_failsafe()));
                Some(Ok(InnerOutcome::Normal))
            }
            0xDAA321E8 => {
                let v = top_int(args);
                host.config_set_quicksave_failsafe(v);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x68BA0964 => {
                self.stack.push(Value::int(host.config_get_quickload_failsafe()));
                Some(Ok(InnerOutcome::Normal))
            }
            0x6FCB7185 => {
                let v = top_int(args);
                host.config_set_quickload_failsafe(v);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x098399F2 => {
                let name = top_str(args);
                let (_, h, w) = three_ints(args);
                eprintln!(
                    "[FONT] cache_load {:?} {}x{} (trace: empty default)", name, w, h
                );
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xDF644F85 => {
                let msg = top_str(args);
                eprintln!("[SYS] msgbox_error (suppressed): {:?}", msg);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            _ => None,
        }
    }
}
fn top_int(args: &[Value]) -> i32 {
    args.last().and_then(|v| v.as_int()).unwrap_or(0)
}
fn top_str(args: &[Value]) -> String {
    args.last()
        .and_then(|v| v.as_str_bytes())
        .map(|b| {
            let mut v = b.to_vec();
            while v.last() == Some(&0) {
                v.pop();
            }
            String::from_utf8_lossy(&v).into_owned()
        })
        .unwrap_or_default()
}
fn cstr_bytes(bytes: &[u8]) -> &[u8] {
    bytes.split(|&byte| byte == 0).next().unwrap_or(bytes)
}
fn native_mjo_basename(bytes: &[u8]) -> Vec<u8> {
    let bytes = cstr_bytes(bytes);
    let mut last_separator = None;
    let mut last_dot = None;
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if is_sjis_lead(byte) && index + 1 < bytes.len() {
            index += 2;
            continue;
        }
        match byte {
            b'\\' | b'/' => {
                last_separator = Some(index);
                last_dot = None;
            }
            b'.' => last_dot = Some(index),
            _ => {}
        }
        index += 1;
    }
    let start = last_separator.map_or(0, |position| position + 1);
    let end = last_dot.filter(|position| *position >= start).unwrap_or(bytes.len());
    let mut output = Vec::with_capacity(end.saturating_sub(start) + 4);
    let mut index = start;
    while index < end {
        let byte = bytes[index];
        if is_sjis_lead(byte) && index + 1 < end {
            output.extend_from_slice(&bytes[index..index + 2]);
            index += 2;
        } else {
            output.push(byte.to_ascii_uppercase());
            index += 1;
        }
    }
    output.extend_from_slice(b".MJO");
    output
}
fn normalize_picture_name(bytes: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if is_sjis_lead(byte) && index + 1 < bytes.len() {
            output.extend_from_slice(&bytes[index..index + 2]);
            index += 2;
        } else {
            if byte == b'.' {
                break;
            }
            output.push(byte.to_ascii_lowercase());
            index += 1;
        }
    }
    output
}
fn normalize_global_name(bytes: &[u8]) -> Vec<u8> {
    let mut has_scope = false;
    let mut index = usize::from(!bytes.is_empty());
    while index < bytes.len() {
        let byte = bytes[index];
        if is_sjis_lead(byte) && index + 1 < bytes.len() {
            index += 2;
            continue;
        }
        if byte == b'@' {
            has_scope = true;
            break;
        }
        index += 1;
    }
    let mut output = bytes.to_vec();
    if !has_scope {
        output.extend_from_slice(b"@GLOBAL");
    }
    output
}
fn is_sjis_lead(byte: u8) -> bool {
    (0x81..=0x9f).contains(&byte) || (0xe0..=0xfc).contains(&byte)
}
fn three_ints(args: &[Value]) -> (i32, i32, i32) {
    let n = args.len();
    let a = n
        .checked_sub(1)
        .and_then(|i| args.get(i))
        .and_then(|v| v.as_int())
        .unwrap_or(0);
    let b = n
        .checked_sub(2)
        .and_then(|i| args.get(i))
        .and_then(|v| v.as_int())
        .unwrap_or(0);
    let c = n
        .checked_sub(3)
        .and_then(|i| args.get(i))
        .and_then(|v| v.as_int())
        .unwrap_or(0);
    (a, b, c)
}
fn format_native_values(args: &[Value], count: usize) -> Vec<u8> {
    let mut output = Vec::new();
    for value in args[..count.min(args.len())].iter().rev() {
        match value.type_tag {
            crate::value::TAG_INT => {
                output
                    .extend_from_slice(
                        format!("{} ", value.as_int().unwrap_or(0)).as_bytes(),
                    );
            }
            crate::value::TAG_FLOAT => {
                output
                    .extend_from_slice(
                        format!("{:.6} ", value.as_float().unwrap_or(0.0)).as_bytes(),
                    );
            }
            crate::value::TAG_STRING => {
                let bytes = value.as_str_bytes().unwrap_or(&[]);
                output.extend_from_slice(cstr_bytes(bytes));
                output.push(b' ');
            }
            crate::value::TAG_INT_ARRAY => output.extend_from_slice(b"[INT_ARRAY] "),
            crate::value::TAG_FIXED_INT_ARRAY => {
                output.extend_from_slice(b"[FIXED_ARRAY] ")
            }
            crate::value::TAG_STRING_ARRAY => {
                output.extend_from_slice(b"[STRING_ARRAY] ")
            }
            _ => {}
        }
    }
    output
}
fn set_native_mode<H: Host>(vm: &mut crate::exec::Vm, host: &mut H, mode: i32) {
    vm.native_mode_special = false;
    vm.native_mode_bits = match mode {
        0 | 7 => 0,
        1 => if host.config_load(b"FastModeFlg_AR", 0) != 0 { 0b010 } else { 0b001 }
        2 => 0b010,
        3 => 0b100,
        4 => 0b001,
        6 => {
            vm.native_mode_special = true;
            0b001
        }
        _ => vm.native_mode_bits,
    };
    host.set_native_mode_flags(vm.native_mode_bits, vm.native_mode_special);
}

