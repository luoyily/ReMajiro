use crate::exec::WaitDeadline;
use crate::host::Host;
use crate::value::Value;
use super::helpers::InnerArgs;
use super::inner::InnerOutcome;
pub(super) const HASHES: &[u32] = &[
    0x05D6ED69, 0x0BEF00DB, 0x0C070535, 0x0DC634C4, 0x19FC7CC7, 0x15EEDEAA, 0x379FDB39,
    0x173B92D1, 0x46B28FEF, 0xDEAF7041, 0xABB7FCFB, 0x844D2028, 0x91CEACF2, 0xEEAA9104,
    0x201D3D29, 0x7F918815, 0x777AA894, 0x2626FE02, 0x26DC6566, 0xB669E371, 0x3D6CF5BE,
    0x1D93FD7E, 0x2E861C51, 0xE799EAC8, 0xB8155FF4, 0x170B0B45, 0xA0B0FEAD, 0x804FF66D,
    0x39AE7891, 0x08CB5C29, 0x5747E915, 0xC0E98B1D, 0x2B10081B, 0x4311CFA6, 0x45AD5F4D,
    0x4604B9AF, 0x94EAEFFC, 0xB2F1993B, 0x4980F82C, 0x4F6367B3, 0x5785A054, 0x57AD7635,
    0x546F69C3, 0x5C7746D1, 0x5A981E41, 0x708B0256, 0x7B657BAB, 0x83A53FFA, 0x8424CF12,
    0x8E8167F2, 0x8EB881EF, 0x8F905B3F, 0x90D5298A, 0x926C13E5, 0x9CABFDF3, 0xA65B03AE,
    0xAE47892F, 0xCB665AC0, 0xD1F672C7, 0xDFCF9F75, 0xF1097A07, 0xF62E3CA7, 0xF9D7B692,
    0x801D7AF9, 0x838DE99B, 0x884949E0, 0x8AD70056, 0x5BF8F175, 0x5C4020E6, 0xE712FEC1,
    0xE0AA2F52, 0xF03A9A01, 0xF7824B92, 0xC9B362D0, 0xD011FF32, 0xC0C15D7C, 0xDE1E2ED0,
    0xE9E5564D, 0xDF91CFC5, 0xE7B90DF5, 0xED84E320, 0xA589DBD1, 0xB268F04F, 0xF86FC950,
    0x95E3A441, 0x925B75D2, 0x77527EF5,
];
impl crate::exec::Vm {
    pub(super) fn handle_inner_audio<H: Host>(
        &mut self,
        hash: u32,
        count: usize,
        _has_retval: bool,
        host: &mut H,
        args: &[Value],
    ) -> Option<Result<InnerOutcome, crate::exec::VmError>> {
        match hash {
            0x0DC634C4 => {
                let primary = top_str(args, count, 0);
                let secondary = top_str(args, count, 1);
                if primary.is_empty() {
                    host.native_music_stop();
                } else {
                    host.native_music_play_dual(primary, secondary);
                }
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x0C070535 => {
                host.native_music_stop();
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x15EEDEAA => {
                let duration = if self.native_effect_skip_active(host) {
                    0
                } else {
                    top_int(args, count, 0).unwrap_or(0)
                };
                host.native_music_fadeout(duration);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x379FDB39 => {
                let now = host.get_timestamp();
                let Some(frame) = self.frames.last_mut() else {
                    return Some(Err(crate::exec::VmError::Exit));
                };
                if let WaitDeadline::At(deadline) = frame.wait_deadline {
                    let wrapped_distance = now.wrapping_sub(deadline).unsigned_abs();
                    if now < deadline && wrapped_distance <= 0x0293_2E00 {
                        return Some(Ok(InnerOutcome::Waiting));
                    }
                    frame.wait_deadline = WaitDeadline::None;
                    self.stack.push(Value::int(0));
                    return Some(Ok(InnerOutcome::Normal));
                }
                let duration = if self.native_effect_skip_active(host) {
                    0
                } else {
                    top_int(args, count, 0).unwrap_or(0)
                };
                host.native_music_fadeout(duration);
                if duration > 0 {
                    let Some(frame) = self.frames.last_mut() else {
                        return Some(Err(crate::exec::VmError::Exit));
                    };
                    frame.wait_deadline = WaitDeadline::At(now.wrapping_add(duration));
                    Some(Ok(InnerOutcome::Waiting))
                } else {
                    self.stack.push(Value::int(0));
                    Some(Ok(InnerOutcome::Normal))
                }
            }
            0x4F6367B3 => {
                let target_volume = top_int(args, count, 0).unwrap_or(0);
                let duration = top_int(args, count, 1).unwrap_or(0);
                host.native_music_fade(target_volume, duration);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x2626FE02 => {
                host.native_music_pause();
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xD011FF32 => {
                host.native_music_resume();
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xABB7FCFB => {
                let primary = top_str(args, count, 0);
                let loop_ = count < 2 || top_int(args, count, 1).unwrap_or(0) != 0;
                host.native_music_replace(primary, &[], loop_);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x844D2028 => {
                let primary = top_str(args, count, 0);
                let secondary = top_str(args, count, 1);
                host.native_music_replace(primary, secondary, true);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x91CEACF2 => {
                host.native_music_clear_end_state();
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xEEAA9104 => {
                host.native_music_set_frequency(top_int(args, count, 0).unwrap_or(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xA589DBD1 => {
                host.native_music_snapshot();
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xF86FC950 => {
                host.native_music_restore();
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x05D6ED69 => {
                host.native_music_aux_stop();
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x801D7AF9 => {
                host.native_music_aux_set_volume(top_int(args, count, 0).unwrap_or(0));
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xDF91CFC5 => {
                host.native_music_aux_set_pan(top_int(args, count, 0).unwrap_or(0));
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x46B28FEF => {
                let target_volume = top_int(args, count, 0).unwrap_or(0);
                let duration = top_int(args, count, 1).unwrap_or(0);
                host.native_music_aux_fade(target_volume, duration);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x173B92D1 => {
                let duration = if self.native_effect_skip_active(host) {
                    0
                } else {
                    top_int(args, count, 0).unwrap_or(0)
                };
                host.native_music_aux_fadeout(duration);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xDEAF7041 => {
                self.stack.push(Value::string(host.native_music_aux_filename()));
                Some(Ok(InnerOutcome::Normal))
            }
            0xE7B90DF5 => {
                host.native_music_set_volume(top_int(args, count, 0).unwrap_or(0));
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xB268F04F => {
                let primary = top_str(args, count, 0);
                let secondary = top_str(args, count, 1);
                host.native_sound_a_play_dual(primary, secondary);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x201D3D29 => {
                host.native_sound_a_set_volume(top_int(args, count, 0).unwrap_or(0));
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x7F918815 => {
                host.native_sound_a_set_pan(top_int(args, count, 0).unwrap_or(0));
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x777AA894 => {
                self.stack.push(Value::int(host.native_sound_a_status()));
                Some(Ok(InnerOutcome::Normal))
            }
            0x926C13E5 => {
                let duration = if self.native_effect_skip_active(host) {
                    0
                } else {
                    top_int(args, count, 0).unwrap_or(0)
                };
                host.native_sound_a_fadeout(duration);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xC0C15D7C => {
                let target_volume = top_int(args, count, 0).unwrap_or(0);
                let duration = top_int(args, count, 1).unwrap_or(0);
                host.native_sound_a_fade(target_volume, duration);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x5785A054 => {
                let now = host.get_timestamp();
                if let Some(outcome) = pending_audio_wait(self, now) {
                    return Some(outcome);
                }
                let playing = if self.native_effect_skip_active(host) {
                    host.native_sound_a_stop();
                    false
                } else {
                    host.native_sound_a_status() == 1
                };
                finish_audio_wait(self, now, playing)
            }
            0x708B0256 => {
                let primary = top_str(args, count, 0);
                let loop_ = count >= 2 && top_int(args, count, 1).unwrap_or(0) != 0;
                host.native_sound_b_play(primary, loop_);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xE799EAC8 => {
                host.native_sound_b_set_volume(top_int(args, count, 0).unwrap_or(0));
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xB8155FF4 => {
                host.native_sound_b_set_pan(top_int(args, count, 0).unwrap_or(0));
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x170B0B45 => {
                let target_volume = top_int(args, count, 0).unwrap_or(0);
                let duration = top_int(args, count, 1).unwrap_or(0);
                host.native_sound_b_fade(target_volume, duration);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x2E861C51 => {
                let duration = if self.native_effect_skip_active(host) {
                    0
                } else {
                    top_int(args, count, 0).unwrap_or(0)
                };
                host.native_sound_b_fadeout(duration);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x546F69C3 => {
                host.native_sound_b_stop();
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xDE1E2ED0 => {
                let primary = top_str(args, count, 0);
                let secondary = top_str(args, count, 1);
                host.native_sound_b_play_dual(primary, secondary);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xA0B0FEAD => {
                self.stack.push(Value::int(host.native_sound_b_status()));
                Some(Ok(InnerOutcome::Normal))
            }
            0x804FF66D => {
                let now = host.get_timestamp();
                if let Some(outcome) = pending_audio_wait(self, now) {
                    return Some(outcome);
                }
                let playing = if self.native_effect_skip_active(host) {
                    host.native_sound_b_stop();
                    false
                } else {
                    host.native_sound_b_status() == 1
                };
                finish_audio_wait(self, now, playing)
            }
            0x4311CFA6 => {
                let primary = top_str(args, count, 0);
                let secondary = top_str(args, count, 1);
                host.native_sound_c_play_dual(primary, secondary);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xED84E320 => {
                let primary = top_str(args, count, 0);
                let loop_ = count >= 2 && top_int(args, count, 1).unwrap_or(0) != 0;
                host.native_sound_c_play(primary, loop_);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x08CB5C29 => {
                host.native_sound_c_set_volume(top_int(args, count, 0).unwrap_or(0));
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x5747E915 => {
                host.native_sound_c_set_pan(top_int(args, count, 0).unwrap_or(0));
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xC0E98B1D => {
                let target_volume = top_int(args, count, 0).unwrap_or(0);
                let duration = top_int(args, count, 1).unwrap_or(0);
                host.native_sound_c_fade(target_volume, duration);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x39AE7891 => {
                let duration = if self.native_effect_skip_active(host) {
                    0
                } else {
                    top_int(args, count, 0).unwrap_or(0)
                };
                host.native_sound_c_fadeout(duration);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x838DE99B => {
                host.native_sound_c_stop();
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x57AD7635 => {
                let now = host.get_timestamp();
                if let Some(outcome) = pending_audio_wait(self, now) {
                    return Some(outcome);
                }
                let playing = if self.native_effect_skip_active(host) {
                    host.native_sound_c_stop();
                    false
                } else {
                    host.native_sound_c_status() == 1
                };
                finish_audio_wait(self, now, playing)
            }
            0x884949E0 => {
                let args = InnerArgs::new(args, count);
                let primary = args.bytes_or_empty(0);
                let loop_ = args.len() >= 2 && args.int_or(1, 0) != 0;
                host.native_sound_d_play(primary, loop_);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xC9B362D0 => {
                host.native_sound_d_stop();
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xE9E5564D => {
                host.native_sound_d_set_volume(top_int(args, count, 0).unwrap_or(0));
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xB669E371 => {
                host.native_sound_d_set_pan(top_int(args, count, 0).unwrap_or(0));
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x3D6CF5BE => {
                self.stack.push(Value::int(host.native_sound_d_status()));
                Some(Ok(InnerOutcome::Normal))
            }
            0x8AD70056 => {
                let target_volume = top_int(args, count, 0).unwrap_or(0);
                let duration = top_int(args, count, 1).unwrap_or(0);
                host.native_sound_d_fade(target_volume, duration);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x5C7746D1 => {
                let duration = if self.native_effect_skip_active(host) {
                    0
                } else {
                    top_int(args, count, 0).unwrap_or(0)
                };
                host.native_sound_d_fadeout(duration);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x26DC6566 => {
                let primary = top_str(args, count, 0);
                let secondary = top_str(args, count, 1);
                host.native_sound_d_play_dual(primary, secondary);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x1D93FD7E => {
                let now = host.get_timestamp();
                if let Some(outcome) = pending_audio_wait(self, now) {
                    return Some(outcome);
                }
                let playing = if self.native_effect_skip_active(host) {
                    host.native_sound_d_stop();
                    false
                } else {
                    host.native_sound_d_status() == 1
                };
                finish_audio_wait(self, now, playing)
            }
            0x5BF8F175 => {
                self.stack.push(Value::string(host.native_sound_a_filename()));
                Some(Ok(InnerOutcome::Normal))
            }
            0x5C4020E6 => {
                self.stack.push(Value::int(host.native_sound_a_flag()));
                Some(Ok(InnerOutcome::Normal))
            }
            0xE712FEC1 => {
                self.stack.push(Value::string(host.native_sound_b_filename()));
                Some(Ok(InnerOutcome::Normal))
            }
            0xE0AA2F52 => {
                self.stack.push(Value::int(host.native_sound_b_flag()));
                Some(Ok(InnerOutcome::Normal))
            }
            0xF03A9A01 => {
                self.stack.push(Value::string(host.native_sound_c_filename()));
                Some(Ok(InnerOutcome::Normal))
            }
            0xF7824B92 => {
                self.stack.push(Value::int(host.native_sound_c_flag()));
                Some(Ok(InnerOutcome::Normal))
            }
            0x77527EF5 => {
                self.stack.push(Value::int(host.native_sound_c_status()));
                Some(Ok(InnerOutcome::Normal))
            }
            0x95E3A441 => {
                self.stack.push(Value::string(host.native_sound_d_filename()));
                Some(Ok(InnerOutcome::Normal))
            }
            0x925B75D2 => {
                self.stack.push(Value::int(host.native_sound_d_flag()));
                Some(Ok(InnerOutcome::Normal))
            }
            0xF62E3CA7 => {
                let filename = top_str(args, count, 0);
                let loop_ = count >= 2 && top_int(args, count, 1).unwrap_or(0) != 0;
                host.main_sound_play(filename, loop_);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x83A53FFA => {
                host.main_sound_stop();
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x8E8167F2 => {
                let filename = top_str(args, count, 1);
                let loop_ = if count >= 3 {
                    top_int(args, count, 2).unwrap_or(0) != 0
                } else {
                    false
                };
                if let Some(ch) = valid_audio_channel(args, count) {
                    host.voice_play(ch, filename, loop_);
                }
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x5A981E41 => {
                if let Some(ch) = valid_audio_channel(args, count) {
                    host.voice_stop(ch);
                }
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xCB665AC0 => {
                if let Some(ch) = valid_audio_channel(args, count) {
                    host.voice_set_vol(ch, top_int(args, count, 1).unwrap_or(0));
                }
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x19FC7CC7 => {
                if let Some(ch) = valid_audio_channel(args, count) {
                    host.voice_fade(
                        ch,
                        top_int(args, count, 1).unwrap_or(0),
                        top_int(args, count, 2).unwrap_or(0),
                    );
                }
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x8F905B3F => {
                if let Some(ch) = valid_audio_channel(args, count) {
                    let duration_ms = if self.native_effect_skip_active(host) {
                        0
                    } else {
                        top_int(args, count, 1).unwrap_or(0)
                    };
                    host.voice_fadeout(ch, duration_ms);
                }
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x8EB881EF => {
                let Some(ch) = valid_audio_channel(args, count) else {
                    if let Some(frame) = self.frames.last_mut() {
                        frame.wait_deadline = WaitDeadline::None;
                    }
                    self.stack.push(Value::int(0));
                    return Some(Ok(InnerOutcome::Normal));
                };
                let now = host.get_timestamp();
                if let Some(outcome) = pending_audio_wait(self, now) {
                    return Some(outcome);
                }
                let playing = if self.native_effect_skip_active(host) {
                    host.voice_stop(ch);
                    false
                } else {
                    host.voice_get_stat(ch) == 1
                };
                finish_audio_wait(self, now, playing)
            }
            0xAE47892F => {
                let status = valid_audio_channel(args, count)
                    .map(|ch| host.voice_get_stat(ch))
                    .unwrap_or(0);
                self.stack.push(Value::int(status));
                Some(Ok(InnerOutcome::Normal))
            }
            0x4604B9AF => {
                let name = valid_audio_channel(args, count)
                    .map(|ch| host.voice_get_filename(ch))
                    .unwrap_or_default();
                self.stack.push(Value::string(name));
                Some(Ok(InnerOutcome::Normal))
            }
            0x94EAEFFC => {
                if let Some(ch) = valid_audio_channel(args, count) {
                    host.voice_set_pan(ch, top_int(args, count, 1).unwrap_or(0));
                }
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x8424CF12 => {
                let filename = top_str(args, count, 1);
                let loop_ = if count >= 3 {
                    top_int(args, count, 2).unwrap_or(0) != 0
                } else {
                    false
                };
                if let Some(ch) = valid_audio_channel(args, count) {
                    host.sound_play(ch, filename, loop_);
                }
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xDFCF9F75 => {
                if let Some(ch) = valid_audio_channel(args, count) {
                    host.sound_stop(ch);
                }
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xF9D7B692 => {
                if let Some(ch) = valid_audio_channel(args, count) {
                    host.sound_set_vol(ch, top_int(args, count, 1).unwrap_or(0));
                }
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xA65B03AE => {
                if let Some(ch) = valid_audio_channel(args, count) {
                    host.sound_set_pan(ch, top_int(args, count, 1).unwrap_or(0));
                }
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x9CABFDF3 => {
                if let Some(ch) = valid_audio_channel(args, count) {
                    host.sound_fade(
                        ch,
                        top_int(args, count, 1).unwrap_or(0),
                        top_int(args, count, 2).unwrap_or(0),
                    );
                }
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x7B657BAB => {
                if let Some(ch) = valid_audio_channel(args, count) {
                    let duration_ms = if self.native_effect_skip_active(host) {
                        0
                    } else {
                        top_int(args, count, 1).unwrap_or(0)
                    };
                    host.sound_fadeout(ch, duration_ms);
                }
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x2B10081B => {
                let status = valid_audio_channel(args, count)
                    .map(|ch| host.sound_get_stat(ch))
                    .unwrap_or(0);
                self.stack.push(Value::int(status));
                Some(Ok(InnerOutcome::Normal))
            }
            0x0BEF00DB => {
                let Some(ch) = valid_audio_channel(args, count) else {
                    if let Some(frame) = self.frames.last_mut() {
                        frame.wait_deadline = WaitDeadline::None;
                    }
                    self.stack.push(Value::int(0));
                    return Some(Ok(InnerOutcome::Normal));
                };
                let now = host.get_timestamp();
                if let Some(outcome) = pending_audio_wait(self, now) {
                    return Some(outcome);
                }
                let playing = if self.native_effect_skip_active(host) {
                    host.sound_stop(ch);
                    false
                } else {
                    host.sound_get_stat(ch) == 1
                };
                finish_audio_wait(self, now, playing)
            }
            0xB2F1993B => {
                let name = valid_audio_channel(args, count)
                    .map(|ch| host.sound_get_filename(ch))
                    .unwrap_or_default();
                self.stack.push(Value::string(name));
                Some(Ok(InnerOutcome::Normal))
            }
            0x45AD5F4D => {
                let name = top_str(args, count, 0);
                let found = host.voice_check_file(name);
                eprintln!(
                    "[AUDIO] voice_check_file {:?} → {}",
                    String::from_utf8_lossy(name), found
                );
                self.stack.push(Value::int(found));
                Some(Ok(InnerOutcome::Normal))
            }
            0x4980F82C => {
                let name = top_str(args, count, 0);
                let loop_ = if count >= 2 {
                    top_int(args, count, 1).map(|v| v != 0).unwrap_or(false)
                } else {
                    true
                };
                host.bgm_play_or_stop(name, loop_);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x90D5298A => {
                let name = top_str(args, count, 0);
                let loop_ = if count >= 2 {
                    top_int(args, count, 1).map(|v| v != 0).unwrap_or(false)
                } else {
                    false
                };
                host.voice_aux_play(name, loop_);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xF1097A07 => {
                self.stack.push(Value::int(host.voice_aux_get_stat()));
                Some(Ok(InnerOutcome::Normal))
            }
            0xD1F672C7 => {
                let now = host.get_timestamp();
                if let Some(outcome) = pending_audio_wait(self, now) {
                    return Some(outcome);
                }
                let playing = if self.native_effect_skip_active(host) {
                    host.native_music_aux_stop();
                    false
                } else {
                    host.voice_aux_get_stat() == 1
                };
                finish_audio_wait(self, now, playing)
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
fn valid_audio_channel(args: &[Value], count: usize) -> Option<u8> {
    let channel = top_int(args, count, 0)?;
    u8::try_from(channel).ok().filter(|channel| *channel < 20)
}
fn pending_audio_wait(
    vm: &mut crate::exec::Vm,
    now: i32,
) -> Option<Result<InnerOutcome, crate::exec::VmError>> {
    let Some(frame) = vm.frames.last_mut() else {
        return Some(Err(crate::exec::VmError::Exit));
    };
    if let WaitDeadline::At(deadline) = frame.wait_deadline {
        let wrapped_distance = now.wrapping_sub(deadline).unsigned_abs();
        if now < deadline && wrapped_distance <= 0x0293_2E00 {
            return Some(Ok(InnerOutcome::Waiting));
        }
    }
    None
}
fn finish_audio_wait(
    vm: &mut crate::exec::Vm,
    now: i32,
    playing: bool,
) -> Option<Result<InnerOutcome, crate::exec::VmError>> {
    let Some(frame) = vm.frames.last_mut() else {
        return Some(Err(crate::exec::VmError::Exit));
    };
    if playing {
        frame.wait_deadline = WaitDeadline::At(now.wrapping_add(10));
        Some(Ok(InnerOutcome::Waiting))
    } else {
        frame.wait_deadline = WaitDeadline::None;
        vm.stack.push(Value::int(0));
        Some(Ok(InnerOutcome::Normal))
    }
}

