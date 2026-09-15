use super::*;
const CONFIG_MAGIC: &[u8; 8] = b"OSTBCFG1";
impl EngineHost {
    pub(super) fn unimplemented_handler_impl(
        &mut self,
        hash: u32,
        name: &str,
        args: &[vm::value::Value],
    ) {
        let count = self.unimplemented_counts.entry(hash).or_default();
        *count += 1;
        if *count == 1 {
            eprintln!(
                "[UNIMPL] first hit 0x{:08X} {} args={:?} (repeats suppressed)", hash,
                name, args
            );
        }
    }
    pub(super) fn sys_skip_logic_impl(&mut self, _flag: u16) {}
    pub(super) fn save_mark_ready_impl(&mut self) {
        self.save_system_ready = true;
    }
    pub(super) fn save_take_ready_flag_impl(&mut self) -> i32 {
        let ready = self.save_system_ready;
        self.save_system_ready = false;
        i32::from(ready)
    }
    pub(super) fn save_dialog_available_impl(&mut self) -> bool {
        !self.save_dialog_avail_flag
    }
    pub(super) fn save_set_dialog_available_impl(&mut self, available: bool) {
        self.save_dialog_avail_flag = available;
    }
    pub(super) fn load_script_impl(&mut self, name: &str) -> Option<LoadedScript> {
        match self.resolve_script(name) {
            Some(loaded) => {
                eprintln!(
                    "[SCRIPT] loaded {:?} ({} entries, {} bytes code)", name, loaded
                    .entries.len(), loaded.code.len()
                );
                Some(loaded)
            }
            None => {
                eprintln!("[SCRIPT] could not load {:?}", name);
                None
            }
        }
    }
    pub(super) fn generate_key_impl(&mut self, key_str: &[u8]) {
        if key_str.first().copied().unwrap_or(0) == 0 {
            return;
        }
        self.rct_key = formats::image::build_key_from_bytes(key_str);
        eprintln!(
            "[GFX] generated active TS/RCT key from {:?}",
            String::from_utf8_lossy(key_str)
        );
    }
    pub(super) fn show_message_ok_impl(&mut self, message: &[u8]) {
        crate::platform_services::message_ok(
            &super::text_files::sjis_to_string(message),
        );
    }
    pub(super) fn config_load_impl(&mut self, name: &[u8], default: i32) -> i32 {
        self.config_values.get(name).copied().unwrap_or(default)
    }
    pub(super) fn config_store_impl(&mut self, name: &[u8], value: i32) {
        self.config_values.insert(name.to_vec(), value);
        self.flush_config_file_impl();
    }
    pub(super) fn set_fullscreen_impl(&mut self, fullscreen: bool) {
        self.render_dirty = true;
        if self.window_fullscreen != fullscreen {
            self.window_fullscreen = fullscreen;
            self.pending_fullscreen = Some(fullscreen);
        }
        self.config_store_impl(b"WinStat", i32::from(fullscreen));
    }
    pub(super) fn is_fullscreen_impl(&mut self) -> bool {
        self.window_fullscreen
    }
    pub fn take_fullscreen_request(&mut self) -> Option<bool> {
        self.pending_fullscreen.take()
    }
    pub(super) fn get_effect_speed_impl(&mut self) -> i32 {
        self.config_load_impl(b"EffectSpeed", 1000)
    }
    pub(super) fn set_effect_speed_impl(&mut self, speed: i32) {
        self.config_store_impl(b"EffectSpeed", speed);
    }
    pub(super) fn config_set_autospeed_impl(&mut self, speed: i32) {
        self.config_store_impl(b"AutoSpeed", speed);
    }
    pub(super) fn get_voice_enable_flag_impl(&mut self) -> bool {
        self.config_load_impl(b"TalkFemale", 1) != 0
    }
    pub(super) fn get_voice_panning_flag_impl(&mut self) -> i32 {
        self.config_load_impl(b"VoicePanning", 0)
    }
    pub(super) fn set_voice_ts_flag_impl(&mut self) {
        self.voice_ts_flag = true;
    }
    pub(super) fn load_config_file_impl(&mut self) {
        let Ok(Some(bytes)) = self.storage.read(&self.config_file) else {
            return;
        };
        let Some(mut rest) = bytes.strip_prefix(CONFIG_MAGIC) else {
            eprintln!("[CONFIG] ignored invalid {}", self.config_file);
            return;
        };
        let Some(count_bytes) = rest.get(..4) else {
            return;
        };
        let count = u32::from_le_bytes(count_bytes.try_into().unwrap()) as usize;
        rest = &rest[4..];
        let mut loaded = HashMap::with_capacity(count.min(4096));
        for _ in 0..count.min(4096) {
            let Some(len_bytes) = rest.get(..4) else {
                return;
            };
            let len = u32::from_le_bytes(len_bytes.try_into().unwrap()) as usize;
            rest = &rest[4..];
            if len > 4096 || rest.len() < len + 4 {
                return;
            }
            let key = rest[..len].to_vec();
            let value = i32::from_le_bytes(rest[len..len + 4].try_into().unwrap());
            rest = &rest[len + 4..];
            loaded.insert(key, value);
        }
        self.config_values = loaded;
        let fullscreen = self.config_load_impl(b"WinStat", 0) != 0;
        if self.window_fullscreen != fullscreen {
            self.window_fullscreen = fullscreen;
            self.pending_fullscreen = Some(fullscreen);
        }
        self.voice_ts_flag = self.config_load_impl(b"VoiceSkipDef", 0) != 0;
        eprintln!(
            "[CONFIG] loaded {} values from {}", self.config_values.len(), self
            .config_file
        );
    }
    fn flush_config_file_impl(&self) {
        let mut entries: Vec<_> = self.config_values.iter().collect();
        entries.sort_unstable_by_key(|(left, _)| *left);
        let mut bytes = Vec::with_capacity(12 + entries.len() * 16);
        bytes.extend_from_slice(CONFIG_MAGIC);
        bytes.extend_from_slice(&(entries.len() as u32).to_le_bytes());
        for (key, value) in entries {
            bytes.extend_from_slice(&(key.len() as u32).to_le_bytes());
            bytes.extend_from_slice(key);
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        if let Err(error) = self.storage.write(&self.config_file, &bytes) {
            eprintln!("[CONFIG] failed to write {}: {}", self.config_file, error);
        }
    }
    pub(super) fn set_input_dispatch_mode_impl(&mut self, mode: i32) {
        self.input_dispatch_mode = mode;
    }
    pub(super) fn take_display_mode_impl(&mut self) -> i32 {
        std::mem::take(&mut self.display_mode)
    }
    pub(super) fn register_picture_hash_impl(&mut self, hash: u32) -> bool {
        self.picture_hashes.contains(&hash)
            || (self.picture_hashes.len() < 10_000 && self.picture_hashes.insert(hash))
    }
    pub(super) fn picture_hash_registered_impl(&mut self, hash: u32) -> bool {
        self.picture_hashes.contains(&hash)
    }
}
