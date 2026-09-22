use super::*;
use crate::SystemTime;
use encoding_rs::SHIFT_JIS;
use formats::save::{
    patch_sav_description, read_mss, read_readmarks, read_sav_meta_with_layout,
    read_sav_with_layout, write_mss, write_readmarks, write_sav_with_layout, MssFile,
    ReadmarkRecord, SavFile, Thumbnail,
};
impl EngineHost {
    pub(super) fn load_picture_hashes_impl(&mut self) {
        self.picture_hashes.clear();
        let Ok(Some(bytes)) = self.storage.read(SYSTEM_MSS) else {
            return;
        };
        match read_mss(&bytes) {
            Ok(system) => {
                self.picture_hashes.extend(system.picture_hashes());
                eprintln!(
                    "[BOOT] loaded {} viewed-picture hashes from {}", self.picture_hashes
                    .len(), SYSTEM_MSS
                );
            }
            Err(error) => {
                eprintln!(
                    "[BOOT] could not restore viewed-picture hashes from {}: {}",
                    SYSTEM_MSS, error
                )
            }
        }
    }
    fn merge_picture_hashes(&mut self, system: &MssFile) {
        self.picture_hashes.extend(system.picture_hashes());
    }
    pub(super) fn save_write_slot_impl(
        &mut self,
        slot: i64,
        save: &SavFile,
        system: &MssFile,
    ) -> bool {
        self.save_last_command_active = true;
        let name = self.save_slot_name(slot);
        let mut save = save.clone();
        save.thumbnail = self.save_thumbnail.clone();
        if let Some(thumbnail) = &save.thumbnail {
            save.header.thumb_w = thumbnail.width;
            save.header.thumb_h = thumbnail.height;
        } else {
            save.header.thumb_w = 0;
            save.header.thumb_h = 0;
        }
        let bytes = match write_sav_with_layout(&mut save, &self.sav_layout) {
            Ok(bytes) => bytes,
            Err(error) => {
                eprintln!("[SAVE] failed to serialize {}: {}", name, error);
                return false;
            }
        };
        if let Err(error) = self.storage.write(&name, &bytes) {
            eprintln!("[SAVE] failed to write {}: {}", name, error);
            return false;
        }
        if !self.save_write_system_impl(system) {
            return false;
        }
        eprintln!("[SAVE] wrote {}", name);
        true
    }
    pub(super) fn save_write_system_impl(&mut self, system: &MssFile) -> bool {
        let mut system = system.clone();
        if let Ok(Some(bytes)) = self.storage.read(SYSTEM_MSS) {
            if let Ok(existing) = read_mss(&bytes) {
                system.header_tail = existing.header_tail;
            }
        }
        let mut picture_hashes = self.picture_hashes.iter().copied().collect::<Vec<_>>();
        picture_hashes.sort_unstable();
        system.set_picture_hashes(picture_hashes);
        let system_bytes = match write_mss(&mut system) {
            Ok(bytes) => bytes,
            Err(error) => {
                eprintln!("[SAVE] failed to serialize {}: {}", SYSTEM_MSS, error);
                return false;
            }
        };
        if let Err(error) = self.storage.write(SYSTEM_MSS, &system_bytes) {
            eprintln!("[SAVE] failed to write {}: {}", SYSTEM_MSS, error);
            return false;
        }
        eprintln!("[SAVE] wrote {}", SYSTEM_MSS);
        true
    }
    pub(super) fn save_read_readmarks_impl(&mut self) -> Option<Vec<ReadmarkRecord>> {
        match self.storage.read(READMARK_MSS) {
            Ok(Some(bytes)) => {
                match read_readmarks(&bytes) {
                    Ok(records) => Some(records),
                    Err(error) => {
                        eprintln!("[SAVE] rejected {}: {}", READMARK_MSS, error);
                        None
                    }
                }
            }
            Ok(None) => Some(Vec::new()),
            Err(error) => {
                eprintln!("[SAVE] failed to read {}: {}", READMARK_MSS, error);
                None
            }
        }
    }
    pub(super) fn save_write_readmarks_impl(
        &mut self,
        records: &[ReadmarkRecord],
    ) -> bool {
        if records.is_empty() {
            return true;
        }
        let bytes = match write_readmarks(records) {
            Ok(bytes) => bytes,
            Err(error) => {
                eprintln!("[SAVE] failed to serialize {}: {}", READMARK_MSS, error);
                return false;
            }
        };
        if let Err(error) = self.storage.write(READMARK_MSS, &bytes) {
            eprintln!("[SAVE] failed to write {}: {}", READMARK_MSS, error);
            return false;
        }
        eprintln!("[SAVE] wrote {}", READMARK_MSS);
        true
    }
    pub fn reset_for_outer_scene_cycle(&mut self) {
        self.reset_outer_scene_state_impl("outer scene cycle", false);
    }
    pub fn reset_for_outer_scene_cycle_preserve_audio(&mut self) {
        self.reset_outer_scene_state_impl("outer scene cycle (preserve audio)", true);
    }
    pub(super) fn save_prepare_restore_impl(&mut self) {
        self.reset_outer_scene_state_impl("save restore", false);
    }
    fn reset_outer_scene_state_impl(&mut self, boundary: &str, preserve_audio: bool) {
        self.frontbuffer = self.base_page;
        self.display_page = Some(self.base_page);
        eprintln!("[VIDEO] scene-cycle cleanup drops movie (boundary {boundary})");
        self.movie.cleanup();
        if preserve_audio {
            vm::text_trace!(
                "[AUDIO] outer scene cycle preserves audio nodes ({boundary})"
            );
        } else {
            self.audio.reset_scene_state();
        }
        self.reset_input_for_load_impl();
        self.native_mode_bits = 0;
        self.native_mode_special = false;
        self.clock_scale = 1.0;
        self.clock_anchor_real_ms = 0;
        self.clock_offset_ms = 0;
        self.last_animation_update_ms = None;
        self.cursor_visible = true;
        self.cursor_visibility_dirty = true;
        for sprite in self.sprites.values_mut() {
            sprite.position = Some((100_000, 100_000));
            sprite.position_animation = None;
            sprite.alpha_animation = None;
            sprite.frame_animation = None;
        }
        self.sprite_visibility_depths.clear();
        self.sprite_rotations.clear();
        self.sprite_xmodifies.clear();
        self.sprite_ymodifies.clear();
        let transient_sprites = self
            .sprites
            .keys()
            .copied()
            .filter(|sprite| !self.scene_resident_sprites.contains(sprite))
            .collect::<Vec<_>>();
        for sprite in &transient_sprites {
            self.sprite_release_impl(*sprite);
        }
        let smooth_pages = std::mem::take(&mut self.sprite_smooth_pages);
        self.sprite_smooth_page_states.clear();
        for page in smooth_pages.into_values() {
            if self.pages.contains_key(&page)
                && !self.scene_resident_pages.contains(&page)
            {
                self.release_unowned_page(page);
            }
        }
        let resident_sprite_pages = self
            .sprites
            .values()
            .map(|sprite| base_page_handle(sprite.page))
            .collect::<HashSet<_>>();
        let transient_pages = self
            .pages
            .keys()
            .copied()
            .filter(|page| {
                *page != self.base_page && !self.scene_resident_pages.contains(page)
                    && !resident_sprite_pages.contains(page)
            })
            .collect::<Vec<_>>();
        for page in &transient_pages {
            if self.pages.contains_key(page) {
                self.release_unowned_page(*page);
            }
        }
        self.pending_quads.clear();
        self.presented_scene = None;
        self.scene_dirty_freeze = 0;
        self.save_system_ready = false;
        self.save_dialog_avail_flag = false;
        self.save_last_command_active = false;
        self.display_epoch = self.display_epoch.wrapping_add(1);
        self.render_dirty = true;
        eprintln!(
            "[SCENE] {} selected base page {}; retired {} sprites and {} pages",
            boundary, self.base_page, transient_sprites.len(), transient_pages.len()
        );
    }
    pub(super) fn save_refresh_after_restore_impl(&mut self) {
        self.frontbuffer = self.base_page;
        self.display_page = Some(self.base_page);
        self.display_epoch = self.display_epoch.wrapping_add(1);
        self.render_dirty = true;
        eprintln!(
            "[SAVE] post-restore display selected resident base page {}", self.base_page
        );
    }
    pub(super) fn save_read_slot_impl(&mut self, slot: i64) -> Option<SavFile> {
        self.save_last_command_active = true;
        let name = self.save_slot_name(slot);
        let bytes = match self.storage.read(&name) {
            Ok(Some(bytes)) => bytes,
            Ok(None) => return None,
            Err(error) => {
                eprintln!("[SAVE] failed to read {}: {}", name, error);
                return None;
            }
        };
        match read_sav_with_layout(&bytes, &self.sav_layout) {
            Ok(save) => {
                self.save_description = nul_trim(&save.header.description).to_vec();
                self.save_thumbnail = save.thumbnail.clone();
                Some(save)
            }
            Err(error) => {
                eprintln!("[SAVE] rejected {}: {}", name, error);
                None
            }
        }
    }
    pub(super) fn save_read_system_impl(&mut self) -> Option<MssFile> {
        let bytes = match self.storage.read(SYSTEM_MSS) {
            Ok(Some(bytes)) => bytes,
            Ok(None) => return None,
            Err(error) => {
                eprintln!("[SAVE] failed to read {}: {}", SYSTEM_MSS, error);
                return None;
            }
        };
        match read_mss(&bytes) {
            Ok(system) => {
                self.merge_picture_hashes(&system);
                Some(system)
            }
            Err(error) => {
                eprintln!("[SAVE] rejected {}: {}", SYSTEM_MSS, error);
                None
            }
        }
    }
    pub(super) fn save_copy_slot_impl(&mut self, src: i64, dst: i64) {
        let source = self.save_slot_name(src);
        let destination = self.save_slot_name(dst);
        if let Err(error) = self.storage.copy(&source, &destination) {
            eprintln!("[SAVE] failed to copy {} to {}: {}", source, destination, error);
        }
    }
    pub(super) fn save_write_title_impl(&mut self, slot: i64, title: &[u8]) {
        let name = self.save_slot_name(slot);
        let mut bytes = match self.storage.read(&name) {
            Ok(Some(bytes)) => bytes,
            Ok(None) => {
                eprintln!("[SAVE] failed to read {}: not found", name);
                return;
            }
            Err(error) => {
                eprintln!("[SAVE] failed to read {}: {}", name, error);
                return;
            }
        };
        let localized = self.localize_save_description_impl(title);
        if let Err(error) = patch_sav_description(&mut bytes, &localized) {
            eprintln!("[SAVE] rejected title patch {}: {}", name, error);
            return;
        }
        if let Err(error) = self.storage.write(&name, &bytes) {
            eprintln!("[SAVE] failed to patch {}: {}", name, error);
        }
    }
    pub(super) fn save_read_thumbnail_impl(
        &mut self,
        slot: i64,
        dst_page: u32,
        x: i32,
        y: i32,
    ) -> Option<[i32; 4]> {
        let name = self.save_slot_name(slot);
        let bytes = self.storage.read(&name).ok().flatten()?;
        let Ok((_, thumbnail)) = read_sav_meta_with_layout(&bytes, &self.sav_layout)
        else {
            return None;
        };
        let thumbnail = thumbnail?;
        let rect = [x, y, thumbnail.width as i32, thumbnail.height as i32];
        self.blit_save_thumbnail(&thumbnail, dst_page, x, y).then_some(rect)
    }
    pub(super) fn save_delete_slot_impl(&mut self, slot: i64) {
        let name = self.save_slot_name(slot);
        self.storage.delete(&name);
    }
    pub(super) fn save_read_meta_impl(&mut self, slot: i64) -> String {
        let name = self.save_slot_name(slot);
        self.storage
            .read(&name)
            .ok()
            .flatten()
            .and_then(|bytes| read_sav_meta_with_layout(&bytes, &self.sav_layout).ok())
            .map(|(header, _)| {
                let text = self.description_text(&header.description);
                self.remember_save_description_bytes(&text);
                text
            })
            .unwrap_or_default()
    }
    pub(super) fn save_get_timestamp_impl(&mut self, slot: i64) -> String {
        let name = self.save_slot_name(slot);
        let valid = self
            .storage
            .read(&name)
            .ok()
            .flatten()
            .and_then(|bytes| read_sav_meta_with_layout(&bytes, &self.sav_layout).ok())
            .is_some();
        if !valid {
            return String::new();
        }
        self.storage
            .last_modified(&name)
            .and_then(format_last_write_time)
            .unwrap_or_default()
    }
    pub(super) fn save_get_description_impl(&mut self) -> String {
        let text = self.description_text(&self.save_description);
        self.remember_save_description_bytes(&text);
        text
    }
    pub(super) fn description_text(&self, raw: &[u8]) -> String {
        let raw = nul_trim(raw);
        crate::text::unicode_override_text(raw)
            .map(str::to_owned)
            .unwrap_or_else(|| decode_sjis(raw))
    }
    pub(super) fn remember_save_description_bytes(&mut self, text: &str) {
        self.last_save_meta_bytes = vm::Value::string_text(text)
            .as_str_bytes()
            .unwrap_or(&[])
            .to_vec();
    }
    pub fn renders_save_description_verbatim(&self, text: &[u8]) -> bool {
        let desc = &self.last_save_meta_bytes;
        desc.len() >= 4 && text.len() >= desc.len()
            && text.windows(desc.len()).any(|window| window == desc.as_slice())
    }
    pub(super) fn decode_save_description_render(&self, text: &[u8]) -> Option<String> {
        let desc = &self.last_save_meta_bytes;
        let decoded_desc = std::str::from_utf8(desc).ok()?;
        if decoded_desc.is_ascii() {
            return None;
        }
        if text.len() < desc.len() {
            return None;
        }
        let position = text
            .windows(desc.len())
            .position(|window| window == desc.as_slice())?;
        let head = sjis_to_string(&text[..position]);
        let tail = sjis_to_string(&text[position + desc.len()..]);
        Some(format!("{head}{decoded_desc}{tail}"))
    }
    pub(super) fn localize_save_description_impl<'a>(
        &self,
        description: &'a [u8],
    ) -> std::borrow::Cow<'a, [u8]> {
        if !self.patch.as_ref().is_some_and(|patch| patch.one_way_save_titles()) {
            return std::borrow::Cow::Borrowed(description);
        }
        let raw = nul_trim(description);
        let Some(text) = self.localize_ir_display(raw) else {
            return std::borrow::Cow::Borrowed(description);
        };
        let mut bytes = text.into_bytes();
        let mut end = bytes.len().min(127);
        while end > 0 && end < bytes.len() && bytes[end] & 0xC0 == 0x80 {
            end -= 1;
        }
        bytes.truncate(end);
        std::borrow::Cow::Owned(bytes)
    }
    pub(super) fn save_set_description_impl(&mut self, description: &[u8]) {
        self.save_description.clear();
        self.save_description
            .extend_from_slice(
                nul_trim(description).get(..127).unwrap_or(nul_trim(description)),
            );
    }
    pub(super) fn save_make_thumbnail_impl(
        &mut self,
        width: u32,
        height: u32,
        source: Option<u32>,
    ) {
        self.save_thumbnail = self.capture_save_thumbnail(width, height, source);
    }
    pub(super) fn save_blit_thumbnail_impl(
        &mut self,
        dst_page: u32,
        x: i32,
        y: i32,
    ) -> bool {
        let Some(thumbnail) = self.save_thumbnail.clone() else {
            return false;
        };
        self.blit_save_thumbnail(&thumbnail, dst_page, x, y)
    }
    pub(super) fn save_free_data_impl(&mut self) {
        self.save_thumbnail = None;
        self.save_last_command_active = false;
    }
    pub(super) fn save_begin_serialize_impl(&mut self) -> i32 {
        let was_available = self.save_dialog_avail_flag;
        self.save_dialog_avail_flag = false;
        i32::from(was_available)
    }
    pub(super) fn scene_unload_and_exit_impl(&mut self) {
        self.exit_requested = true;
    }
    pub fn take_exit_requested(&mut self) -> bool {
        std::mem::take(&mut self.exit_requested)
    }
    fn save_slot_name(&self, slot: i64) -> String {
        if slot < 0 || slot == 10_001 {
            "majiro_last.sav".to_string()
        } else if slot >= 10_000 {
            let quick_num = i64::from(
                self.config_values.get(b"QuickNum".as_slice()).copied().unwrap_or(0),
            );
            let index = (10_011 - slot + quick_num).rem_euclid(8);
            format!("majiro_quick{index}.sav")
        } else {
            format!("majiro_{slot:04}.sav")
        }
    }
    fn capture_save_thumbnail(
        &self,
        width: u32,
        height: u32,
        source: Option<u32>,
    ) -> Option<Thumbnail> {
        if width == 0 || height == 0 {
            return None;
        }
        let source = source.filter(|handle| *handle != 0).unwrap_or(self.frontbuffer);
        let page = self.page_for_read(source)?;
        if page.width == 0 || page.height == 0 {
            return None;
        }
        let mut pixels_bgr = vec![0; width as usize * height as usize * 3];
        let dst_stride = width as usize * 3;
        let sample_offsets = [
            (1usize, 1usize),
            (2usize, 3usize),
            (3usize, 4usize),
            (4usize, 2usize),
        ];
        for dst_y in 0..height as usize {
            for dst_x in 0..width as usize {
                let dst = dst_y * dst_stride + dst_x * 3;
                let base_x = dst_x * page.width as usize / width as usize;
                let base_y = dst_y * page.height as usize / height as usize;
                let block_w = page.width as usize / width as usize;
                let block_h = page.height as usize / height as usize;
                let mut totals = [0u32; 3];
                for (x_fifth, y_fifth) in sample_offsets {
                    let src_x = (base_x + x_fifth * block_w / 5)
                        .min(page.width as usize - 1);
                    let src_y = (base_y + y_fifth * block_h / 5)
                        .min(page.height as usize - 1);
                    let src = (src_y * page.width as usize + src_x) * 4;
                    totals[0] += u32::from(page.pixels[src + 2]);
                    totals[1] += u32::from(page.pixels[src + 1]);
                    totals[2] += u32::from(page.pixels[src]);
                }
                pixels_bgr[dst] = (totals[0] / 4) as u8;
                pixels_bgr[dst + 1] = (totals[1] / 4) as u8;
                pixels_bgr[dst + 2] = (totals[2] / 4) as u8;
            }
        }
        Some(Thumbnail {
            width,
            height,
            pixels_bgr,
            pixels_extra: None,
        })
    }
    fn blit_save_thumbnail(
        &mut self,
        thumbnail: &Thumbnail,
        dst_page: u32,
        x: i32,
        y: i32,
    ) -> bool {
        let page_handle = base_page_handle(dst_page);
        let Some(page) = self.pages.get_mut(&page_handle) else {
            return false;
        };
        let page_width = page.width as i32;
        let page_height = page.height as i32;
        let pixels = Arc::make_mut(&mut page.pixels);
        let mut changed = false;
        for src_y in 0..thumbnail.height as i32 {
            let dst_y = y.saturating_add(src_y);
            if !(0..page_height).contains(&dst_y) {
                continue;
            }
            for src_x in 0..thumbnail.width as i32 {
                let dst_x = x.saturating_add(src_x);
                if !(0..page_width).contains(&dst_x) {
                    continue;
                }
                let source = (src_y as usize * thumbnail.width as usize + src_x as usize)
                    * 3;
                let destination = (dst_y as usize * page.width as usize + dst_x as usize)
                    * 4;
                let rgba = [
                    thumbnail.pixels_bgr[source + 2],
                    thumbnail.pixels_bgr[source + 1],
                    thumbnail.pixels_bgr[source],
                    0xFF,
                ];
                if pixels[destination..destination + 4] != rgba {
                    pixels[destination..destination + 4].copy_from_slice(&rgba);
                    changed = true;
                }
            }
        }
        if changed {
            let rect = [x, y, thumbnail.width as i32, thumbnail.height as i32];
            self.mark_page_dirty_rect(page_handle, rect[0], rect[1], rect[2], rect[3]);
        }
        true
    }
}
fn nul_trim(bytes: &[u8]) -> &[u8] {
    bytes.split(|byte| *byte == 0).next().unwrap_or(bytes)
}
fn decode_sjis(bytes: &[u8]) -> String {
    SHIFT_JIS.decode(bytes).0.into_owned()
}
fn format_last_write_time(modified: SystemTime) -> Option<String> {
    crate::platform_services::format_save_last_write_time(modified)
}
const SYSTEM_MSS: &str = "majiro_system.mss";
const READMARK_MSS: &str = "majiro_readmark.mss";

