use crate::host::Host;
use crate::value::Value;
use super::helpers::InnerArgs;
use super::inner::InnerOutcome;
pub(super) const HASHES: &[u32] = &[
    0x89018DD1, 0x3A3CA0F1, 0x490D6B50, 0x3474D65B, 0x78A6A112, 0x168EFCB7, 0x65075141,
    0xA549E852, 0x4C99B0EA, 0x9143DC9E, 0xE91B0504, 0x89CE8C53, 0x3378635D, 0x7C3B36C5,
    0x1BB47604, 0x44B5C4ED, 0xF8004993, 0x163C0878,
];
impl crate::exec::Vm {
    pub(super) fn handle_inner_save<H: Host>(
        &mut self,
        hash: u32,
        count: usize,
        _has_retval: bool,
        host: &mut H,
        args: &[Value],
    ) -> Option<Result<InnerOutcome, crate::exec::VmError>> {
        let args = InnerArgs::new(args, count);
        match hash {
            0x89018DD1 => {
                let slot = args.int_or(0, 0);
                let ok = if let Some(save) = self.pending_save_snapshot.as_ref() {
                    let system = self.snapshot_system_file();
                    host.save_write_slot(slot as i64, save, &system)
                } else {
                    eprintln!(
                        "[SAVE] write slot={} rejected: no resource_load snapshot", slot
                    );
                    false
                };
                eprintln!("[SAVE] write slot={} ok={}", slot, ok);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x3A3CA0F1 => {
                let slot = args.int_or(0, 0);
                let loaded = host.save_read_slot(slot as i64);
                let ok = loaded.is_some();
                if let Some(save) = loaded {
                    self.queue_save_restore(save);
                }
                eprintln!("[SAVE] read slot={} ok={}", slot, ok);
                self.stack.push(Value::int(if ok { 0 } else { -1 }));
                Some(Ok(InnerOutcome::Normal))
            }
            0x490D6B50 => {
                let dst = args.int_or(0, 0) as i64;
                let src = args.int_or(1, 0) as i64;
                host.save_copy_slot(src, dst);
                eprintln!("[SAVE] copy slot {} -> {}", src, dst);
                self.stack.push(Value::int(-1));
                Some(Ok(InnerOutcome::Normal))
            }
            0x3474D65B => {
                let slot = args.int_or(0, 0) as i64;
                let title = args.bytes_or_empty(1);
                host.save_write_title(slot, title);
                eprintln!(
                    "[SAVE] write_title slot={} {:?}", slot,
                    String::from_utf8_lossy(title)
                );
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x78A6A112 => {
                let slot = args.int_or(0, 0) as i64;
                let page = args.int_or(1, 0) as u32;
                let x = args.int_or(2, 0);
                let y = args.int_or(3, 0);
                let ok = host.save_read_thumbnail(slot, page, x, y);
                eprintln!(
                    "[SAVE] read thumbnail slot={} -> page={} ({},{}) ok={}", slot, page,
                    x, y, ok
                );
                self.stack.push(Value::int(if ok { 0 } else { -1 }));
                Some(Ok(InnerOutcome::Normal))
            }
            0x168EFCB7 => {
                let slot = args.int_or(0, 0) as i64;
                host.save_delete_slot(slot);
                eprintln!("[SAVE] delete slot={}", slot);
                self.stack.push(Value::int(-1));
                Some(Ok(InnerOutcome::Normal))
            }
            0x65075141 => {
                let slot = args.int_or(0, 0) as i64;
                let s = host.save_read_meta(slot);
                eprintln!("[SAVE] read_meta slot={} {:?}", slot, s);
                self.stack.push(Value::string_text(&s));
                Some(Ok(InnerOutcome::Normal))
            }
            0xA549E852 => {
                let slot = args.int_or(0, 0) as i64;
                let s = host.save_get_timestamp(slot);
                eprintln!("[SAVE] get_timestamp slot={} {:?}", slot, s);
                self.stack.push(Value::string_text(&s));
                Some(Ok(InnerOutcome::Normal))
            }
            0x4C99B0EA => {
                let s = host.save_get_description();
                eprintln!("[SAVE] get_description {:?}", s);
                self.stack.push(Value::string_text(&s));
                Some(Ok(InnerOutcome::Normal))
            }
            0x9143DC9E => {
                let w = args.int_or(0, 0) as u32;
                let h = args.int_or(1, 0) as u32;
                let src = if count >= 3 { Some(args.int_or(2, 0) as u32) } else { None };
                host.save_make_thumbnail(w, h, src);
                eprintln!("[SAVE] make_thumbnail_simple {}x{} src={:?}", w, h, src);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xE91B0504 => {
                let dst = args.int_or(0, 0) as u32;
                let src = args.int_or(1, 0) as u32;
                let w = args.int_or(2, 0) as u32;
                let h = args.int_or(3, 0) as u32;
                host.save_make_thumbnail_full(dst, src, w, h);
                eprintln!("[SAVE] make_thumbnail page={} src={} {}x{}", dst, src, w, h);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x89CE8C53 => {
                let page = args.int_or(0, 0) as u32;
                let x = args.int_or(1, 0);
                let y = args.int_or(2, 0);
                let ok = host.save_blit_thumbnail(page, x, y);
                eprintln!(
                    "[SAVE] blit_thumbnail -> page={} ({},{}) ok={}", page, x, y, ok
                );
                self.stack.push(Value::int(if ok { 0 } else { -1 }));
                Some(Ok(InnerOutcome::Normal))
            }
            0x3378635D => {
                self.free_save_snapshot();
                host.save_free_data();
                eprintln!("[SAVE] free_data");
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x7C3B36C5 => {
                let flag = host.save_take_ready_flag();
                eprintln!("[SAVE] get_ready_flag -> {}", flag);
                self.stack.push(Value::int(flag));
                Some(Ok(InnerOutcome::Normal))
            }
            0x1BB47604 => {
                host.save_mark_ready();
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x44B5C4ED => {
                let old = host.save_begin_serialize();
                eprintln!("[SAVE] serialize (was_avail={})", old);
                self.stack.push(Value::int(old));
                Some(Ok(InnerOutcome::Normal))
            }
            0xF8004993 => {
                if count >= 1 {
                    if let Some(desc) = args.bytes(0) {
                        let desc = host.localize_save_description(desc);
                        self.set_save_description(&desc);
                        host.save_set_description(&desc);
                        eprintln!(
                            "[SAVE] resource_load desc={:?}", String::from_utf8_lossy(&
                            desc)
                        );
                    }
                }
                if host.save_snapshot_allowed() {
                    self.capture_save_snapshot();
                    if let Some(snapshot) = self.pending_save_snapshot.as_mut() {
                        let audio_blob = host.save_capture_audio_state();
                        if audio_blob.len() == formats::save::AUDIO_BLOB_SIZE {
                            snapshot.header.audio_blob = audio_blob;
                        } else {
                            eprintln!(
                                "[SAVE] audio snapshot rejected: expected {} bytes, got {}",
                                formats::save::AUDIO_BLOB_SIZE, audio_blob.len()
                            );
                        }
                    }
                }
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x163C0878 => {
                let params: Vec<i32> = (0..args.len())
                    .filter_map(|n| args.int(n))
                    .collect();
                host.save_sysconfig(&params);
                eprintln!("[SAVE] sysconfig {} params {:?}", params.len(), params);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            _ => None,
        }
    }
}

