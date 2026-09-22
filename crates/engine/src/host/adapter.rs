use super::*;
impl Host for EngineHost {
    fn unimplemented_handler(
        &mut self,
        hash: u32,
        name: &str,
        args: &[vm::value::Value],
    ) {
        self.unimplemented_handler_impl(hash, name, args)
    }
    fn sys_skip_logic(&mut self, _flag: u16) {
        self.sys_skip_logic_impl(_flag)
    }
    fn save_mark_ready(&mut self) {
        self.save_mark_ready_impl()
    }
    fn save_take_ready_flag(&mut self) -> i32 {
        self.save_take_ready_flag_impl()
    }
    fn save_dialog_available(&mut self) -> bool {
        self.save_dialog_available_impl()
    }
    fn save_set_dialog_available(&mut self, available: bool) {
        self.save_set_dialog_available_impl(available)
    }
    fn save_finish_command(&mut self) {
        self.save_last_command_active = false;
    }
    fn show_message_ok(&mut self, message: &[u8]) {
        self.show_message_ok_impl(message)
    }
    fn save_snapshot_allowed(&mut self) -> bool {
        self.save_system_ready && self.save_dialog_avail_flag
    }
    fn save_write_slot(
        &mut self,
        slot: i64,
        save: &formats::save::SavFile,
        system: &formats::save::MssFile,
    ) -> bool {
        self.save_write_slot_impl(slot, save, system)
    }
    fn save_read_slot(&mut self, slot: i64) -> Option<formats::save::SavFile> {
        self.save_read_slot_impl(slot)
    }
    fn save_read_system(&mut self) -> Option<formats::save::MssFile> {
        self.save_read_system_impl()
    }
    fn save_write_system(&mut self, system: &formats::save::MssFile) -> bool {
        self.save_write_system_impl(system)
    }
    fn save_read_readmarks(&mut self) -> Option<Vec<formats::save::ReadmarkRecord>> {
        self.save_read_readmarks_impl()
    }
    fn save_write_readmarks(
        &mut self,
        records: &[formats::save::ReadmarkRecord],
    ) -> bool {
        self.save_write_readmarks_impl(records)
    }
    fn save_capture_audio_state(&mut self) -> Vec<u8> {
        self.audio.capture_save_state()
    }
    fn save_prepare_restore(&mut self) {
        self.save_prepare_restore_impl()
    }
    fn save_restore_audio_state(&mut self, audio_blob: &[u8]) {
        self.audio.restore_save_state(&self.vfs, audio_blob)
    }
    fn save_refresh_after_restore(&mut self) {
        self.save_refresh_after_restore_impl()
    }
    fn save_copy_slot(&mut self, src: i64, dst: i64) {
        self.save_copy_slot_impl(src, dst)
    }
    fn save_write_title(&mut self, slot: i64, title: &[u8]) {
        self.save_write_title_impl(slot, title)
    }
    fn save_read_thumbnail(&mut self, slot: i64, dst_page: u32, x: i32, y: i32) -> bool {
        let destination = base_page_handle(dst_page);
        let rect = self.save_read_thumbnail_impl(slot, dst_page, x, y);
        if let Some(rect) = rect {
            self.queue_logical_rect(destination, rect);
        }
        rect.is_some()
    }
    fn save_delete_slot(&mut self, slot: i64) {
        self.save_delete_slot_impl(slot)
    }
    fn save_read_meta(&mut self, slot: i64) -> String {
        self.save_read_meta_impl(slot)
    }
    fn save_get_timestamp(&mut self, slot: i64) -> String {
        self.save_get_timestamp_impl(slot)
    }
    fn save_get_description(&mut self) -> String {
        self.save_get_description_impl()
    }
    fn save_make_thumbnail(&mut self, w: u32, h: u32, src_page: Option<u32>) {
        self.save_make_thumbnail_impl(w, h, src_page)
    }
    fn save_blit_thumbnail(&mut self, dst_page: u32, x: i32, y: i32) -> bool {
        let before = self.page_revision(dst_page);
        let changed = self.save_blit_thumbnail_impl(dst_page, x, y);
        if changed {
            let (width, height) = self
                .save_thumbnail
                .as_ref()
                .map(|thumbnail| (thumbnail.width as i32, thumbnail.height as i32))
                .unwrap_or((0, 0));
            self.queue_logical_rect_if_changed(before, dst_page, [x, y, width, height]);
        }
        changed
    }
    fn save_free_data(&mut self) {
        self.save_free_data_impl()
    }
    fn save_begin_serialize(&mut self) -> i32 {
        self.save_begin_serialize_impl()
    }
    fn save_set_description(&mut self, description: &[u8]) {
        self.save_set_description_impl(description)
    }
    fn localize_save_description<'a>(
        &self,
        description: &'a [u8],
    ) -> std::borrow::Cow<'a, [u8]> {
        self.localize_save_description_impl(description)
    }
    fn localized_first_line_enabled(&self) -> bool {
        self.patch.as_ref().is_some_and(|patch| patch.one_way_save_titles())
    }
    fn take_text_line_localized_parts(&mut self) -> Vec<vm::host::LocalizedLinePart> {
        std::mem::take(&mut self.last_localized_parts)
    }
    fn translate_popup_name(&self, name: &[u8]) -> Option<String> {
        crate::patch::localize_display_message(&self.ir_display_messages, name)
    }
    fn scene_unload_and_exit(&mut self) {
        self.scene_unload_and_exit_impl()
    }
    fn selected_font_face(&mut self) -> Vec<u8> {
        self.selected_font_face.clone()
    }
    fn set_selected_font_face(&mut self, face: &[u8], mode: i32) {
        self.selected_font_face.clear();
        self.selected_font_face
            .extend_from_slice(face.split(|byte| *byte == 0).next().unwrap_or(face));
        self.selected_font_mode = mode;
        vm::text_trace!(
            "[HOST] SELFONT_STASH mode={} face={:?}", mode, String::from_utf8_lossy(&
            self.selected_font_face)
        );
    }
    fn select_font_face(&mut self, current: &[u8], mode: i32) -> Vec<u8> {
        self.set_selected_font_face(current, mode);
        self.selected_font_face.clone()
    }
    fn load_script(&mut self, name: &str) -> Option<LoadedScript> {
        self.load_script_impl(name)
    }
    fn generate_key(&mut self, key_str: &[u8]) {
        self.generate_key_impl(key_str)
    }
    fn pic_unpack(&mut self, filename: &[u8]) {
        self.pic_unpack_impl(filename)
    }
    fn pic_unpack_into(
        &mut self,
        dst_page: u32,
        filename: &[u8],
        dst_x: i32,
        dst_y: i32,
    ) {
        self.pic_unpack_into_impl(dst_page, filename, dst_x, dst_y)
    }
    fn page_set_draw_mode(&mut self, page: u32, mode: u32) {
        self.page_set_draw_mode_impl(page, mode)
    }
    fn page_create_file(&mut self, filename: &[u8]) -> u32 {
        self.page_create_file_impl(filename)
    }
    fn page_create_file_alpha(&mut self, filename: &[u8]) -> u32 {
        self.page_create_file_alpha_impl(filename)
    }
    fn pic_get_width(&mut self, filename: &[u8]) -> i32 {
        self.pic_get_width_impl(filename)
    }
    fn pic_get_height(&mut self, filename: &[u8]) -> i32 {
        self.pic_get_height_impl(filename)
    }
    fn sprite_create_file(&mut self, filename: &[u8]) -> u32 {
        self.sprite_create_file_impl(filename)
    }
    fn sprite_create_raw(&mut self, count: usize, args: &[vm::value::Value]) -> u32 {
        self.sprite_create_raw_impl(count, args)
    }
    fn sprite_create_file_raw(
        &mut self,
        count: usize,
        args: &[vm::value::Value],
    ) -> u32 {
        self.sprite_create_file_raw_impl(count, args)
    }
    fn sprite_set_clip(&mut self, sprite: u32, x: i32, y: i32, width: i32, height: i32) {
        self.sprite_set_clip_impl(sprite, x, y, width, height)
    }
    fn sprite_paste(
        &mut self,
        sprite: u32,
        dst_page: u32,
        position: Option<(i32, i32)>,
    ) {
        let destination = base_page_handle(dst_page);
        let before = self.page_revision(destination);
        let replay = self.sprite_paste_impl(sprite, dst_page, position);
        let rect = self
            .pages
            .get(&destination)
            .map(|page| [0, 0, page.width as i32, page.height as i32])
            .unwrap_or([0; 4]);
        if let Some(replay) = replay {
            let mode = match replay.draw_mode {
                1 => crate::render_model::PageCopyMode::Additive,
                3 => crate::render_model::PageCopyMode::SpriteMultiply,
                4 => crate::render_model::PageCopyMode::Lighten,
                _ => crate::render_model::PageCopyMode::Alpha,
            };
            let parameter = if mode == crate::render_model::PageCopyMode::Alpha {
                i32::from(255 - replay.opacity)
            } else {
                i32::from(replay.opacity)
            };
            let operation = if let Some((angle_degrees, scale_x, scale_y)) = replay
                .transform
            {
                PageOp::TransformCopy {
                    source: replay.source,
                    source_is_presented_scene: replay.source_is_presented_scene,
                    destination,
                    source_rect: replay.source_rect,
                    destination_rect: replay.destination_rect,
                    clip_rect: rect,
                    angle_degrees,
                    scale_x,
                    scale_y,
                    linear_filter: false,
                    mode,
                    parameter,
                    source_has_alpha: replay.source_has_alpha,
                    destination_has_alpha: replay.destination_has_alpha,
                }
            } else {
                PageOp::Copy {
                    source: replay.source,
                    source_is_presented_scene: replay.source_is_presented_scene,
                    destination,
                    source_rect: replay.source_rect,
                    destination_rect: replay.destination_rect,
                    mode,
                    parameter,
                    source_has_alpha: replay.source_has_alpha,
                    destination_has_alpha: replay.destination_has_alpha,
                    source_alpha_view: false,
                    destination_alpha_view: false,
                }
            };
            self.queue_page_op_if_changed(before, destination, operation);
        } else {
            self.queue_logical_rect_if_changed(before, destination, rect);
        }
        if before != self.page_revision(destination) {
            self.queue_alpha_copy_mirrors(
                destination,
                destination,
                rect,
                rect,
                crate::render_model::PageCopyMode::AlphaPlane,
            );
        }
    }
    fn sprite_set_alpha(&mut self, sprite: u32, alpha: u8) {
        self.sprite_set_alpha_impl(sprite, alpha)
    }
    fn sprite_mark_overlay(&mut self, sprite: u32) {
        self.sprite_mark_overlay_impl(sprite)
    }
    fn sprite_move(&mut self, sprite: u32, x: i32, y: i32, timing: Option<i32>) {
        self.sprite_move_impl(sprite, x, y, timing)
    }
    fn sprite_is_moving(&mut self, sprite: u32) -> bool {
        self.sprite_is_moving_impl(sprite)
    }
    fn sprite_is_alpha_animating(&mut self, sprite: u32) -> bool {
        self.sprite_is_alpha_animating_impl(sprite)
    }
    fn sprite_is_frame_animating(&mut self, sprite: u32) -> bool {
        self.sprite_is_frame_animating_impl(sprite)
    }
    fn sprite_priority_high(&mut self, sprite: u32) {
        self.sprite_priority_high_impl(sprite)
    }
    fn sprite_priority_high_group(&mut self, sprite: u32, reference: Option<u32>) {
        self.sprite_priority_high_group_impl(sprite, reference)
    }
    fn sprite_priority_high_single(&mut self, sprite: u32, reference: Option<u32>) {
        self.sprite_priority_high_single_impl(sprite, reference)
    }
    fn sprite_priority_low(&mut self, sprite: u32) {
        self.sprite_priority_low_impl(sprite)
    }
    fn sprite_priority_low_group(&mut self, sprite: u32, reference: Option<u32>) {
        self.sprite_priority_low_group_impl(sprite, reference)
    }
    fn sprite_get_page(&mut self, sprite: u32) -> u32 {
        self.sprite_get_page_impl(sprite)
    }
    fn sprite_width(&mut self, sprite: u32) -> i32 {
        self.sprite_width_impl(sprite)
    }
    fn sprite_height(&mut self, sprite: u32) -> i32 {
        self.sprite_height_impl(sprite)
    }
    fn sprite_pos_x(&mut self, sprite: u32) -> i32 {
        self.sprite_pos_x_impl(sprite)
    }
    fn sprite_pos_y(&mut self, sprite: u32) -> i32 {
        self.sprite_pos_y_impl(sprite)
    }
    fn set_fullscreen(&mut self, fullscreen: bool) {
        self.set_fullscreen_impl(fullscreen)
    }
    fn is_fullscreen(&mut self) -> bool {
        self.is_fullscreen_impl()
    }
    fn config_load(&mut self, name: &[u8], default: i32) -> i32 {
        self.config_load_impl(name, default)
    }
    fn config_store(&mut self, name: &[u8], value: i32) {
        self.config_store_impl(name, value)
    }
    fn get_effect_speed(&mut self) -> i32 {
        self.get_effect_speed_impl()
    }
    fn set_effect_speed(&mut self, speed: i32) {
        self.set_effect_speed_impl(speed)
    }
    fn config_set_autospeed(&mut self, value: i32) {
        self.config_set_autospeed_impl(value)
    }
    fn get_voice_enable_flag(&mut self) -> bool {
        self.get_voice_enable_flag_impl()
    }
    fn get_global_f(&mut self) -> i32 {
        self.get_voice_panning_flag_impl()
    }
    fn set_voice_ts_flag(&mut self) {
        self.set_voice_ts_flag_impl()
    }
    fn invalidate_page(&mut self, page: u32, rect: Option<(i32, i32, i32, i32)>) {
        self.invalidate_page_impl(page, rect)
    }
    fn display_sync(&mut self, mode: i32) -> i32 {
        self.display_sync_impl(mode)
    }
    fn set_input_dispatch_mode(&mut self, mode: i32) {
        self.set_input_dispatch_mode_impl(mode)
    }
    fn trigger_middle_input_pulse(&mut self) {
        self.trigger_middle_input_pulse_impl()
    }
    fn take_display_mode(&mut self) -> i32 {
        self.take_display_mode_impl()
    }
    fn register_picture_hash(&mut self, hash: u32) -> bool {
        self.register_picture_hash_impl(hash)
    }
    fn picture_hash_registered(&mut self, hash: u32) -> bool {
        self.picture_hash_registered_impl(hash)
    }
    fn sprite_priority_low_single(&mut self, sprite: u32, reference: Option<u32>) {
        self.sprite_priority_low_single_impl(sprite, reference)
    }
    fn set_frontbuffer(&mut self, page: u32) -> u32 {
        self.set_frontbuffer_impl(page)
    }
    fn page_get_width(&mut self, page: u32) -> i32 {
        self.page_get_width_impl(page)
    }
    fn page_get_height(&mut self, page: u32) -> i32 {
        self.page_get_height_impl(page)
    }
    fn page_get_pixel(&mut self, page: u32, x: i32, y: i32) -> i32 {
        self.page_get_pixel_impl(page, x, y)
    }
    fn page_get_alpha(&mut self, page: u32) -> u32 {
        self.page_get_alpha_impl(page)
    }
    fn page_release(&mut self, page: u32) {
        self.page_release_impl(page)
    }
    fn page_create(&mut self, w: i32, h: i32, indexed: bool) -> u32 {
        self.page_create_impl(w, h, indexed)
    }
    fn page_create_with_antidata(&mut self, w: i32, h: i32, indexed: bool) -> u32 {
        self.page_create_with_antidata_impl(w, h, indexed)
    }
    fn grp_boxfill(&mut self, page: u32, x: i32, y: i32, w: i32, h: i32, color: u32) {
        let before = self.page_revision(page);
        self.grp_boxfill_impl(page, x, y, w, h, color);
        let base = base_page_handle(page);
        let mode = if !is_alpha_page(page) && self.alpha_bindings.contains_key(&base) {
            crate::render_model::PageFillMode::ColourOnly
        } else {
            crate::render_model::PageFillMode::Replace
        };
        self.queue_page_op_if_changed(
            before,
            page,
            PageOp::Fill {
                destination: base,
                rect: [x, y, w, h],
                color,
                mode,
                parameter: i32::from(is_alpha_page(page)),
            },
        );
        if before != self.page_revision(page) {
            self.queue_alpha_fill_mirrors(page, [x, y, w, h], color, None);
        }
    }
    fn grp_copy(
        &mut self,
        src_page: u32,
        src_x: i32,
        src_y: i32,
        w: i32,
        h: i32,
        dst_page: u32,
        dst_x: i32,
        dst_y: i32,
    ) {
        let before = self.page_revision(dst_page);
        let source_is_presented_scene = self.reads_synthesized_frontbuffer(src_page);
        let source_has_alpha = self.page_has_alpha_semantics(src_page);
        let destination_has_alpha = self.page_has_alpha_semantics(dst_page);
        self.grp_copy_impl(src_page, src_x, src_y, w, h, dst_page, dst_x, dst_y);
        self.queue_page_op_if_changed(
            before,
            dst_page,
            PageOp::Copy {
                source: base_page_handle(src_page),
                source_is_presented_scene,
                destination: base_page_handle(dst_page),
                source_rect: [src_x, src_y, w, h],
                destination_rect: [dst_x, dst_y, w, h],
                mode: crate::render_model::PageCopyMode::Replace,
                parameter: 0,
                source_has_alpha,
                destination_has_alpha,
                source_alpha_view: is_alpha_page(src_page),
                destination_alpha_view: is_alpha_page(dst_page),
            },
        );
        if before != self.page_revision(dst_page) {
            self.queue_alpha_copy_mirrors(
                src_page,
                dst_page,
                [src_x, src_y, w, h],
                [dst_x, dst_y, w, h],
                crate::render_model::PageCopyMode::AlphaPlane,
            );
        }
    }
    fn grp_extcopy(
        &mut self,
        src_page: u32,
        src_x: i32,
        src_y: i32,
        w: i32,
        h: i32,
        dst_page: u32,
        dst_x: i32,
        dst_y: i32,
        alpha: i32,
    ) {
        let before = self.page_revision(dst_page);
        let source_is_presented_scene = self.reads_synthesized_frontbuffer(src_page);
        let source_has_alpha = self.page_has_alpha_semantics(src_page);
        let destination_has_alpha = self.page_has_alpha_semantics(dst_page);
        let text_presentation_copy = self
            .current_text_presentation_copy(
                src_page,
                [src_x, src_y, w, h],
                dst_page,
                [dst_x, dst_y, w, h],
                alpha,
            );
        self.grp_extcopy_impl(
            src_page,
            src_x,
            src_y,
            w,
            h,
            dst_page,
            dst_x,
            dst_y,
            alpha,
        );
        if let Some((cropped, finished)) = text_presentation_copy {
            if let Some(cropped) = cropped {
                self.queue_page_op_if_changed(
                    before,
                    dst_page,
                    PageOp::PresentationOverlay {
                        destination: base_page_handle(dst_page),
                        physical_x: cropped.physical_x,
                        physical_y: cropped.physical_y,
                        width: cropped.width,
                        height: cropped.height,
                        pixels: cropped.pixels,
                        destination_has_alpha,
                    },
                );
            }
            if finished {
                self.active_text_presentation = None;
            }
        } else {
            self.queue_page_op_if_changed(
                before,
                dst_page,
                PageOp::Copy {
                    source: base_page_handle(src_page),
                    source_is_presented_scene,
                    destination: base_page_handle(dst_page),
                    source_rect: [src_x, src_y, w, h],
                    destination_rect: [dst_x, dst_y, w, h],
                    mode: crate::render_model::PageCopyMode::Alpha,
                    parameter: alpha,
                    source_has_alpha,
                    destination_has_alpha,
                    source_alpha_view: false,
                    destination_alpha_view: false,
                },
            );
        }
    }
    fn grp_mulcopy(
        &mut self,
        src_page: u32,
        src_x: i32,
        src_y: i32,
        w: i32,
        h: i32,
        dst_page: u32,
        dst_x: i32,
        dst_y: i32,
    ) {
        let before = self.page_revision(dst_page);
        let source_is_presented_scene = self.reads_synthesized_frontbuffer(src_page);
        let source_has_alpha = self.page_has_alpha_semantics(src_page);
        let destination_has_alpha = self.page_has_alpha_semantics(dst_page);
        self.grp_mulcopy_impl(src_page, src_x, src_y, w, h, dst_page, dst_x, dst_y);
        self.queue_page_op_if_changed(
            before,
            dst_page,
            PageOp::Copy {
                source: base_page_handle(src_page),
                source_is_presented_scene,
                destination: base_page_handle(dst_page),
                source_rect: [src_x, src_y, w, h],
                destination_rect: [dst_x, dst_y, w, h],
                mode: crate::render_model::PageCopyMode::Multiply,
                parameter: 0,
                source_has_alpha,
                destination_has_alpha,
                source_alpha_view: false,
                destination_alpha_view: false,
            },
        );
        if before != self.page_revision(dst_page) {
            self.queue_alpha_copy_mirrors(
                src_page,
                dst_page,
                [src_x, src_y, w, h],
                [dst_x, dst_y, w, h],
                crate::render_model::PageCopyMode::AlphaMultiply,
            );
        }
    }
    fn grp_point_set(&mut self, page: u32, x: i32, y: i32, color: u32) {
        let before = self.page_revision(page);
        self.grp_point_set_impl(page, x, y, color);
        self.queue_point_op_if_changed(
            before,
            page,
            crate::render_model::PagePoint {
                x,
                y,
                color,
                alpha_view: is_alpha_page(page),
            },
        );
        if before != self.page_revision(page) {
            self.queue_alpha_fill_mirrors(page, [x, y, 1, 1], color, None);
        }
    }
    fn grp_reverse(&mut self, page: u32, x: i32, y: i32, w: i32, h: i32) {
        let before = self.page_revision(page);
        self.grp_reverse_impl(page, x, y, w, h);
        self.queue_page_op_if_changed(
            before,
            page,
            PageOp::Color {
                destination: base_page_handle(page),
                rect: [x, y, w, h],
                mode: crate::render_model::PageColorMode::Invert,
                color_a: 0,
                color_b: 0,
                parameter: 0,
            },
        )
    }
    fn grp_mulboxfill(&mut self, page: u32, x: i32, y: i32, w: i32, h: i32, color: u32) {
        let before = self.page_revision(page);
        self.grp_mulboxfill_impl(page, x, y, w, h, color);
        self.queue_page_op_if_changed(
            before,
            page,
            PageOp::Fill {
                destination: base_page_handle(page),
                rect: [x, y, w, h],
                color,
                mode: crate::render_model::PageFillMode::Multiply,
                parameter: 0,
            },
        )
    }
    fn grp_alphablend(&mut self, page: u32, x: i32, y: i32, w: i32, h: i32, color: u32) {
        let before = self.page_revision(page);
        self.grp_alphablend_impl(page, x, y, w, h, color);
        self.queue_page_op_if_changed(
            before,
            page,
            PageOp::Fill {
                destination: base_page_handle(page),
                rect: [x, y, w, h],
                color,
                mode: crate::render_model::PageFillMode::Overlay,
                parameter: 0,
            },
        )
    }
    fn grp_sepia(
        &mut self,
        page: u32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        dark_color: u32,
        light_color: u32,
        mix: Option<i32>,
    ) {
        let before = self.page_revision(page);
        self.grp_sepia_impl(page, x, y, w, h, dark_color, light_color, mix);
        self.queue_page_op_if_changed(
            before,
            page,
            PageOp::Color {
                destination: base_page_handle(page),
                rect: [x, y, w, h],
                mode: crate::render_model::PageColorMode::Sepia,
                color_a: dark_color,
                color_b: light_color,
                parameter: mix.unwrap_or(-1),
            },
        )
    }
    fn grp_extboxfill(
        &mut self,
        page: u32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        color: u32,
        alpha: i32,
    ) {
        let before = self.page_revision(page);
        self.grp_extboxfill_impl(page, x, y, w, h, color, alpha);
        self.queue_page_op_if_changed(
            before,
            page,
            PageOp::Fill {
                destination: base_page_handle(page),
                rect: [x, y, w, h],
                color,
                mode: crate::render_model::PageFillMode::Blend,
                parameter: alpha,
            },
        );
        if before != self.page_revision(page) {
            self.queue_alpha_fill_mirrors(page, [x, y, w, h], color, Some(alpha));
        }
    }
    fn grp_revmulcopy(
        &mut self,
        src_page: u32,
        src_x: i32,
        src_y: i32,
        w: i32,
        h: i32,
        dst_page: u32,
        dst_x: i32,
        dst_y: i32,
    ) {
        let before = self.page_revision(dst_page);
        let source_is_presented_scene = self.reads_synthesized_frontbuffer(src_page);
        let source_has_alpha = self.page_has_alpha_semantics(src_page);
        let destination_has_alpha = self.page_has_alpha_semantics(dst_page);
        self.grp_revmulcopy_impl(src_page, src_x, src_y, w, h, dst_page, dst_x, dst_y);
        self.queue_page_op_if_changed(
            before,
            dst_page,
            PageOp::Copy {
                source: base_page_handle(src_page),
                source_is_presented_scene,
                destination: base_page_handle(dst_page),
                source_rect: [src_x, src_y, w, h],
                destination_rect: [dst_x, dst_y, w, h],
                mode: crate::render_model::PageCopyMode::ReverseMultiply,
                parameter: 0,
                source_has_alpha,
                destination_has_alpha,
                source_alpha_view: false,
                destination_alpha_view: false,
            },
        );
        if before != self.page_revision(dst_page) {
            self.queue_alpha_copy_mirrors(
                src_page,
                dst_page,
                [src_x, src_y, w, h],
                [dst_x, dst_y, w, h],
                crate::render_model::PageCopyMode::AlphaReverseMultiply,
            );
        }
    }
    fn grp_swap(
        &mut self,
        src_page: u32,
        src_x: i32,
        src_y: i32,
        w: i32,
        h: i32,
        dst_page: u32,
        dst_x: i32,
        dst_y: i32,
    ) {
        let source_before = self.page_revision(src_page);
        let destination_before = self.page_revision(dst_page);
        let source_is_presented_scene = self.reads_synthesized_frontbuffer(src_page);
        let destination_is_presented_scene = self
            .reads_synthesized_frontbuffer(dst_page);
        let swap_alpha = self.alpha_bindings.contains_key(&base_page_handle(src_page))
            && self.alpha_bindings.contains_key(&base_page_handle(dst_page));
        self.grp_swap_impl(src_page, src_x, src_y, w, h, dst_page, dst_x, dst_y);
        let source = base_page_handle(src_page);
        let destination = base_page_handle(dst_page);
        if source_before != self.page_revision(source)
            || destination_before != self.page_revision(destination)
        {
            if source_is_presented_scene || destination_is_presented_scene {
                self.queue_logical_rect_if_changed(
                    source_before,
                    source,
                    [src_x, src_y, w, h],
                );
                self.queue_logical_rect_if_changed(
                    destination_before,
                    destination,
                    [dst_x, dst_y, w, h],
                );
                return;
            }
            self.pending_page_ops
                .push(PageOp::Swap {
                    source,
                    destination,
                    source_rect: [src_x, src_y, w, h],
                    destination_x: dst_x,
                    destination_y: dst_y,
                    swap_alpha,
                });
            self.page_op_destinations.insert(source);
            self.page_op_destinations.insert(destination);
        }
    }
    fn page_set_antidata(&mut self, page: u32, alpha_page: u32) {
        let before = self.page_revision(page);
        self.page_set_antidata_impl(page, alpha_page);
        let (width, height) = self
            .pages
            .get(&base_page_handle(page))
            .map(|page| (page.width as i32, page.height as i32))
            .unwrap_or((0, 0));
        if base_page_handle(alpha_page) == 0 {
            self.queue_page_op_if_changed(
                before,
                page,
                PageOp::Fill {
                    destination: base_page_handle(page),
                    rect: [0, 0, width, height],
                    color: 0,
                    mode: crate::render_model::PageFillMode::AlphaReplace,
                    parameter: 0,
                },
            );
        } else {
            self.queue_page_op_if_changed(
                before,
                page,
                PageOp::Copy {
                    source: base_page_handle(alpha_page),
                    source_is_presented_scene: false,
                    destination: base_page_handle(page),
                    source_rect: [0, 0, width, height],
                    destination_rect: [0, 0, width, height],
                    mode: crate::render_model::PageCopyMode::AlphaPlane,
                    parameter: 0,
                    source_has_alpha: false,
                    destination_has_alpha: true,
                    source_alpha_view: false,
                    destination_alpha_view: true,
                },
            );
        }
    }
    fn get_render_page(&mut self) -> i32 {
        self.get_render_page_impl()
    }
    fn grp_modify_copy(
        &mut self,
        dst_page: u32,
        dst_x: i32,
        dst_y: i32,
        dst_w: i32,
        dst_h: i32,
        src_page: u32,
        src_x: i32,
        src_y: i32,
        src_w: i32,
        src_h: i32,
        angle_degrees: f32,
        scale_x: f32,
        scale_y: f32,
    ) {
        let destination = base_page_handle(dst_page);
        let before = self.page_revision(destination);
        let source_is_presented_scene = self.reads_synthesized_frontbuffer(src_page);
        let source_has_alpha = self.page_has_alpha_semantics(src_page);
        let destination_has_alpha = self.page_has_alpha_semantics(dst_page);
        self.grp_modify_copy_impl(
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
            angle_degrees,
            scale_x,
            scale_y,
        );
        let centred_x = dst_x
            .wrapping_add(dst_w.wrapping_sub(src_w).wrapping_add(1) / 2);
        let centred_y = dst_y
            .wrapping_add(dst_h.wrapping_sub(src_h).wrapping_add(1) / 2);
        let angle_milli = (f64::from(angle_degrees) * 1000.0).trunc() as i32;
        let linear_filter = angle_milli % 360_000 == 0
            && (scale_x != 1.0 || scale_y != 1.0) && scale_x > 0.0 && scale_y > 0.0;
        self.queue_page_op_if_changed(
            before,
            destination,
            PageOp::TransformCopy {
                source: base_page_handle(src_page),
                source_is_presented_scene,
                destination,
                source_rect: [src_x, src_y, src_w, src_h],
                destination_rect: [centred_x, centred_y, src_w, src_h],
                clip_rect: [dst_x, dst_y, dst_w, dst_h],
                angle_degrees,
                scale_x,
                scale_y,
                linear_filter,
                mode: crate::render_model::PageCopyMode::Replace,
                parameter: 0,
                source_has_alpha,
                destination_has_alpha,
            },
        );
        if before != self.page_revision(destination) {
            let rect = [dst_x, dst_y, dst_w, dst_h];
            self.queue_alpha_copy_mirrors(
                destination,
                destination,
                rect,
                rect,
                crate::render_model::PageCopyMode::AlphaPlane,
            );
        }
    }
    fn grp_make_mosaic(
        &mut self,
        src_page: u32,
        src_x: i32,
        src_y: i32,
        width: i32,
        height: i32,
        dst_page: u32,
        dst_x: i32,
        dst_y: i32,
        block_size: i32,
        rng_state: u32,
    ) -> u32 {
        let before = self.page_revision(dst_page);
        let result = self
            .grp_make_mosaic_impl(
                src_page,
                src_x,
                src_y,
                width,
                height,
                dst_page,
                dst_x,
                dst_y,
                block_size,
                rng_state,
            );
        self.queue_logical_rect_if_changed(
            before,
            dst_page,
            [dst_x, dst_y, width, height],
        );
        result
    }
    fn grp_modcopy(
        &mut self,
        dst_page: u32,
        dst_x: i32,
        dst_y: i32,
        dst_w: i32,
        dst_h: i32,
        src_page: u32,
        src_x: i32,
        src_y: i32,
        src_w: i32,
        src_h: i32,
    ) {
        let before = self.page_revision(dst_page);
        let source_is_presented_scene = self.reads_synthesized_frontbuffer(src_page);
        let source_has_alpha = self.page_has_alpha_semantics(src_page);
        let destination_has_alpha = self.page_has_alpha_semantics(dst_page);
        self.grp_modcopy_impl(
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
        self.queue_page_op_if_changed(
            before,
            dst_page,
            PageOp::Copy {
                source: base_page_handle(src_page),
                source_is_presented_scene,
                destination: base_page_handle(dst_page),
                source_rect: [src_x, src_y, src_w, src_h],
                destination_rect: [dst_x, dst_y, dst_w, dst_h],
                mode: crate::render_model::PageCopyMode::Replace,
                parameter: 0,
                source_has_alpha,
                destination_has_alpha,
                source_alpha_view: false,
                destination_alpha_view: false,
            },
        )
    }
    fn get_timestamp(&mut self) -> i32 {
        self.get_timestamp_impl()
    }
    fn get_render_width(&mut self) -> i32 {
        self.internal_w as i32
    }
    fn get_render_height(&mut self) -> i32 {
        self.internal_h as i32
    }
    fn begin_scheduler_tick(&mut self) {
        self.begin_scheduler_tick_impl()
    }
    fn set_native_time_scale(&mut self, scale: f32) {
        self.set_native_time_scale_impl(scale)
    }
    fn cooperative_timers(&self) -> bool {
        self.cooperative_timers_impl()
    }
    fn sprite_release(&mut self, sprite: u32) {
        self.sprite_release_impl(sprite)
    }
    fn sprite_visibility_push(&mut self, sprite: Option<u32>) {
        self.sprite_visibility_push_impl(sprite)
    }
    fn sprite_visibility_pop(&mut self, sprite: Option<u32>) {
        self.sprite_visibility_pop_impl(sprite)
    }
    fn sprite_page_auto_release(&mut self, sprite: u32) {
        self.sprite_page_auto_release_impl(sprite)
    }
    fn sprite_set_smooth_animation(&mut self, sprite: u32) {
        self.sprite_set_smooth_animation_impl(sprite)
    }
    fn sprite_rotate(
        &mut self,
        sprite: u32,
        target_degrees: f32,
        duration_ms: i32,
        extrapolate: bool,
    ) {
        self.sprite_rotate_impl(sprite, target_degrees, duration_ms, extrapolate)
    }
    fn sprite_xmodify_define(&mut self, sprite: u32, keyframes: &[(f32, i32)]) {
        self.sprite_xmodify_define_impl(sprite, keyframes)
    }
    fn sprite_xmodify_set(&mut self, sprite: u32, target: f32, duration_ms: i32) {
        self.sprite_xmodify_set_impl(sprite, target, duration_ms)
    }
    fn sprite_ymodify_define(&mut self, sprite: u32, keyframes: &[(f32, i32)]) {
        self.sprite_ymodify_define_impl(sprite, keyframes)
    }
    fn sprite_ymodify_set(&mut self, sprite: u32, target: f32, duration_ms: i32) {
        self.sprite_ymodify_set_impl(sprite, target, duration_ms)
    }
    fn make_alfa_table(
        &mut self,
        source_page: u32,
        destination_page: u32,
        table: &[u8; 256],
    ) {
        let destination = base_page_handle(destination_page);
        let before = self.page_revision(destination);
        self.make_alfa_table_impl(source_page, destination_page, table);
        let rect = self
            .pages
            .get(&destination)
            .map(|page| [0, 0, page.width as i32, page.height as i32])
            .unwrap_or([0; 4]);
        self.queue_logical_rect_if_changed(before, destination, rect);
        if before != self.page_revision(destination) {
            self.queue_alpha_copy_mirrors(
                destination,
                destination,
                rect,
                rect,
                crate::render_model::PageCopyMode::AlphaPlane,
            );
        }
    }
    fn sprite_exists(&mut self, handle: u32) -> bool {
        self.sprite_exists_impl(handle)
    }
    fn sprite_alfa_set(&mut self, sprite: u32, alpha: i32, animate: i32) {
        self.sprite_alfa_set_impl(sprite, alpha, animate)
    }
    fn sprite_alfa_define(&mut self, sprite: u32, keyframes: &[(i32, i32)]) {
        self.sprite_alfa_define_impl(sprite, keyframes)
    }
    fn sprite_animate_define(&mut self, sprite: u32, keyframes: &[(i32, i32)]) {
        self.sprite_animate_define_impl(sprite, keyframes)
    }
    fn sprite_animate_define_aligned(
        &mut self,
        sprite: u32,
        keyframes: &[(i32, i32)],
        total_duration: i32,
    ) {
        self.sprite_animate_define_aligned_impl(sprite, keyframes, total_duration)
    }
    fn sprite_animate_add(&mut self, sprite: u32, keyframes: &[(i32, i32)]) {
        self.sprite_animate_add_impl(sprite, keyframes)
    }
    fn normalize_scheduler_delta(&mut self) {
        self.normalize_scheduler_delta_impl()
    }
    fn scene_freeze_begin(&mut self) {
        self.scene_freeze_begin_impl()
    }
    fn scene_freeze_end(&mut self) {
        self.scene_freeze_end_impl()
    }
    fn scene_freeze_active(&mut self) -> bool {
        self.scene_dirty_freeze != 0
    }
    fn present_epoch(&mut self) -> u64 {
        self.present_epoch_impl()
    }
    fn request_present(&mut self) {
        self.request_present_impl()
    }
    fn mouse_x(&mut self) -> i32 {
        self.mouse_x_impl()
    }
    fn mouse_y(&mut self) -> i32 {
        self.mouse_y_impl()
    }
    fn cursor_inside(&mut self) -> bool {
        self.cursor_inside_impl()
    }
    fn set_viewport_offset(&mut self, x: i32, y: i32) {
        self.set_viewport_offset_impl(x, y)
    }
    fn warp_cursor(&mut self, x: i32, y: i32) {
        self.warp_cursor_impl(x, y)
    }
    fn key_modifier_mask(&mut self) -> u32 {
        self.key_modifier_mask_impl()
    }
    fn recent_key_event_mask(&mut self, context_id: u32) -> u32 {
        self.recent_key_event_mask_impl(context_id)
    }
    fn scheduler_button_latches(&mut self, context_id: u32) -> u8 {
        self.scheduler_button_latches_impl(context_id)
    }
    fn consume_scheduler_button_latches(&mut self, context_id: u32, mask: u8) {
        self.consume_scheduler_button_latches_impl(context_id, mask)
    }
    fn four_key_wait_active(&mut self) -> bool {
        self.four_key_wait_active_impl()
    }
    fn key_capslock_on(&mut self) -> bool {
        self.key_capslock_on_impl()
    }
    fn key_shift_pressed(&mut self) -> bool {
        self.key_shift_pressed_impl()
    }
    fn input_suppression_bypassed(&mut self) -> bool {
        self.input_suppression_bypassed_impl()
    }
    fn input_is_held_or_recent(&mut self, index: usize) -> bool {
        self.input_is_held_or_recent_impl(index)
    }
    fn input_consume_recent_press(&mut self, index: usize) {
        self.input_consume_recent_press_impl(index)
    }
    fn set_native_mode_flags(&mut self, bits: u8, special: bool) {
        self.set_native_mode_flags_impl(bits, special)
    }
    fn input_dispatch_mode(&mut self) -> i32 {
        self.input_dispatch_mode
    }
    fn scene_set_origin(&mut self, x: i32, y: i32) {
        self.scene_set_origin_impl(x, y)
    }
    fn voice_check_file(&mut self, name: &[u8]) -> i32 {
        self.audio.voice_check_file(&self.vfs, name) as i32
    }
    fn voice_play(&mut self, channel: u8, name: &[u8], looped: bool) {
        let skip_active = self.effect_skip_active_impl();
        self.audio.voice_play(&self.vfs, channel, name, looped, skip_active)
    }
    fn voice_stop(&mut self, channel: u8) {
        self.audio.voice_stop(channel)
    }
    fn voice_set_vol(&mut self, channel: u8, volume: i32) {
        self.audio.voice_set_volume(channel, volume)
    }
    fn voice_set_pan(&mut self, channel: u8, pan: i32) {
        self.audio.voice_set_pan(channel, pan)
    }
    fn voice_fade(&mut self, channel: u8, target_volume: i32, duration_ms: i32) {
        self.audio
            .voice_fade(channel, target_volume, duration_ms, self.clock_timestamp_ms)
    }
    fn voice_fadeout(&mut self, channel: u8, duration_ms: i32) {
        self.audio.voice_fadeout(channel, duration_ms, self.clock_timestamp_ms)
    }
    fn voice_get_stat(&mut self, channel: u8) -> i32 {
        self.audio.voice_status(channel)
    }
    fn voice_get_filename(&mut self, channel: u8) -> Vec<u8> {
        self.audio.voice_filename(channel)
    }
    fn sound_play(&mut self, channel: u8, name: &[u8], looped: bool) {
        self.audio.sound_play(&self.vfs, channel, name, looped)
    }
    fn sound_stop(&mut self, channel: u8) {
        self.audio.sound_stop(channel)
    }
    fn sound_set_vol(&mut self, channel: u8, volume: i32) {
        self.audio.sound_set_volume(channel, volume)
    }
    fn sound_set_pan(&mut self, channel: u8, pan: i32) {
        self.audio.sound_set_pan(channel, pan)
    }
    fn sound_fade(&mut self, channel: u8, target_volume: i32, duration_ms: i32) {
        self.audio
            .sound_fade(channel, target_volume, duration_ms, self.clock_timestamp_ms)
    }
    fn sound_fadeout(&mut self, channel: u8, duration_ms: i32) {
        self.audio.sound_fadeout(channel, duration_ms, self.clock_timestamp_ms)
    }
    fn sound_get_stat(&mut self, channel: u8) -> i32 {
        self.audio.sound_status(channel)
    }
    fn sound_get_filename(&mut self, channel: u8) -> Vec<u8> {
        self.audio.sound_filename(channel)
    }
    fn bgm_play_or_stop(&mut self, name: &[u8], looped: bool) {
        self.audio.music_play(&self.vfs, name, &[], looped)
    }
    fn bgm_play(&mut self, name: &str, looped: bool) {
        self.audio.music_play(&self.vfs, name.as_bytes(), &[], looped)
    }
    fn bgm_stop(&mut self) {
        self.audio.music_stop()
    }
    fn music_get_status(&mut self) -> i32 {
        self.audio.music_status()
    }
    fn set_sound_master_volume(&mut self, value: i32) {
        self.audio.set_sound_master(value)
    }
    fn set_voice_master_volume(&mut self, value: i32) {
        self.audio.set_voice_master(value)
    }
    fn set_music_master_volume(&mut self, value: i32) {
        self.audio.set_music_master(value)
    }
    fn movie_play(
        &mut self,
        name: &[u8],
        visible: bool,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
    ) {
        self.movie
            .play(
                &self.vfs,
                name,
                MovieTarget {
                    visible,
                    x,
                    y,
                    width: w,
                    height: h,
                },
            );
    }
    fn movie_cleanup_present(&mut self) {
        eprintln!("[VIDEO] vm opcode movie-stop cleanup");
        self.movie.cleanup();
        self.render_dirty = true;
    }
    fn movie_apply_config(&mut self, value: i32) {
        self.movie.set_volume(value);
    }
    fn movie_is_playing(&mut self) -> bool {
        let (playing, frame_changed) = self.movie.is_playing(&mut self.audio);
        if frame_changed {
            self.render_dirty = true;
        }
        playing
    }
    fn movie_wait_event_flags(&mut self, context_id: u32) -> u8 {
        self.movie_wait_event_flags_impl(context_id)
    }
    fn movie_wait_cancel_pressed(&mut self) -> bool {
        self.movie_wait_cancel_pressed_impl()
    }
    fn clear_movie_wait_input(&mut self, context_id: u32) {
        self.clear_movie_wait_input_impl(context_id)
    }
    fn main_sound_play(&mut self, name: &[u8], looped: bool) {
        self.audio.anonymous_play(&self.vfs, 0, name, &[], looped)
    }
    fn main_sound_stop(&mut self) {
        self.audio.anonymous_stop(0)
    }
    fn native_music_play_dual(&mut self, primary: &[u8], secondary: &[u8]) {
        self.audio.music_play(&self.vfs, primary, secondary, true)
    }
    fn native_music_stop(&mut self) {
        self.audio.music_stop()
    }
    fn native_music_fadeout(&mut self, duration_ms: i32) {
        self.audio.music_fadeout(duration_ms, self.clock_timestamp_ms)
    }
    fn native_music_fade(&mut self, target_volume: i32, duration_ms: i32) {
        self.audio.music_fade(target_volume, duration_ms, self.clock_timestamp_ms)
    }
    fn native_music_pause(&mut self) {
        self.audio.music_pause()
    }
    fn native_music_resume(&mut self) {
        self.audio.music_resume()
    }
    fn native_music_snapshot(&mut self) {
        self.audio.music_snapshot()
    }
    fn native_music_restore(&mut self) {
        self.audio.music_restore(&self.vfs)
    }
    fn native_music_replace(&mut self, primary: &[u8], secondary: &[u8], looped: bool) {
        self.audio.music_replace(&self.vfs, primary, secondary, looped)
    }
    fn native_music_clear_end_state(&mut self) {
        self.audio.music_clear_end_state()
    }
    fn native_music_set_frequency(&mut self, frequency: i32) {
        self.audio.music_set_frequency(frequency)
    }
    fn native_music_set_volume(&mut self, volume: i32) {
        self.audio.music_set_volume(volume)
    }
    fn voice_aux_play(&mut self, name: &[u8], looped: bool) {
        let skip_active = self.effect_skip_active_impl();
        self.audio.aux_play(&self.vfs, name, looped, skip_active)
    }
    fn text_voice_play(&mut self, _channel: u8, name: &[u8], _mode: i32) {
        let voice_ts_flag = self.voice_ts_flag;
        self.voice_ts_flag = self.config_load_impl(b"VoiceSkipDef", 0) != 0;
        if name.eq_ignore_ascii_case(b"##nw##") {
            return;
        }
        if voice_ts_flag || !self.get_voice_enable_flag_impl() {
            self.audio.aux_stop();
            return;
        }
        let looped = false;
        let skip_active = self.effect_skip_active_impl();
        self.audio.aux_play(&self.vfs, name, looped, skip_active)
    }
    fn voice_aux_get_stat(&mut self) -> i32 {
        self.audio.aux_status()
    }
    fn native_music_aux_stop(&mut self) {
        self.audio.aux_stop()
    }
    fn native_music_aux_set_volume(&mut self, volume: i32) {
        self.audio.aux_set_volume(volume)
    }
    fn native_music_aux_set_pan(&mut self, pan: i32) {
        self.audio.aux_set_pan(pan)
    }
    fn native_music_aux_fade(&mut self, target_volume: i32, duration_ms: i32) {
        self.audio.aux_fade(target_volume, duration_ms, self.clock_timestamp_ms)
    }
    fn native_music_aux_fadeout(&mut self, duration_ms: i32) {
        self.audio.aux_fadeout(duration_ms, self.clock_timestamp_ms)
    }
    fn native_music_aux_filename(&mut self) -> Vec<u8> {
        self.audio.aux_filename()
    }
    fn native_sound_a_play_dual(&mut self, primary: &[u8], secondary: &[u8]) {
        self.audio.anonymous_play(&self.vfs, 0, primary, secondary, true)
    }
    fn native_sound_a_stop(&mut self) {
        self.audio.anonymous_stop(0)
    }
    fn native_sound_a_set_volume(&mut self, volume: i32) {
        self.audio.anonymous_set_volume(0, volume)
    }
    fn native_sound_a_set_pan(&mut self, pan: i32) {
        self.audio.anonymous_set_pan(0, pan)
    }
    fn native_sound_a_fade(&mut self, target_volume: i32, duration_ms: i32) {
        self.audio.anonymous_fade(0, target_volume, duration_ms, self.clock_timestamp_ms)
    }
    fn native_sound_a_fadeout(&mut self, duration_ms: i32) {
        self.audio.anonymous_fadeout(0, duration_ms, self.clock_timestamp_ms)
    }
    fn native_sound_a_status(&mut self) -> i32 {
        self.audio.anonymous_status(0)
    }
    fn native_sound_a_filename(&mut self) -> Vec<u8> {
        self.audio.anonymous_filename(0)
    }
    fn native_sound_a_flag(&mut self) -> i32 {
        self.audio.anonymous_flag(0)
    }
    fn native_sound_b_play(&mut self, primary: &[u8], looped: bool) {
        self.audio.anonymous_play(&self.vfs, 1, primary, &[], looped)
    }
    fn native_sound_b_play_dual(&mut self, primary: &[u8], secondary: &[u8]) {
        self.audio.anonymous_play(&self.vfs, 1, primary, secondary, true)
    }
    fn native_sound_b_stop(&mut self) {
        self.audio.anonymous_stop(1)
    }
    fn native_sound_b_set_volume(&mut self, volume: i32) {
        self.audio.anonymous_set_volume(1, volume)
    }
    fn native_sound_b_set_pan(&mut self, pan: i32) {
        self.audio.anonymous_set_pan(1, pan)
    }
    fn native_sound_b_fade(&mut self, target_volume: i32, duration_ms: i32) {
        self.audio.anonymous_fade(1, target_volume, duration_ms, self.clock_timestamp_ms)
    }
    fn native_sound_b_fadeout(&mut self, duration_ms: i32) {
        self.audio.anonymous_fadeout(1, duration_ms, self.clock_timestamp_ms)
    }
    fn native_sound_b_status(&mut self) -> i32 {
        self.audio.anonymous_status(1)
    }
    fn native_sound_b_filename(&mut self) -> Vec<u8> {
        self.audio.anonymous_filename(1)
    }
    fn native_sound_b_flag(&mut self) -> i32 {
        self.audio.anonymous_flag(1)
    }
    fn native_sound_c_play(&mut self, primary: &[u8], looped: bool) {
        self.audio.anonymous_play(&self.vfs, 2, primary, &[], looped)
    }
    fn native_sound_c_play_dual(&mut self, primary: &[u8], secondary: &[u8]) {
        self.audio.anonymous_play(&self.vfs, 2, primary, secondary, true)
    }
    fn native_sound_c_stop(&mut self) {
        self.audio.anonymous_stop(2)
    }
    fn native_sound_c_set_volume(&mut self, volume: i32) {
        self.audio.anonymous_set_volume(2, volume)
    }
    fn native_sound_c_set_pan(&mut self, pan: i32) {
        self.audio.anonymous_set_pan(2, pan)
    }
    fn native_sound_c_fade(&mut self, target_volume: i32, duration_ms: i32) {
        self.audio.anonymous_fade(2, target_volume, duration_ms, self.clock_timestamp_ms)
    }
    fn native_sound_c_fadeout(&mut self, duration_ms: i32) {
        self.audio.anonymous_fadeout(2, duration_ms, self.clock_timestamp_ms)
    }
    fn native_sound_c_status(&mut self) -> i32 {
        self.audio.anonymous_status(2)
    }
    fn native_sound_c_filename(&mut self) -> Vec<u8> {
        self.audio.anonymous_filename(2)
    }
    fn native_sound_c_flag(&mut self) -> i32 {
        self.audio.anonymous_flag(2)
    }
    fn native_sound_d_play(&mut self, primary: &[u8], looped: bool) {
        self.audio.anonymous_play(&self.vfs, 3, primary, &[], looped)
    }
    fn native_sound_d_play_dual(&mut self, primary: &[u8], secondary: &[u8]) {
        self.audio.anonymous_play(&self.vfs, 3, primary, secondary, true)
    }
    fn native_sound_d_stop(&mut self) {
        self.audio.anonymous_stop(3)
    }
    fn native_sound_d_set_volume(&mut self, volume: i32) {
        self.audio.anonymous_set_volume(3, volume)
    }
    fn native_sound_d_set_pan(&mut self, pan: i32) {
        self.audio.anonymous_set_pan(3, pan)
    }
    fn native_sound_d_fade(&mut self, target_volume: i32, duration_ms: i32) {
        self.audio.anonymous_fade(3, target_volume, duration_ms, self.clock_timestamp_ms)
    }
    fn native_sound_d_fadeout(&mut self, duration_ms: i32) {
        self.audio.anonymous_fadeout(3, duration_ms, self.clock_timestamp_ms)
    }
    fn native_sound_d_status(&mut self) -> i32 {
        self.audio.anonymous_status(3)
    }
    fn native_sound_d_filename(&mut self) -> Vec<u8> {
        self.audio.anonymous_filename(3)
    }
    fn native_sound_d_flag(&mut self) -> i32 {
        self.audio.anonymous_flag(3)
    }
    fn effect_skip_active(&mut self) -> bool {
        self.effect_skip_active_impl()
    }
    fn set_click_suppression_mask(&mut self, mask: i32) {
        self.set_click_suppression_mask_impl(mask)
    }
    fn cursor_show_or_hide(&mut self, show: bool) -> bool {
        self.cursor_show_or_hide_impl(show)
    }
    fn set_capslock_state(&mut self, desired_on: bool) {
        self.set_capslock_state_impl(desired_on)
    }
    fn add_hotspot(&mut self, context_id: u32, frame_depth: usize, hotspot: Hotspot) {
        self.add_hotspot_impl(context_id, frame_depth, hotspot)
    }
    fn set_frame_input_callbacks(
        &mut self,
        context_id: u32,
        frame_depth: usize,
        callbacks: FrameInputCallbacks,
    ) {
        self.set_frame_input_callbacks_impl(context_id, frame_depth, callbacks)
    }
    fn set_hotspot_origin(&mut self, context_id: u32, x: i32, y: i32) {
        self.set_hotspot_origin_impl(context_id, x, y)
    }
    fn hotspot_process(
        &mut self,
        context_id: u32,
        frame_depth: usize,
    ) -> Option<HotspotEvent> {
        self.hotspot_process_impl(context_id, frame_depth)
    }
    fn text_line(&mut self, site: &vm::TextSite, bytes: &[u8]) {
        self.text_line_impl(site, bytes)
    }
    fn text_render(&mut self) -> Option<TextRenderInfo> {
        self.text_render_impl()
    }
    fn text_control(&mut self, control: &vm::text::ControlCode) {
        self.text_control_impl(control)
    }
    fn text_configure(
        &mut self,
        page: u32,
        base_x: i32,
        base_y: i32,
        layout_a: i32,
        layout_b: i32,
    ) {
        self.text_configure_impl(page, base_x, base_y, layout_a, layout_b)
    }
    fn text_set_font(
        &mut self,
        size: i32,
        width: i32,
        line_height: i32,
        flags: i32,
        face: &[u8],
    ) {
        self.text_set_font_impl(size, width, line_height, flags, face)
    }
    fn text_set_colors(&mut self, foreground: u32, background: i32) {
        self.text_set_colors_impl(foreground, background)
    }
    fn text_set_position(&mut self, x: i32, y: i32) {
        self.text_set_position_impl(x, y)
    }
    fn text_font_size(&mut self) -> i32 {
        self.text_font_size_impl()
    }
    fn text_line_height(&mut self) -> i32 {
        self.text_line_height_impl()
    }
    fn get_text_pos_x(&mut self) -> i32 {
        self.get_text_pos_x_impl()
    }
    fn get_text_pos_y(&mut self) -> i32 {
        self.get_text_pos_y_impl()
    }
    fn font_locate(&mut self, context_id: u32, page: u32, x: i32, y: i32) {
        self.font_locate_impl(context_id, page, x, y)
    }
    fn fontout_set_style(
        &mut self,
        context_id: u32,
        face: &[u8],
        size: i32,
        width: i32,
        line_height: i32,
        flags: i32,
    ) {
        self.fontout_set_style_impl(context_id, face, size, width, line_height, flags)
    }
    fn fontout_set_colors(&mut self, context_id: u32, foreground: u32, background: i32) {
        self.fontout_set_colors_impl(context_id, foreground, background)
    }
    fn font_render(
        &mut self,
        site: &vm::DisplayTextSite,
        context_id: u32,
        text: &[u8],
        width: Option<i32>,
        height: Option<i32>,
        alignment: Option<i32>,
    ) {
        self.font_render_impl(site, context_id, text, width, height, alignment)
    }
    fn deliver_input(&mut self, input: Input) {
        self.deliver_input_impl(input)
    }
    fn script_reset(&mut self, context_id: u32, frame_depth: usize) {
        self.script_reset_impl(context_id, frame_depth)
    }
    fn file_exists(&mut self, name: &[u8]) -> bool {
        self.file_exists_impl(name)
    }
    fn file_open(&mut self, name: &[u8]) -> u32 {
        self.file_open_impl(name)
    }
    fn file_readline(&mut self, handle: u32) -> Option<Vec<u8>> {
        self.file_readline_impl(handle)
    }
    fn file_readline_raw(&mut self, handle: u32) -> Option<Vec<u8>> {
        self.file_readline_raw_impl(handle)
    }
    fn file_close(&mut self, handle: u32) {
        self.file_close_impl(handle)
    }
}
