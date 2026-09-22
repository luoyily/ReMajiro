use crate::value::Value;
use std::io::{self, BufRead, Write};
#[derive(Debug, Clone)]
pub enum Input {
    Click { x: i32, y: i32 },
    PointerMove { x: i32, y: i32 },
    PointerButton { button: PointerButton, pressed: bool, x: i32, y: i32 },
    Key { virtual_key: u8, pressed: bool },
    Wheel { up: bool },
    Focused(bool),
    PointerInside(bool),
    Select(usize),
    Tick,
    Quit,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerButton {
    Left,
    Right,
    Middle,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextRenderInfo {
    pub page: u32,
    pub rect: [i32; 4],
    pub render_state: i32,
    pub action: TextRenderAction,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextRenderAction {
    #[default]
    Complete,
    Continue,
    PageOverflow,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hotspot {
    pub id: i32,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub on_enter: u32,
    pub on_leave: u32,
    pub on_click: u32,
    pub group: i32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameInputCallbacks {
    pub event_a: u32,
    pub event_b: u32,
    pub event_c: u32,
    pub key_event: u32,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotspotEvent {
    pub callback_hash: u32,
    pub args: Vec<i32>,
}
pub trait Host {
    fn text_show(&mut self, text: &str, box_id: u32) {
        eprintln!("[TEXT@{}] {}", box_id, text);
    }
    fn text_clear(&mut self, box_id: u32) {
        eprintln!("[TEXT@{}] clear", box_id);
    }
    fn text_select(&mut self, options: &[String]) -> usize {
        eprintln!("[SELECT] choose one:");
        for (i, o) in options.iter().enumerate() {
            eprintln!("  {}) {}", i + 1, o);
        }
        loop {
            match read_stdin_input().unwrap_or(Input::Quit) {
                Input::Select(n) if n >= 1 && n <= options.len() => return n - 1,
                Input::Quit => return 0,
                _ => eprintln!("[SELECT] invalid; try `select N`"),
            }
        }
    }
    fn sprite_create(&mut self, name: &str) -> u32 {
        eprintln!("[GFX] sprite_create {:?}", name);
        0
    }
    fn sprite_paste(
        &mut self,
        sprite: u32,
        dst_page: u32,
        position: Option<(i32, i32)>,
    ) {
        eprintln!(
            "[GFX] paste sprite={} into page={} at {:?}", sprite, dst_page, position
        );
    }
    fn sprite_set_alpha(&mut self, sprite: u32, alpha: u8) {
        eprintln!("[GFX] sprite={} alpha={}", sprite, alpha);
    }
    fn sprite_move(&mut self, sprite: u32, x: i32, y: i32, timing: Option<i32>) {
        eprintln!("[GFX] move sprite={} to ({},{}) timing={:?}", sprite, x, y, timing);
    }
    fn sprite_priority_high(&mut self, _sprite: u32) {}
    fn sprite_priority_high_group(&mut self, sprite: u32, _reference: Option<u32>) {
        self.sprite_priority_high(sprite);
    }
    fn sprite_priority_high_single(&mut self, sprite: u32, _reference: Option<u32>) {
        self.sprite_priority_high(sprite);
    }
    fn sprite_priority_low(&mut self, _sprite: u32) {}
    fn sprite_visibility_push(&mut self, _sprite: Option<u32>) {}
    fn sprite_visibility_pop(&mut self, _sprite: Option<u32>) {}
    fn sprite_page_auto_release(&mut self, _sprite: u32) {}
    fn sprite_set_smooth_animation(&mut self, _sprite: u32) {}
    fn sprite_rotate(
        &mut self,
        _sprite: u32,
        _target_degrees: f32,
        _duration_ms: i32,
        _extrapolate: bool,
    ) {}
    fn sprite_xmodify_define(&mut self, _sprite: u32, _keyframes: &[(f32, i32)]) {}
    fn sprite_xmodify_set(&mut self, _sprite: u32, _target: f32, _duration_ms: i32) {}
    fn sprite_ymodify_define(&mut self, _sprite: u32, _keyframes: &[(f32, i32)]) {}
    fn sprite_ymodify_set(&mut self, _sprite: u32, _target: f32, _duration_ms: i32) {}
    fn make_alfa_table(
        &mut self,
        _source_page: u32,
        _destination_page: u32,
        _table: &[u8; 256],
    ) {}
    fn sprite_priority_low_group(&mut self, sprite: u32, _reference: Option<u32>) {
        self.sprite_priority_low(sprite);
    }
    fn sprite_is_moving(&mut self, _sprite: u32) -> bool {
        false
    }
    fn sprite_is_alpha_animating(&mut self, _sprite: u32) -> bool {
        false
    }
    fn sprite_is_frame_animating(&mut self, _sprite: u32) -> bool {
        false
    }
    fn sprite_get_page(&mut self, _sprite: u32) -> u32 {
        0
    }
    fn sprite_width(&mut self, _sprite: u32) -> i32 {
        0
    }
    fn sprite_height(&mut self, _sprite: u32) -> i32 {
        0
    }
    fn sprite_pos_x(&mut self, _sprite: u32) -> i32 {
        0
    }
    fn sprite_pos_y(&mut self, _sprite: u32) -> i32 {
        0
    }
    fn sprite_priority_low_single(&mut self, sprite: u32, _reference: Option<u32>) {
        self.sprite_priority_low(sprite);
    }
    fn generate_key(&mut self, key_str: &[u8]) {
        let _ = key_str;
    }
    fn pic_unpack(&mut self, filename: &[u8]) {
        let _ = filename;
    }
    fn pic_unpack_into(
        &mut self,
        dst_page: u32,
        filename: &[u8],
        dst_x: i32,
        dst_y: i32,
    ) {
        let _ = (dst_page, filename, dst_x, dst_y);
        self.pic_unpack(filename);
    }
    fn page_set_draw_mode(&mut self, _page: u32, _mode: u32) {}
    fn sprite_create_raw(
        &mut self,
        _count: usize,
        _args: &[crate::value::Value],
    ) -> u32 {
        0
    }
    fn sprite_mark_overlay(&mut self, _sprite: u32) {}
    fn sprite_create_file(&mut self, filename: &[u8]) -> u32 {
        eprintln!("[GFX] sprite_create_file {:?}", String::from_utf8_lossy(filename));
        0
    }
    fn sprite_create_file_raw(
        &mut self,
        count: usize,
        args: &[crate::value::Value],
    ) -> u32 {
        let filename = args
            .get(count.saturating_sub(1))
            .and_then(crate::value::Value::as_str_bytes)
            .unwrap_or(&[]);
        self.sprite_create_file(filename)
    }
    fn sprite_set_clip(
        &mut self,
        _sprite: u32,
        _x: i32,
        _y: i32,
        _width: i32,
        _height: i32,
    ) {}
    fn set_frontbuffer(&mut self, page: u32) -> u32 {
        eprintln!("[GFX] set_frontbuffer page={}", page);
        0
    }
    fn sprite_release(&mut self, sprite: u32) {
        eprintln!("[GFX] sprite_release sp={}", sprite);
    }
    fn page_get_width(&mut self, page: u32) -> i32 {
        eprintln!("[GFX] page_get_width page={}", page);
        0
    }
    fn page_get_height(&mut self, page: u32) -> i32 {
        eprintln!("[GFX] page_get_height page={}", page);
        0
    }
    fn page_get_pixel(&mut self, page: u32, _x: i32, _y: i32) -> i32 {
        eprintln!("[GFX] page_get_pixel page={}", page);
        0
    }
    fn page_release(&mut self, page: u32) {
        eprintln!("[GFX] page_release page={}", page);
    }
    fn page_create_file(&mut self, filename: &[u8]) -> u32 {
        eprintln!("[GFX] page_create_file {:?}", String::from_utf8_lossy(filename));
        0
    }
    fn page_create_file_alpha(&mut self, filename: &[u8]) -> u32 {
        self.page_create_file(filename)
    }
    fn pic_get_width(&mut self, filename: &[u8]) -> i32 {
        eprintln!("[GFX] pic_get_width {:?} (trace)", String::from_utf8_lossy(filename));
        0
    }
    fn pic_get_height(&mut self, filename: &[u8]) -> i32 {
        eprintln!(
            "[GFX] pic_get_height {:?} (trace)", String::from_utf8_lossy(filename)
        );
        0
    }
    fn page_create(&mut self, w: i32, h: i32, indexed: bool) -> u32 {
        eprintln!("[GFX] page_create {}x{} indexed={}", w, h, indexed);
        0
    }
    fn page_create_with_antidata(&mut self, w: i32, h: i32, indexed: bool) -> u32 {
        self.page_create(w, h, indexed)
    }
    fn page_get_alpha(&mut self, page: u32) -> u32 {
        eprintln!("[GFX] page_get_alpha page={}", page);
        0
    }
    fn grp_boxfill(&mut self, page: u32, x: i32, y: i32, w: i32, h: i32, color: u32) {
        eprintln!(
            "[GFX] grp_boxfill page={} ({},{}) {}x{} color=0x{:08X}", page, x, y, w, h,
            color
        );
    }
    #[allow(clippy::too_many_arguments)]
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
        eprintln!(
            "[GFX] grp_copy page{}({},{}) {}x{} → page{}({},{})", src_page, src_x,
            src_y, w, h, dst_page, dst_x, dst_y
        );
    }
    #[allow(clippy::too_many_arguments)]
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
        _alpha: i32,
    ) {
        self.grp_copy(src_page, src_x, src_y, w, h, dst_page, dst_x, dst_y);
    }
    #[allow(clippy::too_many_arguments)]
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
        eprintln!(
            "[GFX] grp_mulcopy page{}({},{}) {}x{} -> page{}({},{})", src_page, src_x,
            src_y, w, h, dst_page, dst_x, dst_y
        );
    }
    fn page_set_antidata(&mut self, page: u32, alpha_page: u32) {
        eprintln!("[GFX] page_set_antidata page={} alpha_page={}", page, alpha_page);
    }
    #[allow(clippy::too_many_arguments)]
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
        eprintln!(
            "[GFX] grp_modcopy page{}({},{}) {}x{} → page{}({},{}) {}x{}", src_page,
            src_x, src_y, src_w, src_h, dst_page, dst_x, dst_y, dst_w, dst_h
        );
    }
    #[allow(clippy::too_many_arguments)]
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
        eprintln!(
            "[GFX] grp_modify_copy page{}({},{}) {}x{} -> page{}({},{}) {}x{} angle={} scale=({}, {})",
            src_page, src_x, src_y, src_w, src_h, dst_page, dst_x, dst_y, dst_w, dst_h,
            angle_degrees, scale_x, scale_y
        );
    }
    #[allow(clippy::too_many_arguments)]
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
        eprintln!(
            "[GFX] make_mosaic page{}({},{}) {}x{} -> page{}({},{}) block={}", src_page,
            src_x, src_y, width, height, dst_page, dst_x, dst_y, block_size
        );
        rng_state
    }
    fn voice_check_file(&mut self, name: &[u8]) -> i32 {
        eprintln!("[AUDIO] voice_check_file {:?}", String::from_utf8_lossy(name));
        0
    }
    fn bgm_play_or_stop(&mut self, name: &[u8], loop_: bool) {
        if name.is_empty() {
            eprintln!("[AUDIO] bgm stop");
        } else {
            eprintln!(
                "[AUDIO] bgm play {:?} loop={}", String::from_utf8_lossy(name), loop_
            );
        }
    }
    fn voice_play(&mut self, ch: u8, name: &[u8], loop_: bool) {
        eprintln!(
            "[AUDIO] voice {:?} ch={} loop={}", String::from_utf8_lossy(name), ch, loop_
        );
    }
    fn voice_stop(&mut self, ch: u8) {
        eprintln!("[AUDIO] voice_stop ch={}", ch);
    }
    fn voice_set_vol(&mut self, ch: u8, vol: i32) {
        eprintln!("[AUDIO] voice_vol ch={} vol={}", ch, vol);
    }
    fn voice_fade(&mut self, ch: u8, target_vol: i32, duration_ms: i32) {
        eprintln!("[AUDIO] voice_fade ch={} vol={} ms={}", ch, target_vol, duration_ms);
    }
    fn voice_fadeout(&mut self, ch: u8, duration_ms: i32) {
        eprintln!("[AUDIO] voice_fadeout ch={} ms={}", ch, duration_ms);
    }
    fn voice_get_stat(&mut self, ch: u8) -> i32 {
        eprintln!("[AUDIO] voice_stat ch={} → 0", ch);
        0
    }
    fn voice_get_filename(&mut self, ch: u8) -> Vec<u8> {
        eprintln!("[AUDIO] voice_filename ch={} → \"\"", ch);
        Vec::new()
    }
    fn voice_set_pan(&mut self, ch: u8, pan: i32) {
        eprintln!("[AUDIO] voice_pan ch={} pan={}", ch, pan);
    }
    fn sound_play(&mut self, ch: u8, name: &[u8], loop_: bool) {
        eprintln!(
            "[AUDIO] sound {:?} ch={} loop={}", String::from_utf8_lossy(name), ch, loop_
        );
    }
    fn sound_stop(&mut self, ch: u8) {
        eprintln!("[AUDIO] sound_stop ch={}", ch);
    }
    fn sound_set_vol(&mut self, ch: u8, vol: i32) {
        eprintln!("[AUDIO] sound_vol ch={} vol={}", ch, vol);
    }
    fn sound_set_pan(&mut self, ch: u8, pan: i32) {
        eprintln!("[AUDIO] sound_pan ch={} pan={}", ch, pan);
    }
    fn sound_fade(&mut self, ch: u8, target_vol: i32, duration_ms: i32) {
        eprintln!("[AUDIO] sound_fade ch={} vol={} ms={}", ch, target_vol, duration_ms);
    }
    fn sound_fadeout(&mut self, ch: u8, duration_ms: i32) {
        eprintln!("[AUDIO] sound_fadeout ch={} ms={}", ch, duration_ms);
    }
    fn sound_get_stat(&mut self, ch: u8) -> i32 {
        eprintln!("[AUDIO] sound_stat ch={} → 0", ch);
        0
    }
    fn sound_get_filename(&mut self, ch: u8) -> Vec<u8> {
        eprintln!("[AUDIO] sound_filename ch={}", ch);
        Vec::new()
    }
    fn bgm_play(&mut self, name: &str, loop_: bool) {
        eprintln!("[AUDIO] bgm {:?} loop={}", name, loop_);
    }
    fn bgm_stop(&mut self) {
        eprintln!("[AUDIO] bgm stop");
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
        eprintln!(
            "[VIDEO] play {:?} visible={} rect=({},{},{},{})",
            String::from_utf8_lossy(name), visible, x, y, w, h
        );
    }
    fn movie_texture_play(
        &mut self,
        name: &[u8],
        loop_: bool,
        start_pos: u32,
        duration: i32,
    ) {
        eprintln!(
            "[VIDEO] texture_play {:?} loop={} start={} dur={}",
            String::from_utf8_lossy(name), loop_, start_pos, duration
        );
    }
    fn movie_frame_update(&mut self) -> u32 {
        eprintln!("[VIDEO] frame_update → 0 (trace: no texture)");
        0
    }
    fn movie_cleanup_present(&mut self) {
        eprintln!("[VIDEO] cleanup_present");
    }
    fn movie_apply_config(&mut self, value: i32) {
        eprintln!("[VIDEO] apply_config {value}");
    }
    fn movie_is_playing(&mut self) -> bool {
        false
    }
    fn movie_wait_event_flags(&mut self, context_id: u32) -> u8 {
        let _ = context_id;
        0
    }
    fn movie_wait_cancel_pressed(&mut self) -> bool {
        false
    }
    fn clear_movie_wait_input(&mut self, context_id: u32) {
        let _ = context_id;
    }
    fn set_file_drop_accept(&mut self, enabled: bool) {
        eprintln!("[SYS] file_drop_accept {enabled}");
    }
    fn refresh_platform_menu(&mut self) {
        eprintln!("[SYS] refresh_platform_menu");
    }
    fn open_http(&mut self, suffix: &[u8]) {
        eprintln!("[SYS] open_http {:?}", String::from_utf8_lossy(suffix));
    }
    fn key_modifier_mask(&mut self) -> u32 {
        0
    }
    fn recent_key_event_mask(&mut self, _context_id: u32) -> u32 {
        0
    }
    fn scheduler_button_latches(&mut self, _context_id: u32) -> u8 {
        0
    }
    fn consume_scheduler_button_latches(&mut self, _context_id: u32, _mask: u8) {}
    fn four_key_wait_active(&mut self) -> bool {
        false
    }
    fn input_suppression_bypassed(&mut self) -> bool {
        false
    }
    fn input_is_held_or_recent(&mut self, _index: usize) -> bool {
        false
    }
    fn input_consume_recent_press(&mut self, _index: usize) {}
    fn set_click_suppression_mask(&mut self, mask: i32) {
        eprintln!("[INPUT] set_click_suppression_mask {mask:#x}");
    }
    fn key_capslock_on(&mut self) -> bool {
        false
    }
    fn key_shift_pressed(&mut self) -> bool {
        false
    }
    fn mouse_x(&mut self) -> i32 {
        0
    }
    fn mouse_y(&mut self) -> i32 {
        0
    }
    fn set_viewport_offset(&mut self, x: i32, y: i32) {
        eprintln!("[INPUT] set_viewport_offset ({},{})", x, y);
    }
    fn warp_cursor(&mut self, x: i32, y: i32) {
        eprintln!("[INPUT] warp_cursor ({},{})", x, y);
    }
    fn cursor_inside(&mut self) -> bool {
        eprintln!("[INPUT] cursor_inside → false (trace: no window)");
        false
    }
    fn add_hotspot(&mut self, _context_id: u32, _frame_depth: usize, hotspot: Hotspot) {
        eprintln!(
            "[HOTSPOT] add id={} rect=({},{},{},{}) callbacks=({:08X},{:08X},{:08X}) group={}",
            hotspot.id, hotspot.x, hotspot.y, hotspot.width, hotspot.height, hotspot
            .on_enter, hotspot.on_leave, hotspot.on_click, hotspot.group,
        );
    }
    fn set_frame_input_callbacks(
        &mut self,
        _context_id: u32,
        _frame_depth: usize,
        callbacks: FrameInputCallbacks,
    ) {
        eprintln!(
            "[HOTSPOT] set_frame_callbacks a=0x{:X} b=0x{:X} c=0x{:X} key=0x{:X}",
            callbacks.event_a, callbacks.event_b, callbacks.event_c, callbacks.key_event
        );
    }
    fn set_hotspot_origin(&mut self, _context_id: u32, x: i32, y: i32) {
        eprintln!("[HOTSPOT] set_origin ({},{})", x, y);
    }
    fn hotspot_process(
        &mut self,
        _context_id: u32,
        _frame_depth: usize,
    ) -> Option<HotspotEvent> {
        None
    }
    fn drag_init(&mut self, a0: i32, a1: i32, a2: i32) {
        eprintln!("[DRAG] init ({},{},{})", a0, a1, a2);
    }
    fn drag_end(&mut self) {
        eprintln!("[DRAG] end");
    }
    fn scene_transition(&mut self, target: &str) {
        eprintln!("[SCENE] transition → {:?}", target);
    }
    fn scene_unload_and_exit(&mut self) {
        eprintln!("[SCENE] unload + exit");
    }
    fn script_reset(&mut self, _context_id: u32, _frame_depth: usize) {
        eprintln!("[SCENE] script_reset");
    }
    fn cursor_show_or_hide(&mut self, show: bool) -> bool {
        eprintln!("[CURSOR] {}", if show { "show" } else { "hide" });
        true
    }
    fn set_capslock_state(&mut self, desired_on: bool) {
        eprintln!("[INPUT] set CapsLock → {}", desired_on);
    }
    fn get_timestamp(&mut self) -> i32 {
        0
    }
    fn begin_scheduler_tick(&mut self) {}
    fn set_native_time_scale(&mut self, _scale: f32) {}
    fn set_fullscreen(&mut self, _fullscreen: bool) {}
    fn is_fullscreen(&mut self) -> bool {
        false
    }
    fn config_load(&mut self, _name: &[u8], default: i32) -> i32 {
        default
    }
    fn save_dialog_available(&mut self) -> bool {
        true
    }
    fn save_set_dialog_available(&mut self, _available: bool) {}
    fn save_snapshot_allowed(&mut self) -> bool {
        true
    }
    fn save_finish_command(&mut self) {
        eprintln!("[SAVE] finish_command");
    }
    fn selected_font_face(&mut self) -> Vec<u8> {
        Vec::new()
    }
    fn set_selected_font_face(&mut self, _face: &[u8], _mode: i32) {}
    fn select_font_face(&mut self, current: &[u8], _mode: i32) -> Vec<u8> {
        current.to_vec()
    }
    fn confirm_yes_no(&mut self, _message: &[u8]) -> bool {
        false
    }
    fn show_message_ok(&mut self, message: &[u8]) {
        eprintln!("[SYS] message_ok {:?}", String::from_utf8_lossy(message));
    }
    fn set_native_mode_flags(&mut self, bits: u8, special: bool) {
        eprintln!("[SYS] native_mode bits={bits:03b} special={special}");
    }
    fn input_dispatch_mode(&mut self) -> i32 {
        0
    }
    fn native_mode_pending(&mut self) -> bool {
        false
    }
    fn music_get_status(&mut self) -> i32 {
        0
    }
    fn config_store(&mut self, _name: &[u8], _value: i32) {}
    fn set_sound_master_volume(&mut self, _value: i32) {}
    fn set_voice_master_volume(&mut self, _value: i32) {}
    fn set_music_master_volume(&mut self, _value: i32) {}
    fn main_sound_play(&mut self, name: &[u8], loop_: bool) {
        eprintln!(
            "[AUDIO] main_sound_play {:?} loop={}", String::from_utf8_lossy(name), loop_
        );
    }
    fn grp_point_set(&mut self, _page: u32, _x: i32, _y: i32, _color: u32) {}
    fn grp_reverse(&mut self, _page: u32, _x: i32, _y: i32, _w: i32, _h: i32) {}
    fn grp_mulboxfill(
        &mut self,
        _page: u32,
        _x: i32,
        _y: i32,
        _w: i32,
        _h: i32,
        _color: u32,
    ) {}
    fn grp_alphablend(
        &mut self,
        _page: u32,
        _x: i32,
        _y: i32,
        _w: i32,
        _h: i32,
        _color: u32,
    ) {}
    #[allow(clippy::too_many_arguments)]
    fn grp_sepia(
        &mut self,
        _page: u32,
        _x: i32,
        _y: i32,
        _w: i32,
        _h: i32,
        _dark_color: u32,
        _light_color: u32,
        _mix: Option<i32>,
    ) {}
    #[allow(clippy::too_many_arguments)]
    fn grp_extboxfill(
        &mut self,
        _page: u32,
        _x: i32,
        _y: i32,
        _w: i32,
        _h: i32,
        _color: u32,
        _alpha: i32,
    ) {}
    #[allow(clippy::too_many_arguments)]
    fn grp_revmulcopy(
        &mut self,
        _src_page: u32,
        _src_x: i32,
        _src_y: i32,
        _w: i32,
        _h: i32,
        _dst_page: u32,
        _dst_x: i32,
        _dst_y: i32,
    ) {}
    #[allow(clippy::too_many_arguments)]
    fn grp_swap(
        &mut self,
        _src_page: u32,
        _src_x: i32,
        _src_y: i32,
        _w: i32,
        _h: i32,
        _dst_page: u32,
        _dst_x: i32,
        _dst_y: i32,
    ) {}
    fn main_sound_stop(&mut self) {
        eprintln!("[AUDIO] main_sound_stop");
    }
    fn effect_skip_active(&mut self) -> bool {
        false
    }
    fn native_music_play_dual(&mut self, primary: &[u8], secondary: &[u8]) {
        eprintln!(
            "[AUDIO] native_music_play_dual primary={:?} secondary={:?}",
            String::from_utf8_lossy(primary), String::from_utf8_lossy(secondary)
        );
    }
    fn native_music_stop(&mut self) {
        eprintln!("[AUDIO] native_music_stop");
    }
    fn native_music_fadeout(&mut self, duration_ms: i32) {
        eprintln!("[AUDIO] native_music_fadeout ms={duration_ms}");
    }
    fn native_music_fade(&mut self, target_volume: i32, duration_ms: i32) {
        eprintln!("[AUDIO] native_music_fade volume={target_volume} ms={duration_ms}");
    }
    fn native_music_pause(&mut self) {
        eprintln!("[AUDIO] native_music_pause");
    }
    fn native_music_resume(&mut self) {
        eprintln!("[AUDIO] native_music_resume");
    }
    fn native_music_snapshot(&mut self) {
        eprintln!("[AUDIO] native_music_snapshot");
    }
    fn native_music_restore(&mut self) {
        eprintln!("[AUDIO] native_music_restore");
    }
    fn native_music_replace(&mut self, primary: &[u8], secondary: &[u8], loop_: bool) {
        eprintln!(
            "[AUDIO] native_music_replace primary={:?} secondary={:?} loop={loop_}",
            String::from_utf8_lossy(primary), String::from_utf8_lossy(secondary)
        );
    }
    fn native_music_clear_end_state(&mut self) {
        eprintln!("[AUDIO] native_music_clear_end_state");
    }
    fn native_music_set_frequency(&mut self, frequency: i32) {
        eprintln!("[AUDIO] native_music_frequency {frequency}");
    }
    fn native_music_aux_stop(&mut self) {
        eprintln!("[AUDIO] native_music_aux_stop");
    }
    fn native_music_aux_set_volume(&mut self, volume: i32) {
        eprintln!("[AUDIO] native_music_aux_volume {volume}");
    }
    fn native_music_aux_set_pan(&mut self, value: i32) {
        eprintln!("[AUDIO] native_music_aux_pan {value}");
    }
    fn native_music_aux_fade(&mut self, target_volume: i32, duration_ms: i32) {
        eprintln!(
            "[AUDIO] native_music_aux_fade volume={target_volume} ms={duration_ms}"
        );
    }
    fn native_music_aux_fadeout(&mut self, duration_ms: i32) {
        eprintln!("[AUDIO] native_music_aux_fadeout ms={duration_ms}");
    }
    fn native_music_aux_filename(&mut self) -> Vec<u8> {
        Vec::new()
    }
    fn native_music_set_volume(&mut self, volume: i32) {
        eprintln!("[AUDIO] native_music_volume {volume}");
    }
    fn native_sound_a_set_volume(&mut self, volume: i32) {
        eprintln!("[AUDIO] native_sound_a_volume {volume}");
    }
    fn native_sound_a_play_dual(&mut self, primary: &[u8], secondary: &[u8]) {
        eprintln!(
            "[AUDIO] native_sound_a_play_dual primary={:?} secondary={:?}",
            String::from_utf8_lossy(primary), String::from_utf8_lossy(secondary)
        );
    }
    fn native_sound_a_fadeout(&mut self, duration_ms: i32) {
        eprintln!("[AUDIO] native_sound_a_fadeout ms={duration_ms}");
    }
    fn native_sound_a_fade(&mut self, target_volume: i32, duration_ms: i32) {
        eprintln!("[AUDIO] native_sound_a_fade volume={target_volume} ms={duration_ms}");
    }
    fn native_sound_a_status(&mut self) -> i32 {
        0
    }
    fn native_sound_a_stop(&mut self) {
        eprintln!("[AUDIO] native_sound_a_stop");
    }
    fn native_sound_a_set_pan(&mut self, pan: i32) {
        eprintln!("[AUDIO] native_sound_a_pan {pan}");
    }
    fn native_sound_a_filename(&mut self) -> Vec<u8> {
        Vec::new()
    }
    fn native_sound_a_flag(&mut self) -> i32 {
        0
    }
    fn native_sound_b_play(&mut self, primary: &[u8], loop_: bool) {
        eprintln!(
            "[AUDIO] native_sound_b_play {:?} loop={loop_}",
            String::from_utf8_lossy(primary)
        );
    }
    fn native_sound_b_fadeout(&mut self, duration_ms: i32) {
        eprintln!("[AUDIO] native_sound_b_fadeout ms={duration_ms}");
    }
    fn native_sound_b_stop(&mut self) {
        eprintln!("[AUDIO] native_sound_b_stop");
    }
    fn native_sound_b_play_dual(&mut self, primary: &[u8], secondary: &[u8]) {
        eprintln!(
            "[AUDIO] native_sound_b_play_dual primary={:?} secondary={:?}",
            String::from_utf8_lossy(primary), String::from_utf8_lossy(secondary)
        );
    }
    fn native_sound_b_set_volume(&mut self, volume: i32) {
        eprintln!("[AUDIO] native_sound_b_volume {volume}");
    }
    fn native_sound_b_set_pan(&mut self, pan: i32) {
        eprintln!("[AUDIO] native_sound_b_pan {pan}");
    }
    fn native_sound_b_fade(&mut self, target_volume: i32, duration_ms: i32) {
        eprintln!("[AUDIO] native_sound_b_fade volume={target_volume} ms={duration_ms}");
    }
    fn native_sound_b_status(&mut self) -> i32 {
        0
    }
    fn native_sound_b_filename(&mut self) -> Vec<u8> {
        Vec::new()
    }
    fn native_sound_b_flag(&mut self) -> i32 {
        0
    }
    fn native_sound_c_play(&mut self, primary: &[u8], loop_: bool) {
        eprintln!(
            "[AUDIO] native_sound_c_play {:?} loop={loop_}",
            String::from_utf8_lossy(primary)
        );
    }
    fn native_sound_c_play_dual(&mut self, primary: &[u8], secondary: &[u8]) {
        eprintln!(
            "[AUDIO] native_sound_c_play_dual primary={:?} secondary={:?}",
            String::from_utf8_lossy(primary), String::from_utf8_lossy(secondary)
        );
    }
    fn native_sound_c_fadeout(&mut self, duration_ms: i32) {
        eprintln!("[AUDIO] native_sound_c_fadeout ms={duration_ms}");
    }
    fn native_sound_c_status(&mut self) -> i32 {
        0
    }
    fn native_sound_c_stop(&mut self) {
        eprintln!("[AUDIO] native_sound_c_stop");
    }
    fn native_sound_c_set_volume(&mut self, volume: i32) {
        eprintln!("[AUDIO] native_sound_c_volume {volume}");
    }
    fn native_sound_c_set_pan(&mut self, pan: i32) {
        eprintln!("[AUDIO] native_sound_c_pan {pan}");
    }
    fn native_sound_c_fade(&mut self, target_volume: i32, duration_ms: i32) {
        eprintln!("[AUDIO] native_sound_c_fade volume={target_volume} ms={duration_ms}");
    }
    fn native_sound_c_filename(&mut self) -> Vec<u8> {
        Vec::new()
    }
    fn native_sound_c_flag(&mut self) -> i32 {
        0
    }
    fn native_sound_d_play(&mut self, primary: &[u8], loop_: bool) {
        eprintln!(
            "[AUDIO] native_sound_d_play {:?} loop={loop_}",
            String::from_utf8_lossy(primary)
        );
    }
    fn native_sound_d_play_dual(&mut self, primary: &[u8], secondary: &[u8]) {
        eprintln!(
            "[AUDIO] native_sound_d_play_dual primary={:?} secondary={:?}",
            String::from_utf8_lossy(primary), String::from_utf8_lossy(secondary)
        );
    }
    fn native_sound_d_stop(&mut self) {
        eprintln!("[AUDIO] native_sound_d_stop");
    }
    fn native_sound_d_set_volume(&mut self, volume: i32) {
        eprintln!("[AUDIO] native_sound_d_volume {volume}");
    }
    fn native_sound_d_fade(&mut self, target_volume: i32, duration_ms: i32) {
        eprintln!("[AUDIO] native_sound_d_fade volume={target_volume} ms={duration_ms}");
    }
    fn native_sound_d_fadeout(&mut self, duration_ms: i32) {
        eprintln!("[AUDIO] native_sound_d_fadeout ms={duration_ms}");
    }
    fn native_sound_d_set_pan(&mut self, pan: i32) {
        eprintln!("[AUDIO] native_sound_d_pan {pan}");
    }
    fn native_sound_d_status(&mut self) -> i32 {
        0
    }
    fn native_sound_d_filename(&mut self) -> Vec<u8> {
        Vec::new()
    }
    fn native_sound_d_flag(&mut self) -> i32 {
        0
    }
    fn invalidate_page(&mut self, _page: u32, _rect: Option<(i32, i32, i32, i32)>) {}
    fn display_sync(&mut self, mode: i32) -> i32 {
        mode
    }
    fn set_input_dispatch_mode(&mut self, _mode: i32) {}
    fn trigger_middle_input_pulse(&mut self) {}
    fn take_display_mode(&mut self) -> i32 {
        0
    }
    fn register_picture_hash(&mut self, _hash: u32) -> bool {
        true
    }
    fn picture_hash_registered(&mut self, _hash: u32) -> bool {
        false
    }
    fn cooperative_timers(&self) -> bool {
        false
    }
    fn timer_lerp(&mut self, end: i32, start_ts: i32, duration: i32, value: i32) {
        eprintln!(
            "[TIMER] lerp end={} start={} dur={} → {}", end, start_ts, duration, value
        );
    }
    fn bgm_wait_one_tick(&mut self) {}
    fn set_render_mode(&mut self, w: i32, h: i32, scale_x: i32, scale_y: i32) {
        eprintln!("[GFX] set_render_mode {}x{} scale=({},{})", w, h, scale_x, scale_y);
    }
    fn get_render_width(&mut self) -> i32 {
        1024
    }
    fn get_render_height(&mut self) -> i32 {
        576
    }
    fn get_render_page(&mut self) -> i32 {
        0
    }
    fn get_alfapage(&mut self, _scene_or_page: u32) -> u32 {
        0
    }
    fn get_text_pos_x(&mut self) -> i32 {
        0
    }
    fn get_text_pos_y(&mut self) -> i32 {
        0
    }
    fn get_voice_enable_flag(&mut self) -> bool {
        false
    }
    fn get_global_d(&mut self) -> i32 {
        0
    }
    fn get_global_f(&mut self) -> i32 {
        0
    }
    fn set_text_redraw_flag(&mut self) {
        eprintln!("[TEXT] redraw flag set");
    }
    fn text_configure(
        &mut self,
        page: u32,
        base_x: i32,
        base_y: i32,
        layout_a: i32,
        layout_b: i32,
    ) {
        eprintln!(
            "[TEXT] configure page={} origin=({},{}) layout=({},{})", page, base_x,
            base_y, layout_a, layout_b
        );
    }
    fn text_set_font(
        &mut self,
        size: i32,
        width: i32,
        line_height: i32,
        flags: i32,
        face: &[u8],
    ) {
        eprintln!(
            "[TEXT] font size={} width={} line={} flags={} face={:?}", size, width,
            line_height, flags, String::from_utf8_lossy(face)
        );
    }
    fn text_set_colors(&mut self, foreground: u32, background: i32) {
        eprintln!(
            "[TEXT] colors fg=0x{:08X} bg=0x{:08X}", foreground, background as u32
        );
    }
    fn text_set_position(&mut self, x: i32, y: i32) {
        eprintln!("[TEXT] position ({},{})", x, y);
    }
    fn text_font_size(&mut self) -> i32 {
        24
    }
    fn text_line_height(&mut self) -> i32 {
        30
    }
    fn font_locate(&mut self, context_id: u32, page: u32, x: i32, y: i32) {
        eprintln!("[FONT] ctx={} locate page={} ({},{})", context_id, page, x, y);
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
        eprintln!(
            "[FONT] ctx={} style face={:?} size={} width={} line={} flags={}",
            context_id, String::from_utf8_lossy(face), size, width, line_height, flags
        );
    }
    fn fontout_set_colors(&mut self, context_id: u32, foreground: u32, background: i32) {
        eprintln!(
            "[FONT] ctx={} colors fg=0x{:08X} bg=0x{:08X}", context_id, foreground,
            background as u32
        );
    }
    fn font_render(
        &mut self,
        _site: &DisplayTextSite,
        context_id: u32,
        text: &[u8],
        width: Option<i32>,
        height: Option<i32>,
        alignment: Option<i32>,
    ) {
        eprintln!(
            "[FONT] ctx={} render {:?} box={:?}x{:?} align={:?}", context_id,
            String::from_utf8_lossy(text), width, height, alignment
        );
    }
    fn debug_log_stack(&mut self, line: &[u8]) {
        eprintln!("[DEBUG] {}", String::from_utf8_lossy(line));
    }
    fn set_voice_ts_flag(&mut self) {
        eprintln!("[VOICE] ts flag set");
    }
    fn set_voice_flag(&mut self) {
        eprintln!("[VOICE] enable flag set");
    }
    fn get_error_state(&mut self) -> bool {
        false
    }
    fn check_skip_flag(&mut self) -> bool {
        false
    }
    fn global_var_lookup(&mut self, name: &str) -> i32 {
        eprintln!("[SYS] global_var_lookup {:?}", name);
        0
    }
    fn config_name_stash(&mut self, name: &str) {
        eprintln!("[CONFIG] name stash {:?}", name);
    }
    fn config_set_autospeed(&mut self, value: i32) {
        eprintln!("[CONFIG] AutoSpeed = {}", value);
    }
    fn config_getset_scale(&mut self, new_value: Option<i32>) -> i32 {
        if let Some(v) = new_value {
            eprintln!("[CONFIG] RegSyukusyou = {}", v);
        }
        0
    }
    fn config_get_quicksave_failsafe(&mut self) -> i32 {
        0
    }
    fn config_set_quicksave_failsafe(&mut self, value: i32) {
        eprintln!("[CONFIG] QuicksaveFailsafe = {}", value);
    }
    fn config_get_quickload_failsafe(&mut self) -> i32 {
        0
    }
    fn config_set_quickload_failsafe(&mut self, value: i32) {
        eprintln!("[CONFIG] QuickloadFailsafe = {}", value);
    }
    fn save_write_slot(
        &mut self,
        slot: i64,
        _save: &formats::save::SavFile,
        _system: &formats::save::MssFile,
    ) -> bool {
        eprintln!("[SAVE] write slot={}", slot);
        false
    }
    fn save_read_slot(&mut self, slot: i64) -> Option<formats::save::SavFile> {
        eprintln!("[SAVE] read slot={} (trace: miss)", slot);
        None
    }
    fn save_read_system(&mut self) -> Option<formats::save::MssFile> {
        eprintln!("[SAVE] read system state (trace: miss)");
        None
    }
    fn save_write_system(&mut self, _system: &formats::save::MssFile) -> bool {
        eprintln!("[SAVE] write system state (trace: unsupported)");
        false
    }
    fn save_read_readmarks(&mut self) -> Option<Vec<formats::save::ReadmarkRecord>> {
        None
    }
    fn save_write_readmarks(
        &mut self,
        _records: &[formats::save::ReadmarkRecord],
    ) -> bool {
        true
    }
    fn save_capture_audio_state(&mut self) -> Vec<u8> {
        vec![0; formats::save::AUDIO_BLOB_SIZE]
    }
    fn save_prepare_restore(&mut self) {
        eprintln!("[SAVE] prepare restore boundary");
    }
    fn save_restore_audio_state(&mut self, _audio_blob: &[u8]) {}
    fn save_refresh_after_restore(&mut self) {
        eprintln!("[SAVE] refresh display after restore");
    }
    fn save_copy_slot(&mut self, src: i64, dst: i64) {
        eprintln!("[SAVE] copy slot {} -> {}", src, dst);
    }
    fn save_write_title(&mut self, slot: i64, title: &[u8]) {
        eprintln!(
            "[SAVE] write_title slot={} {:?}", slot, String::from_utf8_lossy(title)
        );
    }
    fn save_read_thumbnail(&mut self, slot: i64, dst_page: u32, x: i32, y: i32) -> bool {
        eprintln!(
            "[SAVE] read_thumbnail slot={} -> page={} ({},{}) (trace: miss)", slot,
            dst_page, x, y
        );
        false
    }
    fn save_delete_slot(&mut self, slot: i64) {
        eprintln!("[SAVE] delete slot={}", slot);
    }
    fn save_read_meta(&mut self, slot: i64) -> String {
        eprintln!("[SAVE] read_meta slot={} (trace: empty)", slot);
        String::new()
    }
    fn save_get_timestamp(&mut self, slot: i64) -> String {
        eprintln!("[SAVE] get_timestamp slot={} (trace: empty)", slot);
        String::new()
    }
    fn save_get_description(&mut self) -> String {
        eprintln!("[SAVE] get_description (trace: empty)");
        String::new()
    }
    fn save_make_thumbnail(&mut self, w: u32, h: u32, src_page: Option<u32>) {
        eprintln!("[SAVE] make_thumbnail {}x{} src={:?}", w, h, src_page);
    }
    fn save_make_thumbnail_full(
        &mut self,
        dst_page: u32,
        src_page: u32,
        w: u32,
        h: u32,
    ) {
        eprintln!(
            "[SAVE] make_thumbnail_full dst={} src={} {}x{}", dst_page, src_page, w, h
        );
    }
    fn save_blit_thumbnail(&mut self, dst_page: u32, x: i32, y: i32) -> bool {
        eprintln!(
            "[SAVE] blit_thumbnail -> page={} ({},{}) (trace: miss)", dst_page, x, y
        );
        false
    }
    fn save_free_data(&mut self) {
        eprintln!("[SAVE] free_data");
    }
    fn save_take_ready_flag(&mut self) -> i32 {
        eprintln!("[SAVE] take_ready_flag (trace: 0)");
        0
    }
    fn save_mark_ready(&mut self) {
        eprintln!("[SAVE] mark_ready");
    }
    fn save_begin_serialize(&mut self) -> i32 {
        eprintln!("[SAVE] begin_serialize (trace: returns 0)");
        0
    }
    fn save_set_description(&mut self, description: &[u8]) {
        eprintln!("[SAVE] set_description {:?}", String::from_utf8_lossy(description));
    }
    fn localize_save_description<'a>(
        &self,
        description: &'a [u8],
    ) -> std::borrow::Cow<'a, [u8]> {
        let _ = description;
        std::borrow::Cow::Borrowed(description)
    }
    fn save_snapshot_state(&mut self) {
        eprintln!("[SAVE] snapshot_state");
    }
    fn save_sysconfig(&mut self, params: &[i32]) {
        eprintln!("[SAVE] sysconfig {} params {:?}", params.len(), params);
    }
    fn file_exists(&mut self, name: &[u8]) -> bool {
        eprintln!("[FILE] exists {:?} → false (trace)", String::from_utf8_lossy(name));
        false
    }
    fn file_open(&mut self, name: &[u8]) -> u32 {
        eprintln!("[FILE] open {:?} (trace: not found)", String::from_utf8_lossy(name));
        0
    }
    fn file_readline(&mut self, handle: u32) -> Option<Vec<u8>> {
        let _ = handle;
        None
    }
    fn file_readline_raw(&mut self, handle: u32) -> Option<Vec<u8>> {
        let _ = handle;
        None
    }
    fn file_close(&mut self, handle: u32) {
        eprintln!("[FILE] close handle={}", handle);
    }
    fn voice_aux_play(&mut self, name: &[u8], loop_: bool) {
        eprintln!(
            "[AUDIO] voice_aux {:?} loop={}", String::from_utf8_lossy(name), loop_
        );
    }
    fn voice_aux_get_stat(&mut self) -> i32 {
        0
    }
    fn bgm_aux_wait_one_tick(&mut self) {}
    fn load_script(&mut self, _name: &str) -> Option<LoadedScript> {
        eprintln!("[SCRIPT] load {:?} — not wired in this host", _name);
        None
    }
    fn deliver_input(&mut self, _input: Input) {}
    fn get_effect_speed(&mut self) -> i32 {
        1000
    }
    fn set_effect_speed(&mut self, _speed: i32) {}
    fn sprite_exists(&mut self, _handle: u32) -> bool {
        false
    }
    fn sprite_alfa_set(&mut self, _sprite: u32, _alpha: i32, _animate: i32) {}
    fn sprite_alfa_define(&mut self, _sprite: u32, _keyframes: &[(i32, i32)]) {}
    fn sprite_animate_define(&mut self, _sprite: u32, _keyframes: &[(i32, i32)]) {}
    fn sprite_animate_define_aligned(
        &mut self,
        sprite: u32,
        keyframes: &[(i32, i32)],
        _total_duration: i32,
    ) {
        self.sprite_animate_define(sprite, keyframes);
    }
    fn sprite_animate_add(&mut self, _sprite: u32, _keyframes: &[(i32, i32)]) {}
    fn page_is_invalid(&mut self, _page: u32) -> bool {
        false
    }
    fn normalize_scheduler_delta(&mut self) {}
    fn scene_freeze_begin(&mut self) {}
    fn scene_freeze_end(&mut self) {}
    fn scene_freeze_active(&mut self) -> bool {
        false
    }
    fn present_epoch(&mut self) -> u64 {
        0
    }
    fn request_present(&mut self) {}
    fn scene_set_origin(&mut self, _x: i32, _y: i32) {}
    fn unimplemented_handler(&mut self, hash: u32, name: &str, args: &[Value]) {
        eprintln!("[UNIMPL] handler 0x{:08X} {} args={:?}", hash, name, args);
    }
    fn sys_skip_logic(&mut self, flag: u16) {
        eprintln!("[SYS] skip_logic flag=0x{:04X}", flag);
    }
    fn text_line(&mut self, _site: &TextSite, bytes: &[u8]) {
        use crate::text::{format_control, tokenize, TextToken};
        let tokens = tokenize(bytes);
        let mut line = String::new();
        for tok in &tokens {
            match tok {
                TextToken::Text(b) => {
                    let (cow, _, _) = encoding_rs::SHIFT_JIS.decode(b);
                    line.push_str(&cow);
                }
                TextToken::Control(cc) => {
                    line.push_str(&format_control(cc));
                }
            }
        }
        eprintln!("[TEXT] {}", line);
        for tok in &tokens {
            if let TextToken::Control(cc) = tok {
                self.text_control(cc);
            }
        }
    }
    fn text_render(&mut self) -> Option<TextRenderInfo> {
        eprintln!("[TEXT] render");
        None
    }
    fn sys_log_format(&mut self, fmt: &[u8]) {
        eprintln!("[SYS] log_format {} bytes", fmt.len());
    }
    fn text_control(&mut self, segment: &crate::text::ControlCode) {
        let _ = segment;
    }
    fn text_voice_play(&mut self, channel: u8, name: &[u8], mode: i32) {
        eprintln!(
            "[TEXT] voice channel={channel} name={:?} mode={mode}",
            String::from_utf8_lossy(name)
        );
    }
    fn take_text_line_localized_parts(&mut self) -> Vec<LocalizedLinePart> {
        Vec::new()
    }
    fn take_text_line_transcoded(&mut self) -> bool {
        false
    }
    fn localized_first_line_enabled(&self) -> bool {
        false
    }
    fn translate_popup_name(&self, name: &[u8]) -> Option<String> {
        let _ = name;
        None
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextSite {
    pub script_name: Vec<u8>,
    pub render_offset: usize,
    pub code_crc32: u32,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayTextSite {
    pub script_name: Vec<u8>,
    pub render_offset: usize,
    pub code_crc32: u32,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalizedLinePart {
    Text(String),
    Wait,
    Newline,
    NewlineRelative,
}
impl LocalizedLinePart {
    pub fn from_control_kind(kind: crate::text::ControlCodeKind) -> Option<Self> {
        match kind {
            crate::text::ControlCodeKind::Wait => Some(Self::Wait),
            crate::text::ControlCodeKind::Newline => Some(Self::Newline),
            crate::text::ControlCodeKind::NewlineRelative => Some(Self::NewlineRelative),
            _ => None,
        }
    }
}
pub struct LoadedScript {
    pub name: String,
    pub code: Vec<u8>,
    pub code_crc32: u32,
    pub main_offset: u32,
    pub line_count: u32,
    pub entries: Vec<(u32, u32)>,
}
#[derive(Default)]
pub struct TraceHost;
impl Host for TraceHost {}
pub struct FileTraceHost {
    pub search_root: std::path::PathBuf,
    pub data_root: Option<std::path::PathBuf>,
    text_files: std::collections::HashMap<u32, TextFileState>,
    next_handle: u32,
}
struct TextFileState {
    data: Vec<u8>,
    cursor: usize,
    pending_tail: Option<Vec<u8>>,
}
impl FileTraceHost {
    pub fn new(search_root: impl Into<std::path::PathBuf>) -> Self {
        Self {
            search_root: search_root.into(),
            data_root: None,
            text_files: std::collections::HashMap::new(),
            next_handle: 1,
        }
    }
    pub fn with_data_root(mut self, root: impl Into<std::path::PathBuf>) -> Self {
        self.data_root = Some(root.into());
        self
    }
    fn resolve(&self, name: &str) -> Option<std::path::PathBuf> {
        let leaf = name.split(['/', '\\']).next_back().unwrap_or(name);
        let stem = leaf.rsplit_once('.').map_or(leaf, |(stem, _)| stem).to_uppercase();
        let target = format!("{}.MJO", stem);
        walk_dir(&self.search_root, &target)
    }
}
impl Host for FileTraceHost {
    fn load_script(&mut self, name: &str) -> Option<LoadedScript> {
        let path = self.resolve(name)?;
        let data = std::fs::read(&path).ok()?;
        let mjo = formats::script::MjoFile::parse(&data).ok()?;
        eprintln!(
            "[SCRIPT] loaded {:?} → {} ({} entries, {} bytes code)", name, path
            .display(), mjo.entries.len(), mjo.data.len()
        );
        let code_crc32 = formats::crypto::crc32(&mjo.data);
        Some(LoadedScript {
            name: name.to_string(),
            code: mjo.data,
            code_crc32,
            main_offset: mjo.main_offset,
            line_count: mjo.line_count,
            entries: mjo.entries.iter().map(|e| (e.name_hash, e.offset)).collect(),
        })
    }
    fn file_open(&mut self, name: &[u8]) -> u32 {
        let name_str = {
            let (cow, _, _) = encoding_rs::SHIFT_JIS.decode(name);
            cow.trim_end_matches('\0').to_string()
        };
        let data = self
            .data_root
            .as_ref()
            .and_then(|root| { find_file_recursive(root, &name_str) });
        match data {
            Some(data) => {
                let handle = self.next_handle;
                self.next_handle += 1;
                eprintln!(
                    "[FILE] opened {:?} → handle {} ({} bytes)", name_str, handle, data
                    .len()
                );
                self.text_files
                    .insert(
                        handle,
                        TextFileState {
                            data,
                            cursor: 0,
                            pending_tail: None,
                        },
                    );
                handle
            }
            None => {
                eprintln!("[FILE] not found {:?}", name_str);
                0
            }
        }
    }
    fn file_readline(&mut self, handle: u32) -> Option<Vec<u8>> {
        let file = self.text_files.get_mut(&handle)?;
        loop {
            if let Some(tail) = file.pending_tail.take() {
                return Some(tail);
            }
            let line = read_next_line(&file.data, &mut file.cursor)?;
            let token = parse_line_token(&line);
            if token.head.is_empty() && token.tail.is_none() {
                continue;
            }
            file.pending_tail = token.tail;
            return Some(token.head);
        }
    }
    fn file_readline_raw(&mut self, handle: u32) -> Option<Vec<u8>> {
        let file = self.text_files.get_mut(&handle)?;
        loop {
            let line = match file.pending_tail.take() {
                Some(tail) => tail,
                None => read_next_line(&file.data, &mut file.cursor)?,
            };
            let line = parse_raw_line(&line);
            if !line.is_empty() {
                return Some(line);
            }
        }
    }
    fn file_close(&mut self, handle: u32) {
        if self.text_files.remove(&handle).is_some() {
            eprintln!("[FILE] closed handle {}", handle);
        }
    }
}
fn walk_dir(root: &std::path::Path, filename: &str) -> Option<std::path::PathBuf> {
    let entries = std::fs::read_dir(root).ok()?;
    let target_upper = filename.to_uppercase();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = walk_dir(&path, filename) {
                return Some(found);
            }
        } else if let Some(fname) = path.file_name().and_then(|s| s.to_str()) {
            if fname.to_uppercase() == target_upper {
                return Some(path);
            }
        }
    }
    None
}
fn find_file_recursive(root: &std::path::Path, name: &str) -> Option<Vec<u8>> {
    if let Ok(data) = std::fs::read(root.join(name)) {
        return Some(data);
    }
    let bare = name.rsplit(['/', '\\']).next().unwrap_or(name);
    if let Some(path) = walk_dir(root, bare) {
        if let Ok(data) = std::fs::read(&path) {
            return Some(data);
        }
    }
    None
}
fn read_next_line(data: &[u8], cursor: &mut usize) -> Option<Vec<u8>> {
    if *cursor >= data.len() {
        return None;
    }
    let start = *cursor;
    let end = data[*cursor..]
        .iter()
        .position(|&b| b == b'\n')
        .map(|i| *cursor + i)
        .unwrap_or(data.len());
    *cursor = (end + 1).min(data.len());
    let mut line_end = end;
    if line_end > start && data[line_end - 1] == b'\r' {
        line_end -= 1;
    }
    Some(data[start..line_end].to_vec())
}
struct ParsedLine {
    head: Vec<u8>,
    tail: Option<Vec<u8>>,
}
fn parse_line_token(line: &[u8]) -> ParsedLine {
    let mut s = line;
    if let Some(i) = s.iter().position(|&b| b == b';') {
        s = &s[..i];
    }
    s = strip_substring(s, b"//");
    let (head_raw, tail) = match s.iter().position(|&b| b == b',') {
        Some(i) => {
            let before = &s[..i];
            let after = &s[i + 1..];
            let tail_trim = trim_trailing(after);
            if tail_trim.is_empty() {
                (before, None)
            } else {
                (before, Some(after.to_vec()))
            }
        }
        None => (s, None),
    };
    let head = trim_whitespace(head_raw);
    let head = unwrap_quotes(head);
    ParsedLine {
        head: head.to_vec(),
        tail: tail.map(|t| t.to_vec()),
    }
}
fn parse_raw_line(line: &[u8]) -> Vec<u8> {
    let mut s = line;
    if let Some(i) = s.iter().position(|&b| b == b';') {
        s = &s[..i];
    }
    s = strip_substring(s, b"//");
    trim_whitespace(s).to_vec()
}
fn trim_whitespace(s: &[u8]) -> &[u8] {
    let mut start = 0;
    while start < s.len() && s[start] <= 0x20 {
        start += 1;
    }
    let mut end = s.len();
    while end > start && s[end - 1] <= 0x20 {
        end -= 1;
    }
    &s[start..end]
}
fn trim_trailing(s: &[u8]) -> &[u8] {
    let mut end = s.len();
    while end > 0 && s[end - 1] <= 0x20 {
        end -= 1;
    }
    &s[..end]
}
fn strip_substring<'a>(haystack: &'a [u8], needle: &[u8]) -> &'a [u8] {
    if needle.is_empty() || haystack.len() < needle.len() {
        return haystack;
    }
    for i in 0..=(haystack.len() - needle.len()) {
        if &haystack[i..i + needle.len()] == needle {
            return &haystack[..i];
        }
    }
    haystack
}
fn unwrap_quotes(s: &[u8]) -> &[u8] {
    if s.len() >= 2 && s[0] == b'"' && s[s.len() - 1] == b'"' {
        &s[1..s.len() - 1]
    } else {
        s
    }
}
fn read_stdin_input() -> Option<Input> {
    let mut line = String::new();
    let n = io::stdin().lock().read_line(&mut line).ok()?;
    if n == 0 {
        return None;
    }
    let line = line.trim();
    let mut parts = line.split_whitespace();
    Some(
        match parts.next() {
            Some("click") => {
                let x = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
                let y = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
                Input::Click { x, y }
            }
            Some("key") => {
                let name = parts.next().unwrap_or("return");
                let virtual_key = parse_trace_virtual_key(name).unwrap_or(0x0D);
                Input::Key {
                    virtual_key,
                    pressed: true,
                }
            }
            Some("select") => {
                let n = parts.next().and_then(|s| s.parse().ok()).unwrap_or(1);
                Input::Select(n)
            }
            Some("tick") | None => Input::Tick,
            Some("quit") | Some("exit") => return None,
            Some(other) => {
                eprintln!("[INPUT] unknown command {:?}, treating as tick", other);
                let _ = io::stderr().flush();
                Input::Tick
            }
        },
    )
}
fn parse_trace_virtual_key(name: &str) -> Option<u8> {
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
        "return" | "enter" => Some(0x0D),
        "shift" => Some(0x10),
        "ctrl" | "control" => Some(0x11),
        "caps" | "capslock" => Some(0x14),
        "escape" | "esc" => Some(0x1B),
        "space" => Some(0x20),
        "pgup" | "pageup" => Some(0x21),
        "pgdn" | "pagedown" => Some(0x22),
        "left" => Some(0x25),
        "up" => Some(0x26),
        "right" => Some(0x27),
        "down" => Some(0x28),
        _ if lower.len() == 1 => Some(lower.as_bytes()[0].to_ascii_uppercase()),
        _ => {
            lower
                .strip_prefix("0x")
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
                .or_else(|| lower.parse().ok())
        }
    }
}

