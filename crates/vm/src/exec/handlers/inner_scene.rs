use crate::exec::VmError;
use crate::host::Host;
use super::inner::InnerOutcome;
pub(super) const HASHES: &[u32] = &[
    0x10DB0A43, 0xCD00D280, 0x9627DDC3, 0xEDEFB0E0, 0xEDEFB0B0, 0x718EF651,
];
enum EntryResolution {
    MainOffset,
    ByHash(u32),
}
impl crate::exec::Vm {
    pub(super) fn handle_inner_scene<H: Host>(
        &mut self,
        hash: u32,
        count: usize,
        has_retval: bool,
        host: &mut H,
    ) -> Option<Result<InnerOutcome, VmError>> {
        match hash {
            0x10DB0A43 | 0xCD00D280 => {
                Some(
                    self
                        .inner_scene_load(
                            count,
                            has_retval,
                            host,
                            "scene_load_script",
                            EntryResolution::MainOffset,
                            false,
                        ),
                )
            }
            0x9627DDC3 => {
                let offset_hash = self
                    .stack
                    .peek_from_top(1)
                    .map(|v| v.bits as i32 as u32)
                    .unwrap_or(0);
                Some(
                    self
                        .inner_scene_load(
                            count,
                            has_retval,
                            host,
                            "scene_load_with_offset",
                            EntryResolution::ByHash(offset_hash),
                            false,
                        ),
                )
            }
            0xEDEFB0E0 | 0xEDEFB0B0 => {
                let entry_hash = self
                    .stack
                    .peek_from_top(1)
                    .and_then(|v| v.as_str_bytes().map(crate::formats_crc32))
                    .unwrap_or(0);
                Some(
                    self
                        .inner_scene_load(
                            count,
                            has_retval,
                            host,
                            "scene_load_script_c",
                            EntryResolution::ByHash(entry_hash),
                            false,
                        ),
                )
            }
            0x718EF651 => {
                Some(
                    self
                        .inner_scene_load(
                            count,
                            has_retval,
                            host,
                            "scene_jump_load",
                            EntryResolution::MainOffset,
                            true,
                        ),
                )
            }
            _ => None,
        }
    }
    fn inner_scene_load<H: Host>(
        &mut self,
        count: usize,
        _has_retval: bool,
        host: &mut H,
        kind: &str,
        resolution: EntryResolution,
        reset_current: bool,
    ) -> Result<InnerOutcome, VmError> {
        let name_val = match self.stack.peek().cloned() {
            Some(v) => v,
            None => return Err(VmError::StackUnderflow),
        };
        let mut name_bytes = name_val.as_str_bytes().unwrap_or(&[]).to_vec();
        while name_bytes.last() == Some(&0) {
            name_bytes.pop();
        }
        self.stack.pop();
        let pops_second = matches!(resolution, EntryResolution::ByHash(_));
        if pops_second {
            self.stack.pop();
        }
        let name_str = {
            let (cow, _, _) = encoding_rs::SHIFT_JIS.decode(&name_bytes);
            cow.into_owned()
        };
        let log_extra = match &resolution {
            EntryResolution::MainOffset => String::new(),
            EntryResolution::ByHash(h) => format!(" entry_hash=0x{:08X}", h),
        };
        eprintln!("[SCRIPT] {} name={:?}{}", kind, name_str, log_extra);
        if let Some(loaded) = host.load_script(&name_str) {
            let main_offset = loaded.main_offset as usize;
            let entries: Vec<crate::exec::ScriptEntry> = loaded
                .entries
                .iter()
                .map(|&(hash, off)| crate::exec::ScriptEntry {
                    name_hash: hash,
                    offset: off,
                })
                .collect();
            let script_idx = self
                .load_script_with_entries_named_readmarks_crc(
                    loaded.code,
                    entries,
                    Some(name_bytes),
                    loaded.line_count,
                    loaded.code_crc32,
                );
            let (entry_offset, resolution_label) = match resolution {
                EntryResolution::MainOffset => (main_offset, "main_offset".to_string()),
                EntryResolution::ByHash(hash) => {
                    let Some(entry) = self
                        .script_entries(script_idx)
                        .iter()
                        .find(|entry| entry.name_hash == hash) else {
                        return Err(VmError::UnknownEntryHash {
                            hash,
                            from_script: script_idx,
                            ip: 0,
                        });
                    };
                    (entry.offset as usize, format!("hash:0x{:08X}", hash))
                }
            };
            let caller_sp = self.stack.len();
            let consumed = if pops_second { 2 } else { 1 };
            let forwarded = count.saturating_sub(consumed);
            if reset_current {
                let start = caller_sp.saturating_sub(forwarded);
                let forwarded_args = self.stack.as_slice()[start..caller_sp].to_vec();
                self.reset_active_context_to_entry(
                    script_idx,
                    entry_offset,
                    forwarded_args,
                    format!("scene-reset@0x{entry_offset:06X}"),
                )?;
            } else {
                self.exec_call(
                    script_idx,
                    self.current_ip().unwrap_or(0),
                    entry_offset,
                    caller_sp,
                    0,
                    forwarded as i16,
                )?;
            }
            eprintln!(
                "[SCRIPT] → entered {:?} script_idx={} entry_offset=0x{:X} ({})",
                name_str, script_idx, entry_offset, resolution_label
            );
        } else {
            eprintln!(
                "[SCRIPT] ⚠ {:?} could not be loaded by host; continuing (trace-noop)",
                name_str
            );
        }
        Ok(InnerOutcome::SelfManaged)
    }
}
