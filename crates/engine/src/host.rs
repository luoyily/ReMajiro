use crate::audio::AudioBackend;
use crate::patch::{canonical_script_name, PatchBundle};
use crate::render_model::{FrameMark, PageOp, RenderFrame, RenderPage, RenderQuad};
use crate::storage::{FsStore, SaveStore};
use crate::text::{FontoutBox, TextRenderer};
use crate::vfs::Vfs;
use crate::video::{MoviePlayer, MovieTarget};
use crate::Instant;
use formats::save::SavLayout;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use vm::host::{
    FrameInputCallbacks, Host, Hotspot, HotspotEvent, Input, LoadedScript, PointerButton,
    TextRenderAction, TextRenderInfo,
};
const ALPHA_PAGE_BIT: u32 = 0x8000_0000;
#[derive(Debug, Clone, Copy)]
struct HotspotRuntime {
    hotspot: Hotspot,
    state: i32,
    created_frame_depth: usize,
    keyboard_marked: bool,
}
#[derive(Debug, Default)]
struct ContextInputState {
    hotspots: Vec<HotspotRuntime>,
    origin: (i32, i32),
    frames: HashMap<usize, FrameInputRuntime>,
    navigation_reset_ms: i32,
    latch_generation: u64,
    button_latches: [bool; 3],
}
#[derive(Debug)]
struct FrameInputRuntime {
    callbacks: FrameInputCallbacks,
    keyboard_selected: Option<usize>,
    keyboard_anchor: Option<usize>,
}
impl Default for FrameInputRuntime {
    fn default() -> Self {
        Self {
            callbacks: FrameInputCallbacks {
                event_a: 0,
                event_b: 0,
                event_c: 0,
                key_event: 0,
            },
            keyboard_selected: None,
            keyboard_anchor: None,
        }
    }
}
fn base_page_handle(handle: u32) -> u32 {
    handle & !ALPHA_PAGE_BIT
}
fn is_alpha_page(handle: u32) -> bool {
    handle & ALPHA_PAGE_BIT != 0
}
fn crop_text_presentation(
    presentation: &ActiveTextPresentation,
    source_rect: [i32; 4],
    destination_rect: [i32; 4],
    destination_width: u32,
    destination_height: u32,
) -> Option<CroppedTextPresentation> {
    let [source_x, source_y, source_width, source_height] = source_rect;
    let [destination_x, destination_y, destination_rect_width, destination_rect_height,
    ] = destination_rect;
    if source_width <= 0 || source_height <= 0 || source_width != destination_rect_width
        || source_height != destination_rect_height || presentation.scale <= 1
    {
        return None;
    }
    let scale = i64::from(presentation.scale);
    let source_left = i64::from(source_x) * scale;
    let source_top = i64::from(source_y) * scale;
    let source_right = i64::from(source_x.saturating_add(source_width)) * scale;
    let source_bottom = i64::from(source_y.saturating_add(source_height)) * scale;
    let layer_left = i64::from(presentation.physical_x);
    let layer_top = i64::from(presentation.physical_y);
    let layer_right = layer_left + i64::from(presentation.width);
    let layer_bottom = layer_top + i64::from(presentation.height);
    let mut crop_left = source_left.max(layer_left);
    let mut crop_top = source_top.max(layer_top);
    let mut crop_right = source_right.min(layer_right);
    let mut crop_bottom = source_bottom.min(layer_bottom);
    if crop_left >= crop_right || crop_top >= crop_bottom {
        return None;
    }
    let translate_x = i64::from(destination_x.saturating_sub(source_x)) * scale;
    let translate_y = i64::from(destination_y.saturating_sub(source_y)) * scale;
    let destination_limit_x = i64::from(destination_width) * scale;
    let destination_limit_y = i64::from(destination_height) * scale;
    let destination_left = (crop_left + translate_x).max(0);
    let destination_top = (crop_top + translate_y).max(0);
    let destination_right = (crop_right + translate_x).min(destination_limit_x);
    let destination_bottom = (crop_bottom + translate_y).min(destination_limit_y);
    if destination_left >= destination_right || destination_top >= destination_bottom {
        return None;
    }
    crop_left += destination_left - (crop_left + translate_x);
    crop_top += destination_top - (crop_top + translate_y);
    crop_right -= (crop_right + translate_x) - destination_right;
    crop_bottom -= (crop_bottom + translate_y) - destination_bottom;
    let crop_width = usize::try_from(crop_right - crop_left).ok()?;
    let crop_height = usize::try_from(crop_bottom - crop_top).ok()?;
    let layer_width = usize::try_from(presentation.width).ok()?;
    let layer_height = usize::try_from(presentation.height).ok()?;
    let expected_len = layer_width.checked_mul(layer_height)?.checked_mul(4)?;
    if presentation.pixels.len() != expected_len {
        return None;
    }
    let source_offset_x = usize::try_from(crop_left - layer_left).ok()?;
    let source_offset_y = usize::try_from(crop_top - layer_top).ok()?;
    if source_offset_x.checked_add(crop_width)? > layer_width
        || source_offset_y.checked_add(crop_height)? > layer_height
    {
        return None;
    }
    let mut pixels = Vec::with_capacity(
        crop_width.checked_mul(crop_height)?.checked_mul(4)?,
    );
    let source_stride = layer_width.checked_mul(4)?;
    let row_bytes = crop_width.checked_mul(4)?;
    for row in 0..crop_height {
        let offset = (source_offset_y + row)
            .checked_mul(source_stride)?
            .checked_add(source_offset_x.checked_mul(4)?)?;
        pixels.extend_from_slice(presentation.pixels.get(offset..offset + row_bytes)?);
    }
    Some(CroppedTextPresentation {
        physical_x: i32::try_from(destination_left).ok()?,
        physical_y: i32::try_from(destination_top).ok()?,
        width: u32::try_from(crop_width).ok()?,
        height: u32::try_from(crop_height).ok()?,
        pixels: Arc::new(pixels),
    })
}
fn native_missing_page_is_indexed(filename: &[u8]) -> bool {
    let mut decoded = sjis_to_string(filename);
    decoded.push('.');
    decoded.contains("_.")
}
fn refresh_indexed_pixels_rect(
    page: &mut Page,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> usize {
    let (Some(samples), Some(palette)) = (
        page.indexed_samples.as_deref(),
        page.indexed_palette.as_deref(),
    ) else {
        return 0;
    };
    debug_assert_eq!(samples.len(), page.width as usize * page.height as usize);
    debug_assert_eq!(palette.len(), 256);
    let x0 = x.max(0).min(page.width as i32) as usize;
    let y0 = y.max(0).min(page.height as i32) as usize;
    let x1 = x.saturating_add(width).max(0).min(page.width as i32) as usize;
    let y1 = y.saturating_add(height).max(0).min(page.height as i32) as usize;
    if x1 <= x0 || y1 <= y0 {
        return 0;
    }
    let sample_stride = page.width as usize;
    let pixel_stride = sample_stride * 4;
    let pixels = Arc::make_mut(&mut page.pixels);
    for row in y0..y1 {
        let sample_row = &samples[row * sample_stride + x0..row * sample_stride + x1];
        let pixel_row = &mut pixels[row * pixel_stride
            + x0 * 4..row * pixel_stride + x1 * 4];
        for (sample, pixel) in sample_row.iter().zip(pixel_row.chunks_exact_mut(4)) {
            let alpha = pixel[3];
            pixel.copy_from_slice(&palette[*sample as usize]);
            pixel[3] = alpha;
        }
    }
    (x1 - x0) * (y1 - y0)
}
fn refresh_indexed_pixels(page: &mut Page) -> usize {
    refresh_indexed_pixels_rect(page, 0, 0, page.width as i32, page.height as i32)
}
#[derive(Clone)]
struct Sprite {
    page: u32,
    target_page: u32,
    group_id: u32,
    opacity: u8,
    source: Option<SpriteSource>,
    original_source_position: (i32, i32),
    overlay: bool,
    position: Option<(i32, i32)>,
    position_animation: Option<SpritePositionAnimation>,
    alpha_animation: Option<SpriteAlphaAnimation>,
    frame_animation: Option<SpriteFrameAnimation>,
}
#[derive(Clone)]
struct SpritePositionAnimation {
    state: i32,
    start: (i32, i32),
    delta: (i32, i32),
    started_ms: i32,
    duration_ms: i32,
    easing_flags: i32,
}
#[derive(Clone)]
struct SpriteAlphaAnimation {
    started_ms: i32,
    segment_duration_ms: i32,
    segment_index: i32,
    previous_transparency: u8,
    keyframes: Vec<(u8, i32)>,
}
#[derive(Clone)]
struct SpriteFrameAnimation {
    state: i32,
    started_ms: i32,
    segment_duration_ms: i32,
    segment_index: i32,
    keyframes: Vec<(i32, i32)>,
}
#[derive(Clone)]
struct SpriteRotationAnimation {
    state: i32,
    current: f32,
    previous: f32,
    target: f32,
    started_ms: i32,
    duration_ms: i32,
}
#[derive(Clone)]
struct SpriteScaleAnimation {
    state: i32,
    current: f32,
    started_ms: i32,
    segment_duration_ms: i32,
    segment_index: i32,
    previous: f32,
    easing_flags: i32,
    ease_out_mask: i32,
    ease_in_mask: i32,
    keyframes: Vec<(f32, i32)>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct SpriteSource {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct SmoothPageState {
    source_page: u32,
    source_revision: u64,
    source: SpriteSource,
    target: Option<(SpriteSource, u32)>,
}
#[derive(Clone)]
pub(crate) struct Page {
    width: u32,
    height: u32,
    pixels: Arc<Vec<u8>>,
    presentation_seed: Arc<Vec<u8>>,
    presentation: Option<Arc<crate::render_model::PresentationImage>>,
    indexed_samples: Option<Arc<Vec<u8>>>,
    indexed_palette: Option<Arc<Vec<[u8; 4]>>>,
    revision: u64,
    alpha_masked: bool,
    draw_mode: u32,
}
struct PresentedScene {
    display_page: Option<u32>,
    pages: HashMap<u32, Page>,
    quads: Vec<RenderQuad>,
}
#[derive(Clone)]
struct ActiveTextPresentation {
    source_page: u32,
    console_rect: [i32; 4],
    scale: u32,
    physical_x: i32,
    physical_y: i32,
    width: u32,
    height: u32,
    pixels: Arc<Vec<u8>>,
}
struct CroppedTextPresentation {
    physical_x: i32,
    physical_y: i32,
    width: u32,
    height: u32,
    pixels: Arc<Vec<u8>>,
}
pub(crate) mod page_source;
pub mod script_source;
mod sprite_ops;
use page_source::{CachedPageTemplate, PageSource};
use sprite_ops::{
    advance_alpha_animation, advance_frame_animation, advance_position_animation,
    advance_rotation_animation, advance_scale_animation, native_8bit_sample,
    scale_set_animation, source_for_animation_frame, sprite_source_rect,
};
struct TextFile {
    data: Vec<u8>,
    cursor: usize,
    pending_tail: Option<Vec<u8>>,
}
pub struct EngineHost {
    internal_w: u32,
    internal_h: u32,
    sav_layout: SavLayout,
    vfs: Arc<Vfs>,
    patch: Option<Arc<PatchBundle>>,
    ir_dir: Option<std::path::PathBuf>,
    ir_scripts: Option<std::sync::Arc<std::collections::HashMap<String, String>>>,
    ir_text_overrides: HashMap<
        String,
        std::collections::BTreeMap<u32, crate::patch::Message>,
    >,
    ir_replay_messages: Vec<crate::patch::Message>,
    ir_display_messages: Vec<crate::patch::Message>,
    text_renderer: TextRenderer,
    audio: AudioBackend,
    movie: MoviePlayer,
    pages: HashMap<u32, Page>,
    page_source: PageSource,
    page_debug_names: HashMap<u32, Arc<str>>,
    render_debug_names: bool,
    page_presentation_names: HashMap<u32, Arc<str>>,
    alpha_bindings: HashMap<u32, u32>,
    owned_alpha_bindings: HashSet<u32>,
    sprites: HashMap<u32, Sprite>,
    sprite_order: Vec<u32>,
    scene_resident_sprites: HashSet<u32>,
    scene_resident_pages: HashSet<u32>,
    sprite_visibility_depths: HashMap<u32, i32>,
    sprite_auto_release_pages: HashSet<u32>,
    sprite_smooth_animation: HashSet<u32>,
    sprite_smooth_pages: HashMap<u32, u32>,
    sprite_smooth_page_states: HashMap<u32, SmoothPageState>,
    sprite_rotations: HashMap<u32, SpriteRotationAnimation>,
    sprite_xmodifies: HashMap<u32, SpriteScaleAnimation>,
    sprite_ymodifies: HashMap<u32, SpriteScaleAnimation>,
    next_handle: u32,
    next_page_revision: u64,
    dirty_pages: HashSet<u32>,
    dirty_regions: HashMap<u32, Option<[i32; 4]>>,
    pending_page_ops: Vec<PageOp>,
    page_op_destinations: HashSet<u32>,
    frame_marks: Vec<FrameMark>,
    active_text_presentation: Option<ActiveTextPresentation>,
    published_pages: HashSet<u32>,
    released_pages: Vec<u32>,
    released_page_snapshots: HashMap<u32, Page>,
    pending_quads: Vec<RenderQuad>,
    display_page: Option<u32>,
    display_epoch: u64,
    present_epoch: u64,
    presented_scene: Option<PresentedScene>,
    render_dirty: bool,
    scene_dirty_freeze: i32,
    reported_render_skips: HashSet<(u32, &'static str)>,
    composite_derived_pages: HashMap<u32, &'static str>,
    reported_composite_writebacks: HashSet<(&'static str, u32, u32)>,
    reported_composite_presents: HashSet<u32>,
    composite_read_counts: HashMap<&'static str, u64>,
    sprites_created: u64,
    sprites_released: u64,
    peak_quads: usize,
    text_files: HashMap<u32, TextFile>,
    mouse_pos: (i32, i32),
    logical_pointer_override: Option<(i32, i32)>,
    viewport_offset: (i32, i32),
    cursor_inside: bool,
    pending_cursor_warp: Option<(i32, i32)>,
    saved_mouse_pos_before_warp: Option<(i32, i32)>,
    cursor_visible: bool,
    cursor_visibility_dirty: bool,
    key_live_held: [bool; 256],
    key_live_pressed_ms: [i32; 256],
    key_snapshot_held: [bool; 256],
    key_snapshot_pressed_ms: [i32; 256],
    input_snapshot_generation: u64,
    snapshot_button_edges: [bool; 3],
    click_suppression_mask: u8,
    click_suppression_remaining: i32,
    click_suppression_last_ms: i32,
    last_key_down: u8,
    pointer_moved_since_keyboard: bool,
    capslock_on: bool,
    input_contexts: HashMap<u32, ContextInputState>,
    native_mode_bits: u8,
    native_mode_special: bool,
    frontbuffer: u32,
    base_page: u32,
    clock_start: Instant,
    clock_timestamp_ms: i32,
    last_animation_update_ms: Option<i32>,
    clock_scale: f64,
    clock_anchor_real_ms: u32,
    clock_offset_ms: i32,
    rct_key: [u8; 1024],
    unimplemented_counts: HashMap<u32, u64>,
    config_values: HashMap<Vec<u8>, i32>,
    config_file: String,
    window_fullscreen: bool,
    pending_fullscreen: Option<bool>,
    voice_ts_flag: bool,
    input_dispatch_mode: i32,
    display_mode: i32,
    picture_hashes: HashSet<u32>,
    selected_font_face: Vec<u8>,
    selected_font_mode: i32,
    save_system_ready: bool,
    save_dialog_avail_flag: bool,
    save_last_command_active: bool,
    storage: Arc<dyn SaveStore>,
    save_description: Vec<u8>,
    last_save_meta_bytes: Vec<u8>,
    last_localized_parts: Vec<vm::host::LocalizedLinePart>,
    last_line_transcoded: bool,
    save_thumbnail: Option<formats::save::Thumbnail>,
    exit_requested: bool,
}
impl EngineHost {
    pub fn new(vfs: Vfs) -> Self {
        Self::new_with_text(
            vfs,
            TextRenderer::disabled(),
            None,
            &crate::profile::GameProfile::OWARUSEKAI,
        )
    }
    pub fn with_font(vfs: Vfs, font_path: Option<&Path>) -> Result<Self, String> {
        Self::with_font_and_patch(vfs, font_path, None)
    }
    pub fn with_font_and_patch(
        vfs: Vfs,
        font_path: Option<&Path>,
        patch: Option<PatchBundle>,
    ) -> Result<Self, String> {
        Self::with_profile(
            vfs,
            font_path,
            patch,
            &crate::profile::GameProfile::OWARUSEKAI,
        )
    }
    pub fn with_profile(
        vfs: Vfs,
        font_path: Option<&Path>,
        patch: Option<PatchBundle>,
        profile: &crate::profile::GameProfile,
    ) -> Result<Self, String> {
        let patch_fonts = patch
            .as_ref()
            .map(PatchBundle::font_paths)
            .unwrap_or_default();
        let text_renderer = TextRenderer::load_with_fallbacks(font_path, patch_fonts)?;
        if let Some(path) = text_renderer.font_source() {
            eprintln!("[FONT] using {}", path.display());
        }
        Ok(Self::new_with_text(vfs, text_renderer, patch, profile))
    }
    pub fn with_profile_and_font_bytes(
        vfs: Vfs,
        font_bytes: Option<Vec<u8>>,
        patch_fonts: Vec<Vec<u8>>,
        patch: Option<PatchBundle>,
        profile: &crate::profile::GameProfile,
    ) -> Result<Self, String> {
        let mut fallbacks = patch_fonts;
        let from_patch = font_bytes.is_none();
        let primary = match font_bytes {
            Some(bytes) => bytes,
            None => {
                fallbacks
                    .first()
                    .cloned()
                    .ok_or_else(|| {
                        "no font bytes: the game directory must provide font.ttf or font.ttc"
                            .to_string()
                    })?
            }
        };
        if from_patch {
            fallbacks.remove(0);
        }
        let text_renderer = TextRenderer::from_font_bytes(primary, fallbacks)?;
        Ok(Self::new_with_text(vfs, text_renderer, patch, profile))
    }
    fn new_with_text(
        vfs: Vfs,
        text_renderer: TextRenderer,
        patch: Option<PatchBundle>,
        profile: &crate::profile::GameProfile,
    ) -> Self {
        let internal_w = profile.internal_w;
        let internal_h = profile.internal_h;
        let vfs = Arc::new(vfs);
        let patch = patch.map(Arc::new);
        let fb_pixels = vec![0u8; (internal_w as usize) * (internal_h as usize) * 4];
        let sav_layout = profile.sav;
        let mut host = Self {
            internal_w,
            internal_h,
            sav_layout,
            vfs,
            patch,
            ir_dir: None,
            ir_scripts: None,
            ir_text_overrides: HashMap::new(),
            ir_replay_messages: Vec::new(),
            ir_display_messages: Vec::new(),
            text_renderer,
            audio: AudioBackend::new(),
            movie: MoviePlayer::new(),
            pages: HashMap::new(),
            page_debug_names: HashMap::new(),
            render_debug_names: false,
            page_source: PageSource::new(),
            page_presentation_names: HashMap::new(),
            alpha_bindings: HashMap::new(),
            owned_alpha_bindings: HashSet::new(),
            sprites: HashMap::new(),
            sprite_order: Vec::new(),
            scene_resident_sprites: HashSet::new(),
            scene_resident_pages: HashSet::new(),
            sprite_visibility_depths: HashMap::new(),
            sprite_auto_release_pages: HashSet::new(),
            sprite_smooth_animation: HashSet::new(),
            sprite_smooth_pages: HashMap::new(),
            sprite_smooth_page_states: HashMap::new(),
            sprite_rotations: HashMap::new(),
            sprite_xmodifies: HashMap::new(),
            sprite_ymodifies: HashMap::new(),
            next_handle: 1,
            next_page_revision: 1,
            dirty_pages: HashSet::new(),
            dirty_regions: HashMap::new(),
            pending_page_ops: Vec::new(),
            page_op_destinations: HashSet::new(),
            frame_marks: Vec::new(),
            active_text_presentation: None,
            published_pages: HashSet::new(),
            released_pages: Vec::new(),
            released_page_snapshots: HashMap::new(),
            pending_quads: Vec::new(),
            display_page: None,
            display_epoch: 0,
            present_epoch: 0,
            presented_scene: None,
            render_dirty: true,
            scene_dirty_freeze: 0,
            reported_render_skips: HashSet::new(),
            composite_derived_pages: HashMap::new(),
            reported_composite_writebacks: HashSet::new(),
            reported_composite_presents: HashSet::new(),
            composite_read_counts: HashMap::new(),
            sprites_created: 0,
            sprites_released: 0,
            peak_quads: 0,
            text_files: HashMap::new(),
            mouse_pos: (0, 0),
            logical_pointer_override: None,
            viewport_offset: (0, 0),
            cursor_inside: false,
            pending_cursor_warp: None,
            saved_mouse_pos_before_warp: None,
            cursor_visible: true,
            cursor_visibility_dirty: false,
            key_live_held: [false; 256],
            key_live_pressed_ms: [0; 256],
            key_snapshot_held: [false; 256],
            key_snapshot_pressed_ms: [0; 256],
            input_snapshot_generation: 0,
            snapshot_button_edges: [false; 3],
            click_suppression_mask: 0,
            click_suppression_remaining: 0,
            click_suppression_last_ms: 0,
            last_key_down: 0,
            pointer_moved_since_keyboard: false,
            capslock_on: false,
            input_contexts: HashMap::new(),
            native_mode_bits: 0,
            native_mode_special: false,
            frontbuffer: 0,
            base_page: 0,
            clock_start: Instant::now(),
            clock_timestamp_ms: 0,
            last_animation_update_ms: None,
            clock_scale: 1.0,
            clock_anchor_real_ms: 0,
            clock_offset_ms: 0,
            rct_key: formats::image::build_key_from_hash(profile.default_rct_key_hash),
            unimplemented_counts: HashMap::new(),
            config_values: HashMap::new(),
            config_file: format!("{}_config.dat", profile.bin_name),
            window_fullscreen: false,
            pending_fullscreen: None,
            voice_ts_flag: false,
            input_dispatch_mode: 0,
            display_mode: 0,
            picture_hashes: HashSet::new(),
            selected_font_face: Vec::new(),
            selected_font_mode: 0,
            save_system_ready: false,
            save_dialog_avail_flag: false,
            save_last_command_active: false,
            storage: Arc::new(FsStore::new("savedata")),
            save_description: Vec::new(),
            last_save_meta_bytes: Vec::new(),
            last_localized_parts: Vec::new(),
            last_line_transcoded: false,
            save_thumbnail: None,
            exit_requested: false,
        };
        let fb_handle = host.alloc_handle();
        host.insert_page(fb_handle, internal_w, internal_h, fb_pixels, None, false);
        host.frontbuffer = fb_handle;
        host.base_page = fb_handle;
        host.display_page = Some(fb_handle);
        host.display_epoch = 1;
        eprintln!(
            "[GFX] default frontbuffer → handle {} ({}×{})", fb_handle, internal_w,
            internal_h
        );
        host
    }
    pub fn set_save_dir(&mut self, path: impl Into<PathBuf>) {
        self.set_save_store(Arc::new(FsStore::new(path.into())));
    }
    pub fn set_save_store(&mut self, store: Arc<dyn SaveStore>) {
        self.storage = store;
        self.load_config_file_impl();
        self.load_picture_hashes_impl();
    }
    pub fn save_store(&self) -> Arc<dyn SaveStore> {
        Arc::clone(&self.storage)
    }
    pub fn set_ir_dir(&mut self, path: impl Into<PathBuf>) {
        self.ir_dir = Some(path.into());
    }
    pub fn set_ir_scripts(
        &mut self,
        scripts: std::sync::Arc<std::collections::HashMap<String, String>>,
    ) {
        self.ir_scripts = Some(scripts);
    }
    pub fn enable_render_profiling(&mut self) {
        self.render_debug_names = true;
    }
    pub fn pin_scene_resources(&mut self) {
        self.scene_resident_sprites.extend(self.sprites.keys().copied());
        self.scene_resident_pages.extend(self.pages.keys().copied());
        eprintln!(
            "[SCENE] pinned init resources: {} sprites, {} pages", self
            .scene_resident_sprites.len(), self.scene_resident_pages.len()
        );
    }
    fn alloc_handle(&mut self) -> u32 {
        let h = self.next_handle;
        self.next_handle += 1;
        h
    }
    fn alloc_page_revision(&mut self) -> u64 {
        let revision = self.next_page_revision;
        self.next_page_revision = self.next_page_revision.wrapping_add(1).max(1);
        revision
    }
    fn insert_page(
        &mut self,
        handle: u32,
        width: u32,
        height: u32,
        pixels: Vec<u8>,
        indexed_samples: Option<Vec<u8>>,
        alpha_masked: bool,
    ) {
        let revision = self.alloc_page_revision();
        let indexed_palette = indexed_samples
            .as_ref()
            .map(|_| Arc::new(
                (0..=255).map(|value| [value, value, value, 0xFF]).collect(),
            ));
        let pixels = Arc::new(pixels);
        self.pages
            .insert(
                handle,
                Page {
                    width,
                    height,
                    pixels: pixels.clone(),
                    presentation_seed: pixels,
                    presentation: None,
                    indexed_samples: indexed_samples.map(Arc::new),
                    indexed_palette,
                    revision,
                    alpha_masked,
                    draw_mode: 0,
                },
            );
        self.dirty_pages.insert(handle);
        self.render_dirty = true;
    }
    fn mark_page_dirty(&mut self, handle: u32) {
        let revision = self.alloc_page_revision();
        if let Some(page) = self.pages.get_mut(&handle) {
            page.revision = revision;
            self.dirty_pages.insert(handle);
            self.dirty_regions.insert(handle, None);
            self.render_dirty = true;
        }
    }
    fn mark_page_dirty_rect(
        &mut self,
        handle: u32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) {
        if let Some(page) = self.pages.get_mut(&handle) {
            refresh_indexed_pixels_rect(page, x, y, width, height);
        }
        let revision = self.alloc_page_revision();
        if let Some(page) = self.pages.get_mut(&handle) {
            page.revision = revision;
            self.dirty_pages.insert(handle);
            let rect = [x, y, width, height];
            match self.dirty_regions.entry(handle) {
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(Some(rect));
                }
                std::collections::hash_map::Entry::Occupied(mut entry) => {
                    if let Some(previous) = entry.get_mut() {
                        let left = previous[0].min(x);
                        let top = previous[1].min(y);
                        let right = previous[0]
                            .saturating_add(previous[2])
                            .max(x.saturating_add(width));
                        let bottom = previous[1]
                            .saturating_add(previous[3])
                            .max(y.saturating_add(height));
                        *previous = [left, top, right - left, bottom - top];
                    }
                }
            }
            self.render_dirty = true;
        }
    }
    fn page_revision(&self, handle: u32) -> Option<u64> {
        self.pages.get(&base_page_handle(handle)).map(|page| page.revision)
    }
    fn page_has_alpha_semantics(&self, handle: u32) -> bool {
        let handle = base_page_handle(handle);
        self.alpha_bindings.contains_key(&handle)
            || self.pages.get(&handle).is_some_and(|page| page.alpha_masked)
    }
    fn current_text_presentation_copy(
        &self,
        src_page: u32,
        source_rect: [i32; 4],
        dst_page: u32,
        destination_rect: [i32; 4],
        alpha: i32,
    ) -> Option<(Option<CroppedTextPresentation>, bool)> {
        let presentation = self.active_text_presentation.as_ref()?;
        let [source_x, source_y, source_width, source_height] = source_rect;
        let [_, _, destination_width, destination_height] = destination_rect;
        let [console_x, console_y, console_width, console_height] = presentation
            .console_rect;
        if alpha != 0 || base_page_handle(src_page) != presentation.source_page
            || base_page_handle(src_page) == base_page_handle(dst_page)
            || source_width <= 0 || source_height <= 0
            || source_width != destination_width || source_height != destination_height
            || source_height > 128 || source_y > console_y
            || source_y.saturating_add(source_height)
                < console_y.saturating_add(console_height)
            || source_x >= console_x.saturating_add(console_width)
            || source_x.saturating_add(source_width) <= console_x
        {
            return None;
        }
        let destination = self.pages.get(&base_page_handle(dst_page))?;
        let cropped = crop_text_presentation(
            presentation,
            source_rect,
            destination_rect,
            destination.width,
            destination.height,
        );
        let finished = source_x.saturating_add(source_width)
            >= console_x.saturating_add(console_width);
        Some((cropped, finished))
    }
    fn push_frame_mark(&mut self, name: &str, detail: String) {
        self.frame_marks
            .push(FrameMark {
                name: name.to_owned(),
                detail,
            });
    }
    fn queue_page_op_if_changed(
        &mut self,
        before: Option<u64>,
        destination: u32,
        op: PageOp,
    ) {
        let destination = base_page_handle(destination);
        if before == self.page_revision(destination) {
            return;
        }
        if let (
            Some(
                PageOp::Fill {
                    destination: previous_destination,
                    rect: previous,
                    color: previous_color,
                    mode: previous_mode,
                    parameter: previous_parameter,
                },
            ),
            PageOp::Fill {
                destination: next_destination,
                rect: next,
                color: next_color,
                mode: next_mode,
                parameter: next_parameter,
            },
        ) = (self.pending_page_ops.last_mut(), &op) {
            let same_operation = previous_destination == next_destination
                && previous_color == next_color && previous_mode == next_mode
                && previous_parameter == next_parameter;
            if same_operation && previous[1] == next[1] && previous[3] == next[3]
                && previous[0].saturating_add(previous[2]) == next[0]
            {
                previous[2] = previous[2].saturating_add(next[2]);
                self.page_op_destinations.insert(destination);
                return;
            }
            if same_operation && previous[0] == next[0] && previous[2] == next[2]
                && previous[1].saturating_add(previous[3]) == next[1]
            {
                previous[3] = previous[3].saturating_add(next[3]);
                self.page_op_destinations.insert(destination);
                return;
            }
        }
        self.pending_page_ops.push(op);
        self.page_op_destinations.insert(destination);
    }
    fn queue_logical_rect_if_changed(
        &mut self,
        before: Option<u64>,
        destination: u32,
        rect: [i32; 4],
    ) {
        let destination = base_page_handle(destination);
        if before == self.page_revision(destination) {
            return;
        }
        self.queue_logical_rect(destination, rect);
    }
    fn queue_logical_rect(&mut self, destination: u32, rect: [i32; 4]) {
        let destination = base_page_handle(destination);
        if let Some(page) = self.pages.get(&destination) {
            self.pending_page_ops
                .push(PageOp::UploadLogicalRect {
                    destination,
                    page_width: page.width,
                    page_height: page.height,
                    rect,
                    pixels: page.pixels.clone(),
                });
            self.page_op_destinations.insert(destination);
            self.render_dirty = true;
        }
    }
    fn queue_point_op_if_changed(
        &mut self,
        before: Option<u64>,
        destination: u32,
        point: crate::render_model::PagePoint,
    ) {
        let destination = base_page_handle(destination);
        if before == self.page_revision(destination) {
            return;
        }
        if let Some(PageOp::Points { destination: previous, points }) = self
            .pending_page_ops
            .last_mut()
        {
            if *previous == destination {
                points.push(point);
                self.page_op_destinations.insert(destination);
                return;
            }
        }
        self.pending_page_ops
            .push(PageOp::Points {
                destination,
                points: vec![point],
            });
        self.page_op_destinations.insert(destination);
    }
    fn queue_alpha_fill_mirrors(
        &mut self,
        alpha_page: u32,
        rect: [i32; 4],
        color: u32,
        blended: Option<i32>,
    ) {
        let alpha_page = base_page_handle(alpha_page);
        let owners = self
            .alpha_bindings
            .iter()
            .filter_map(|(&color_page, &bound_alpha)| {
                (bound_alpha == alpha_page).then_some(color_page)
            })
            .collect::<Vec<_>>();
        for destination in owners {
            self.pending_page_ops
                .push(PageOp::Fill {
                    destination,
                    rect,
                    color,
                    mode: if blended.is_some() {
                        crate::render_model::PageFillMode::AlphaBlend
                    } else {
                        crate::render_model::PageFillMode::AlphaReplace
                    },
                    parameter: blended.unwrap_or(0),
                });
            self.page_op_destinations.insert(destination);
        }
    }
    fn queue_alpha_copy_mirrors(
        &mut self,
        source: u32,
        destination: u32,
        source_rect: [i32; 4],
        destination_rect: [i32; 4],
        mode: crate::render_model::PageCopyMode,
    ) {
        let source_base = base_page_handle(source);
        let destination_base = base_page_handle(destination);
        if is_alpha_page(destination) {
            return;
        }
        let destination_is_color_owner = self
            .alpha_bindings
            .contains_key(&destination_base);
        let owners = if destination_is_color_owner {
            if is_alpha_page(source) || !self.alpha_bindings.contains_key(&source_base) {
                return;
            }
            vec![destination_base]
        } else {
            self.alpha_bindings
                .iter()
                .filter_map(|(&color_page, &bound_alpha)| {
                    (bound_alpha == destination_base).then_some(color_page)
                })
                .collect::<Vec<_>>()
        };
        let source_alpha_view = if destination_is_color_owner {
            true
        } else {
            is_alpha_page(source)
        };
        for destination in owners {
            self.pending_page_ops
                .push(PageOp::Copy {
                    source: source_base,
                    source_is_presented_scene: false,
                    destination,
                    source_rect,
                    destination_rect,
                    mode,
                    parameter: 0,
                    source_has_alpha: true,
                    destination_has_alpha: true,
                    source_alpha_view,
                    destination_alpha_view: true,
                });
            self.page_op_destinations.insert(destination);
        }
    }
    fn mark_indexed_page_dirty(&mut self, handle: u32) {
        if let Some(page) = self.pages.get_mut(&handle) {
            refresh_indexed_pixels(page);
        }
        self.mark_page_dirty(handle);
    }
    fn sync_color_alpha(&mut self, color_page: u32) -> bool {
        let Some(color) = self.pages.get(&color_page) else {
            return false;
        };
        self.sync_color_alpha_rect(
            color_page,
            0,
            0,
            color.width as i32,
            color.height as i32,
        )
    }
    fn sync_color_alpha_rect(
        &mut self,
        color_page: u32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> bool {
        let Some(&alpha_page) = self.alpha_bindings.get(&color_page) else {
            return false;
        };
        let Some(alpha) = self.pages.get(&alpha_page).cloned() else {
            return false;
        };
        let Some(color) = self.pages.get_mut(&color_page) else {
            return false;
        };
        color.alpha_masked = true;
        let common_width = color.width.min(alpha.width) as i32;
        let common_height = color.height.min(alpha.height) as i32;
        let x0 = x.max(0).min(common_width);
        let y0 = y.max(0).min(common_height);
        let x1 = x.saturating_add(width).max(0).min(common_width);
        let y1 = y.saturating_add(height).max(0).min(common_height);
        if x1 <= x0 || y1 <= y0 {
            return false;
        }
        let color_stride = color.width as usize * 4;
        let color_pixels = Arc::make_mut(&mut color.pixels);
        for y in y0 as usize..y1 as usize {
            for x in x0 as usize..x1 as usize {
                let anti = native_8bit_sample(&alpha, x, y);
                color_pixels[y * color_stride + x * 4 + 3] = 255 - anti;
            }
        }
        true
    }
    fn refresh_alpha_links(&mut self, changed_page: u32) {
        let mut colors = Vec::new();
        if self.alpha_bindings.contains_key(&changed_page) {
            colors.push(changed_page);
        }
        colors
            .extend(
                self
                    .alpha_bindings
                    .iter()
                    .filter_map(|(&color, &alpha)| {
                        (alpha == changed_page).then_some(color)
                    }),
            );
        colors.sort_unstable();
        colors.dedup();
        for color in colors {
            if self.sync_color_alpha(color) {
                self.mark_page_dirty(color);
            }
        }
    }
    fn make_page_opaque(&mut self, color_page: u32) {
        let changed = self
            .pages
            .get_mut(&color_page)
            .is_some_and(|page| {
                let pixels = Arc::make_mut(&mut page.pixels);
                let mut changed = page.alpha_masked;
                page.alpha_masked = false;
                for alpha in pixels[3..].iter_mut().step_by(4) {
                    changed |= *alpha != 255;
                    *alpha = 255;
                }
                changed
            });
        if changed {
            self.mark_page_dirty(color_page);
        }
    }
    fn refresh_alpha_links_rect(
        &mut self,
        changed_page: u32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) {
        let mut colors = Vec::new();
        if self.alpha_bindings.contains_key(&changed_page) {
            colors.push(changed_page);
        }
        colors
            .extend(
                self
                    .alpha_bindings
                    .iter()
                    .filter_map(|(&color, &alpha)| {
                        (alpha == changed_page).then_some(color)
                    }),
            );
        colors.sort_unstable();
        colors.dedup();
        for color in colors {
            if self.sync_color_alpha_rect(color, x, y, width, height) {
                self.mark_page_dirty_rect(color, x, y, width, height);
            }
        }
    }
    fn sync_bound_alpha_from_color_rect(
        &mut self,
        color_page: u32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> bool {
        let Some(&alpha_page) = self.alpha_bindings.get(&color_page) else {
            return false;
        };
        let Some(color) = self.pages.get(&color_page).cloned() else {
            return false;
        };
        let Some(alpha) = self.pages.get_mut(&alpha_page) else {
            return false;
        };
        let common_width = color.width.min(alpha.width) as i32;
        let common_height = color.height.min(alpha.height) as i32;
        let x0 = x.max(0).min(common_width);
        let y0 = y.max(0).min(common_height);
        let x1 = x.saturating_add(width).max(0).min(common_width);
        let y1 = y.saturating_add(height).max(0).min(common_height);
        if x1 <= x0 || y1 <= y0 {
            return false;
        }
        let color_stride = color.width as usize * 4;
        let alpha_stride = alpha.width as usize * 4;
        let alpha_sample_stride = alpha.width as usize;
        let mut alpha_samples = alpha.indexed_samples.as_mut().map(Arc::make_mut);
        let alpha_pixels = Arc::make_mut(&mut alpha.pixels);
        for row in y0 as usize..y1 as usize {
            for column in x0 as usize..x1 as usize {
                let color_offset = row * color_stride + column * 4;
                let alpha_offset = row * alpha_stride + column * 4;
                let value = 255 - color.pixels[color_offset + 3];
                if let Some(samples) = alpha_samples.as_deref_mut() {
                    samples[row * alpha_sample_stride + column] = value;
                }
                alpha_pixels[alpha_offset..alpha_offset + 3].fill(value);
                alpha_pixels[alpha_offset + 3] = 0xFF;
            }
        }
        self.mark_page_dirty_rect(alpha_page, x, y, width, height);
        true
    }
    fn render_text_page(&mut self) -> Option<TextRenderInfo> {
        self.active_text_presentation = None;
        let configured = self.text_renderer.target_page();
        let target = if self.pages.contains_key(&configured) {
            configured
        } else {
            self.frontbuffer
        };
        let presentation_scale = self
            .patch
            .as_ref()
            .map(|patch| patch.presentation().scale)
            .unwrap_or(1);
        let target_has_alpha = self
            .alpha_bindings
            .contains_key(&base_page_handle(target))
            || self
                .pages
                .get(&base_page_handle(target))
                .is_some_and(|page| page.alpha_masked);
        let trace_before = self.text_renderer.trace_state();
        let (changed, action, presentation, console_rect) = if let Some(page) = self
            .pages
            .get_mut(&target)
        {
            let pixels = Arc::make_mut(&mut page.pixels);
            let result = self
                .text_renderer
                .render_pending_scaled(
                    page.width,
                    page.height,
                    pixels,
                    presentation_scale,
                );
            (result.changed, result.action, result.presentation, result.console_rect)
        } else {
            (false, TextRenderAction::Complete, None, None)
        };
        if changed {
            self.mark_page_dirty(target);
            if presentation_scale > 1 {
                if trace_before.0 {
                    if let Some((left, top, right, bottom)) = trace_before.2 {
                        self.pending_page_ops
                            .push(PageOp::Fill {
                                destination: target,
                                rect: [left, top, right - left, bottom - top],
                                color: 0,
                                mode: crate::render_model::PageFillMode::Clear,
                                parameter: 0,
                            });
                    }
                }
                if let Some(presentation) = presentation {
                    let pixels = Arc::new(presentation.pixels);
                    self.pending_page_ops
                        .push(PageOp::PresentationOverlay {
                            destination: target,
                            physical_x: presentation.physical_x,
                            physical_y: presentation.physical_y,
                            width: presentation.width,
                            height: presentation.height,
                            pixels: pixels.clone(),
                            destination_has_alpha: target_has_alpha,
                        });
                    if let Some(console_rect) = console_rect {
                        self.active_text_presentation = Some(ActiveTextPresentation {
                            source_page: base_page_handle(target),
                            console_rect,
                            scale: presentation_scale,
                            physical_x: presentation.physical_x,
                            physical_y: presentation.physical_y,
                            width: presentation.width,
                            height: presentation.height,
                            pixels,
                        });
                    }
                }
                self.page_op_destinations.insert(target);
            }
            if let Some((left, top, right, bottom)) = self
                .text_renderer
                .last_ink_bounds()
            {
                let _ = self
                    .sync_bound_alpha_from_color_rect(
                        target,
                        left,
                        top,
                        right.saturating_sub(left),
                        bottom.saturating_sub(top),
                    );
            }
        }
        if changed {
            if let Some(page) = self.pages.get(&target) {
                let mut transparent = 0usize;
                let mut opaque = 0usize;
                let mut partial = 0usize;
                let mut non_black = 0usize;
                for pixel in page.pixels.chunks_exact(4) {
                    match pixel[3] {
                        0 => transparent += 1,
                        255 => opaque += 1,
                        _ => partial += 1,
                    }
                    if pixel[0] != 0 || pixel[1] != 0 || pixel[2] != 0 {
                        non_black += 1;
                    }
                }
                eprintln!(
                    "[TEXT-DIAG] page={} alpha(clear={},partial={},opaque={}) non_black={}",
                    target, transparent, partial, opaque, non_black
                );
            }
        }
        eprintln!(
            "[TEXT] render page={} changed={} cursor={:?} ink_bounds={:?} console_rect={:?}",
            target, changed, self.text_renderer.cursor(), self.text_renderer
            .last_ink_bounds(), console_rect
        );
        (changed || action != TextRenderAction::Complete)
            .then(|| {
                let mut rect = console_rect.unwrap_or([0, 0, 0, 0]);
                if !changed {
                    rect[2] = 0;
                    rect[3] = 0;
                }
                TextRenderInfo {
                    page: target,
                    rect,
                    render_state: 0,
                    action,
                }
            })
    }
    pub fn take_render_frame(&mut self) -> Option<RenderFrame> {
        if self.scene_dirty_freeze != 0 {
            return None;
        }
        if self.movie.poll(&mut self.audio) {
            self.render_dirty = true;
        }
        let force_animation_update = self.render_dirty;
        self.update_sprite_animations(force_animation_update);
        if !self.render_dirty {
            return None;
        }
        self.update_smooth_animation_pages();
        let mut handles: Vec<_> = self.dirty_pages.drain().collect();
        handles.sort_unstable();
        for &handle in &handles {
            self.ensure_page_presentation(handle);
        }
        for &handle in &handles {
            if self.published_pages.contains(&handle)
                && !self.page_op_destinations.contains(&handle)
            {
                if let Some(page) = self.pages.get(&handle) {
                    match self.dirty_regions.get(&handle).copied().flatten() {
                        Some(rect) => {
                            self.pending_page_ops
                                .push(PageOp::UploadLogicalRect {
                                    destination: handle,
                                    page_width: page.width,
                                    page_height: page.height,
                                    rect,
                                    pixels: page.pixels.clone(),
                                })
                        }
                        None => {
                            self.pending_page_ops
                                .push(PageOp::UploadLogical {
                                    destination: handle,
                                    width: page.width,
                                    height: page.height,
                                    pixels: page.pixels.clone(),
                                })
                        }
                    }
                }
            }
        }
        let mut pages = handles
            .iter()
            .copied()
            .filter_map(|handle| {
                let page = self.pages.get(&handle)?;
                Some(RenderPage {
                    handle,
                    debug_name: self
                        .render_debug_names
                        .then(|| self.page_debug_names.get(&handle).cloned())
                        .flatten(),
                    revision: page.revision,
                    width: page.width,
                    height: page.height,
                    pixels: page.pixels.clone(),
                    presentation_seed: page.presentation_seed.clone(),
                    presentation: page.presentation.clone(),
                })
            })
            .collect::<Vec<_>>();
        pages
            .extend(
                std::mem::take(&mut self.released_page_snapshots)
                    .into_iter()
                    .map(|(handle, page)| RenderPage {
                        handle,
                        debug_name: self
                            .render_debug_names
                            .then(|| self.page_debug_names.get(&handle).cloned())
                            .flatten(),
                        revision: page.revision,
                        width: page.width,
                        height: page.height,
                        pixels: page.pixels,
                        presentation_seed: page.presentation_seed,
                        presentation: page.presentation,
                    }),
            );
        self.published_pages.extend(handles);
        self.dirty_regions.clear();
        let page_ops = std::mem::take(&mut self.pending_page_ops);
        if crate::diag_log_enabled() {
            for operation in &page_ops {
                let text = match operation {
                    crate::render_model::PageOp::Copy {
                        source,
                        destination,
                        mode,
                        source_is_presented_scene,
                        ..
                    } => {
                        format!(
                            "Copy src={source} dst={destination} mode={mode:?} presented_src={source_is_presented_scene}"
                        )
                    }
                    crate::render_model::PageOp::TransformCopy {
                        source,
                        destination,
                        mode,
                        ..
                    } => {
                        format!(
                            "TransformCopy src={source} dst={destination} mode={mode:?}"
                        )
                    }
                    crate::render_model::PageOp::Swap { source, destination, .. } => {
                        format!("Swap src={source} dst={destination}")
                    }
                    crate::render_model::PageOp::UploadLogical {
                        destination,
                        width,
                        height,
                        ..
                    } => format!("UploadLogical dst={destination} {width}x{height}"),
                    crate::render_model::PageOp::PresentationOverlay {
                        destination,
                        ..
                    } => format!("PresentationOverlay dst={destination}"),
                    _ => continue,
                };
                crate::diag::emit_log(format_args!("[OP] {text}"));
            }
        }
        self.page_op_destinations.clear();
        let mut quads = std::mem::take(&mut self.pending_quads);
        for handle in &self.sprite_order {
            if self.sprite_visibility_depths.get(handle).copied().unwrap_or(0) != 0 {
                let reason = "native_visibility_depth";
                if self.reported_render_skips.insert((*handle, reason)) {
                    eprintln!("[RENDER-SKIP] sprite={} reason={}", handle, reason);
                }
                continue;
            }
            let Some(sprite) = self.sprites.get(handle) else {
                let reason = "missing_sprite";
                if self.reported_render_skips.insert((*handle, reason)) {
                    eprintln!("[RENDER-SKIP] sprite={} reason={}", handle, reason);
                }
                continue;
            };
            if base_page_handle(sprite.target_page) != base_page_handle(self.frontbuffer)
            {
                let reason = "non_presented_target_page";
                if self.reported_render_skips.insert((*handle, reason)) {
                    eprintln!(
                        "[RENDER-SKIP] sprite={} reason={} target_page={} frontbuffer={}",
                        handle, reason, sprite.target_page, self.frontbuffer
                    );
                }
                continue;
            }
            if sprite.opacity == 0 {
                continue;
            }
            let Some((cx, cy)) = sprite.position else {
                let reason = "no_position";
                if crate::diag_log_enabled()
                    && self.reported_render_skips.insert((*handle, reason))
                {
                    eprintln!("[RENDER-SKIP] sprite={} reason={}", handle, reason);
                }
                continue;
            };
            let Some(page) = self.pages.get(&sprite.page) else {
                let reason = "missing_page";
                if self.reported_render_skips.insert((*handle, reason)) {
                    eprintln!("[RENDER-SKIP] sprite={} reason={}", handle, reason);
                }
                continue;
            };
            let Some((source_x, source_y, source_width, source_height)) = sprite_source_rect(
                sprite,
                page,
            ) else {
                let reason = "empty_source_rect";
                if self.reported_render_skips.insert((*handle, reason)) {
                    eprintln!("[RENDER-SKIP] sprite={} reason={}", handle, reason);
                }
                continue;
            };
            let rotation_degrees = self
                .sprite_rotations
                .get(handle)
                .map(|animation| animation.current)
                .unwrap_or(0.0);
            let scale_x = self
                .sprite_xmodifies
                .get(handle)
                .map(|animation| animation.current)
                .unwrap_or(1.0);
            let scale_y = self
                .sprite_ymodifies
                .get(handle)
                .map(|animation| animation.current)
                .unwrap_or(1.0);
            let transform_enabled = self.sprite_rotations.contains_key(handle)
                || self.sprite_xmodifies.contains_key(handle)
                || self.sprite_ymodifies.contains_key(handle);
            let (x, y) = if sprite.overlay || transform_enabled {
                (cx as f32 - source_width / 2.0, cy as f32 - source_height / 2.0)
            } else {
                (cx as f32, cy as f32)
            };
            let render_page = self
                .sprite_smooth_pages
                .get(handle)
                .copied()
                .unwrap_or(sprite.page);
            let rendered_page = self.pages.get(&render_page).unwrap_or(page);
            let uses_smooth_scratch = render_page != sprite.page;
            let quad = RenderQuad {
                page: render_page,
                x,
                y,
                width: source_width,
                height: source_height,
                source_x: if uses_smooth_scratch { 0.0 } else { source_x },
                source_y: if uses_smooth_scratch { 0.0 } else { source_y },
                source_width,
                source_height,
                alpha: f32::from(sprite.opacity) / 255.0,
                rotation_degrees,
                scale_x,
                scale_y,
                source_has_alpha: rendered_page.alpha_masked,
                draw_mode: rendered_page.draw_mode,
            };
            quads.push(quad);
            if crate::diag_log_enabled() {
                crate::diag::emit_log(
                    format_args!(
                        "[QUAD] sp={} sprite_page={} draw_page={} smooth={} pos=({:.0},{:.0}) size={}x{} alpha={} src_alpha={} draw_mode={} pres={} alpha_masked={} rot={:.1} scale=({:.2},{:.2})",
                        handle, sprite.page, render_page, uses_smooth_scratch, x, y,
                        source_width, source_height, sprite.opacity, rendered_page
                        .alpha_masked, rendered_page.draw_mode, rendered_page
                        .presentation.is_some(), rendered_page.alpha_masked,
                        rotation_degrees, scale_x, scale_y,
                    ),
                );
            }
        }
        let mut presented_handles = quads
            .iter()
            .map(|quad| quad.page)
            .collect::<HashSet<_>>();
        if let Some(display_page) = self.display_page {
            presented_handles.insert(display_page);
        }
        let presented_pages = presented_handles
            .into_iter()
            .filter_map(|handle| {
                self.pages
                    .get(&base_page_handle(handle))
                    .cloned()
                    .map(|page| (base_page_handle(handle), page))
            })
            .collect();
        self.report_composite_double_present(&quads);
        self.report_draw_list_census(&quads);
        self.presented_scene = Some(PresentedScene {
            display_page: self.display_page,
            pages: presented_pages,
            quads: quads.clone(),
        });
        let mut required_page_handles = quads
            .iter()
            .map(|quad| quad.page)
            .collect::<HashSet<_>>();
        required_page_handles.extend(self.display_page);
        for operation in &page_ops {
            match operation {
                PageOp::Copy { source, source_is_presented_scene, destination, .. }
                | PageOp::TransformCopy {
                    source,
                    source_is_presented_scene,
                    destination,
                    ..
                } => {
                    if !source_is_presented_scene {
                        required_page_handles.insert(*source);
                    }
                    required_page_handles.insert(*destination);
                }
                PageOp::Swap { source, destination, .. } => {
                    required_page_handles.insert(*source);
                    required_page_handles.insert(*destination);
                }
                PageOp::Points { destination, .. }
                | PageOp::Fill { destination, .. }
                | PageOp::Color { destination, .. }
                | PageOp::PresentationOverlay { destination, .. }
                | PageOp::UploadLogical { destination, .. }
                | PageOp::UploadLogicalRect { destination, .. } => {
                    required_page_handles.insert(*destination);
                }
            }
        }
        let mut packet_handles = pages
            .iter()
            .map(|page| page.handle)
            .collect::<HashSet<_>>();
        let mut rehydrate_pages = Vec::new();
        let mut required_page_handles = required_page_handles
            .into_iter()
            .collect::<Vec<_>>();
        required_page_handles.sort_unstable();
        for handle in required_page_handles {
            if !packet_handles.insert(handle) {
                continue;
            }
            self.ensure_page_presentation(handle);
            let Some(page) = self.pages.get(&handle) else {
                continue;
            };
            rehydrate_pages
                .push(RenderPage {
                    handle,
                    debug_name: self
                        .render_debug_names
                        .then(|| self.page_debug_names.get(&handle).cloned())
                        .flatten(),
                    revision: page.revision,
                    width: page.width,
                    height: page.height,
                    pixels: page.pixels.clone(),
                    presentation_seed: page.presentation_seed.clone(),
                    presentation: page.presentation.clone(),
                });
        }
        let mut retained_page_set = self.scene_resident_pages.clone();
        retained_page_set.insert(self.frontbuffer);
        retained_page_set.insert(self.base_page);
        retained_page_set.extend(self.display_page);
        retained_page_set
            .extend(
                self
                    .sprites
                    .values()
                    .flat_map(|sprite| {
                        [
                            base_page_handle(sprite.page),
                            base_page_handle(sprite.target_page),
                        ]
                    }),
            );
        retained_page_set.extend(self.sprite_smooth_pages.values().copied());
        let mut retained_pages = retained_page_set.iter().copied().collect::<Vec<_>>();
        retained_pages.sort_unstable();
        self.drop_inactive_page_presentations(&retained_page_set);
        self.render_dirty = false;
        self.present_epoch = self.present_epoch.wrapping_add(1);
        let released_pages = std::mem::take(&mut self.released_pages);
        for handle in &released_pages {
            self.published_pages.remove(handle);
            self.page_debug_names.remove(handle);
        }
        Some(RenderFrame {
            pages,
            rehydrate_pages,
            page_ops,
            released_pages,
            retained_pages,
            display_page: self.display_page,
            display_epoch: self.display_epoch,
            quads,
            movie: self.movie.current_frame(),
            viewport_offset: self.viewport_offset,
            marks: std::mem::take(&mut self.frame_marks),
        })
    }
    pub fn frame_probe_summary(&self) -> String {
        format!(
            "freeze={} dirty={} display={:?} epoch={} frontbuffer={} base={} pages={} sprites={} pending_quads={}",
            self.scene_dirty_freeze, self.render_dirty, self.display_page, self
            .display_epoch, self.frontbuffer, self.base_page, self.pages.len(), self
            .sprites.len(), self.pending_quads.len()
        )
    }
    fn page_for_read(&self, handle: u32) -> Option<Page> {
        let base = base_page_handle(handle);
        if !is_alpha_page(handle) && base == base_page_handle(self.frontbuffer) {
            return self
                .presented_scene
                .as_ref()
                .and_then(|scene| {
                    compose_presented_scene_snapshot(
                        scene,
                        self.internal_w,
                        self.internal_h,
                    )
                })
                .or_else(|| self.compose_frontbuffer_snapshot());
        }
        self.pages.get(&base).cloned()
    }
    fn reads_synthesized_frontbuffer(&self, handle: u32) -> bool {
        !is_alpha_page(handle)
            && base_page_handle(handle) == base_page_handle(self.frontbuffer)
    }
    fn note_composite_writeback(
        &mut self,
        op: &'static str,
        src_page: u32,
        dst_page: u32,
    ) {
        let src_base = base_page_handle(src_page);
        let dst_base = base_page_handle(dst_page);
        let direct = self.reads_synthesized_frontbuffer(src_page);
        let inherited = self.composite_derived_pages.contains_key(&src_base);
        if !direct && !inherited {
            return;
        }
        if direct {
            *self.composite_read_counts.entry(op).or_insert(0) += 1;
        }
        if dst_base == src_base {
            return;
        }
        self.composite_derived_pages.insert(dst_base, op);
        if self.reported_composite_writebacks.insert((op, src_base, dst_base)) {
            let quads = self
                .presented_scene
                .as_ref()
                .map(|scene| scene.quads.len())
                .unwrap_or(0);
            let origin = if direct {
                "synthesized frontbuffer"
            } else {
                "composite-derived page"
            };
            eprintln!(
                "[COMPOSITE-READ] op={op} {origin}={src_base} -> page={dst_base} \
                 (live quads in last presented scene={quads})"
            );
        }
    }
    fn clear_composite_provenance(&mut self, page: u32) {
        self.composite_derived_pages.remove(&base_page_handle(page));
    }
    fn report_composite_double_present(&mut self, quads: &[RenderQuad]) {
        if quads.is_empty() {
            return;
        }
        let candidates: Vec<u32> = self
            .display_page
            .into_iter()
            .chain(std::iter::once(base_page_handle(self.frontbuffer)))
            .map(base_page_handle)
            .filter(|page| self.composite_derived_pages.contains_key(page))
            .collect();
        for page in candidates {
            if !self.reported_composite_presents.insert(page) {
                continue;
            }
            let op = self.composite_derived_pages.get(&page).copied().unwrap_or("?");
            let sprite_pages: Vec<u32> = quads.iter().map(|quad| quad.page).collect();
            eprintln!(
                "[COMPOSITE-DOUBLE] page={page} was baked from a synthesized frontbuffer \
                 composite by {op} and is now presented with {} live quad(s) \
                 (quad pages={sprite_pages:?}). Sprites in both layers are \
                 composited twice.",
                quads.len()
            );
        }
    }
    fn report_draw_list_census(&mut self, quads: &[RenderQuad]) {
        if quads.len() <= self.peak_quads {
            return;
        }
        self.peak_quads = quads.len();
        let covers_screen = |quad: &RenderQuad| {
            quad.x <= 0.0 && quad.y <= 0.0 && quad.width >= self.internal_w as f32
                && quad.height >= self.internal_h as f32
        };
        let opaque_base = quads
            .iter()
            .rposition(|quad| {
                covers_screen(quad) && !quad.source_has_alpha && quad.alpha >= 1.0
                    && !matches!(quad.draw_mode, 1 | 3 | 4)
            });
        let visible = match opaque_base {
            Some(index) => &quads[index..],
            None => quads,
        };
        let mut per_page: HashMap<u32, usize> = HashMap::new();
        for quad in visible {
            *per_page.entry(quad.page).or_insert(0) += 1;
        }
        let repeated: Vec<(u32, usize)> = {
            let mut repeated: Vec<(u32, usize)> = per_page
                .iter()
                .filter(|(_, count)| **count > 1)
                .map(|(page, count)| (*page, *count))
                .collect();
            repeated.sort_unstable();
            repeated
        };
        let blended = visible
            .iter()
            .filter(|quad| quad.source_has_alpha || quad.alpha < 1.0)
            .count();
        let fullscreen = visible.iter().filter(|quad| covers_screen(quad)).count();
        eprintln!(
            "[CENSUS] quads={} above_opaque_base={} blended={} fullscreen={} \
             distinct_pages={} repeated_pages={:?} sprites_live={} \
             created={} released={} order={}",
            quads.len(), visible.len(), blended, fullscreen, per_page.len(), repeated,
            self.sprites.len(), self.sprites_created, self.sprites_released, self
            .sprite_order.len()
        );
    }
    pub fn composite_read_summary(&self) -> Vec<(&'static str, u64)> {
        let mut counts: Vec<(&'static str, u64)> = self
            .composite_read_counts
            .iter()
            .map(|(op, count)| (*op, *count))
            .collect();
        counts.sort_unstable();
        counts
    }
    pub fn has_active_animations(&self) -> bool {
        self
            .sprites
            .values()
            .any(|sprite| {
                sprite
                    .position_animation
                    .as_ref()
                    .is_some_and(|animation| animation.state == 1)
                    || sprite.alpha_animation.is_some()
                    || sprite
                        .frame_animation
                        .as_ref()
                        .is_some_and(|animation| animation.state != 0)
            }) || self.sprite_rotations.values().any(|animation| animation.state != 0)
            || self.sprite_xmodifies.values().any(|animation| animation.state != 0)
            || self.sprite_ymodifies.values().any(|animation| animation.state != 0)
    }
    fn update_sprite_animations(&mut self, force: bool) {
        let now_ms = self.clock_timestamp_ms;
        if !force
            && self
                .last_animation_update_ms
                .is_some_and(|last| now_ms.wrapping_sub(last).unsigned_abs() < 16)
        {
            return;
        }
        self.last_animation_update_ms = Some(now_ms);
        let mut active = false;
        let mut changed = false;
        for animation in self.sprite_rotations.values_mut() {
            let previous = animation.current;
            active |= advance_rotation_animation(animation, now_ms);
            changed |= previous != animation.current;
        }
        for animation in self.sprite_xmodifies.values_mut() {
            let previous = animation.current;
            active |= advance_scale_animation(animation, now_ms);
            changed |= previous != animation.current;
        }
        for animation in self.sprite_ymodifies.values_mut() {
            let previous = animation.current;
            active |= advance_scale_animation(animation, now_ms);
            changed |= previous != animation.current;
        }
        for sprite in self.sprites.values_mut() {
            if let Some(animation) = sprite.position_animation.as_mut() {
                let previous = sprite.position;
                active
                    |= advance_position_animation(
                        &mut sprite.position,
                        animation,
                        now_ms,
                    );
                changed |= previous != sprite.position;
            }
            if let Some(animation) = sprite.alpha_animation.as_mut() {
                let (transparency, finished) = advance_alpha_animation(
                    animation,
                    now_ms,
                );
                let opacity = 255 - transparency;
                if sprite.opacity != opacity {
                    sprite.opacity = opacity;
                    changed = true;
                }
                if finished {
                    sprite.alpha_animation = None;
                } else {
                    active = true;
                }
            }
            let (frame, frame_active) = match sprite.frame_animation.as_mut() {
                Some(animation) => advance_frame_animation(animation, now_ms),
                None => (None, false),
            };
            active |= frame_active;
            if let Some(frame) = frame {
                if let Some(page) = self.pages.get(&sprite.page) {
                    let source = source_for_animation_frame(sprite, page, frame);
                    let differs = sprite
                        .source
                        .is_none_or(|current| {
                            current.x != source.x || current.y != source.y
                                || current.width != source.width
                                || current.height != source.height
                        });
                    sprite.source = Some(source);
                    changed |= differs;
                }
            }
        }
        if changed || active {
            self.render_dirty = true;
        }
    }
    fn ensure_page_presentation(&mut self, handle: u32) {
        if self.pages.get(&handle).is_none_or(|page| page.presentation.is_some()) {
            return;
        }
        let Some(resource_name) = self.page_presentation_names.get(&handle).cloned()
        else {
            return;
        };
        let cached = self.page_source.cached_presentation(resource_name.as_ref());
        let presentation = if let Some(presentation) = cached {
            presentation
        } else {
            let Some(page) = self.pages.get(&handle) else {
                return;
            };
            let Some(patch) = self.patch.as_ref() else {
                return;
            };
            match patch.presentation_image(&resource_name, page.width, page.height) {
                Ok(Some(presentation)) => presentation,
                Ok(None) => return,
                Err(error) => {
                    eprintln!("[PATCH] {error}; using the native image");
                    return;
                }
            }
        };
        if let Some(page) = self.pages.get_mut(&handle) {
            page.presentation = Some(presentation);
        }
        if crate::diag_log_enabled() {
            let page = self.pages.get(&handle);
            crate::diag::emit_log(
                format_args!(
                    "[PRES] attach page={} name={} seed_ok={} pres_sz={}x{}", handle,
                    resource_name, page.is_some_and(| page | { page.presentation_seed
                    .len() == page.width as usize * page.height as usize * 4 }), page
                    .and_then(| page | page.presentation.as_ref().map(| image | image
                    .width)).unwrap_or(0), page.and_then(| page | page.presentation
                    .as_ref().map(| image | image.height)).unwrap_or(0),
                ),
            );
        }
        self.bake_page_presentation(handle);
    }
    fn bake_page_presentation(&mut self, handle: u32) {
        let Some(page) = self.pages.get_mut(&handle) else {
            return;
        };
        let Some(presentation) = page.presentation.as_ref() else {
            return;
        };
        let expected_len = page.width as usize * page.height as usize * 4;
        if page.width == 0 || page.height == 0
            || page.presentation_seed.len() != expected_len
        {
            return;
        }
        let baked = crate::render_model::PresentationImage::composited_over_seed(
            &page.presentation_seed,
            page.width,
            page.height,
            presentation,
        );
        page.presentation = Some(Arc::new(baked));
    }
    fn drop_inactive_page_presentations(&mut self, retained_pages: &HashSet<u32>) {
        for (&handle, page) in &mut self.pages {
            if !retained_pages.contains(&handle)
                && self.page_presentation_names.contains_key(&handle)
            {
                page.presentation = None;
            }
        }
        self.page_source.trim_template_cache();
    }
    fn cached_page_template(
        &mut self,
        filename: &[u8],
    ) -> Option<Arc<CachedPageTemplate>> {
        self.page_source
            .cached_template(&self.vfs, &self.rct_key, self.patch.as_deref(), filename)
    }
    fn instantiate_page_template(&mut self, template: &CachedPageTemplate) -> u32 {
        let handle = self.alloc_handle();
        let mut page = template.page.clone();
        page.revision = self.alloc_page_revision();
        let has_presentation = page.presentation.is_some();
        page.presentation = None;
        self.pages.insert(handle, page);
        if has_presentation {
            self.page_presentation_names
                .insert(handle, Arc::<str>::from(template.resolved_name.to_lowercase()));
        }
        if self.render_debug_names {
            self.page_debug_names
                .insert(handle, Arc::<str>::from(template.resolved_name.as_str()));
        }
        self.dirty_pages.insert(handle);
        self.render_dirty = true;
        handle
    }
    fn load_page_with_antidata(
        &mut self,
        filename: &[u8],
        attach_antidata: bool,
    ) -> u32 {
        let Some(template) = self.cached_page_template(filename) else {
            return 0;
        };
        let handle = self.instantiate_page_template(&template);
        if attach_antidata {
            let resolved_name = template.resolved_name.clone();
            match self.paired_antidata_name(&resolved_name) {
                Some(alpha_source) => {
                    let (alpha_source_sjis, _, _) = encoding_rs::SHIFT_JIS
                        .encode(&alpha_source);
                    let alpha_page = self
                        .load_page_with_antidata(&alpha_source_sjis, false);
                    let dimensions_match = self
                        .pages
                        .get(&alpha_page)
                        .is_some_and(|alpha| {
                            alpha.width == template.page.width
                                && alpha.height == template.page.height
                        });
                    if alpha_page != 0 && dimensions_match {
                        self.alpha_bindings.insert(handle, alpha_page);
                        self.owned_alpha_bindings.insert(handle);
                        let composite_key = resolved_name.to_lowercase();
                        if let Some(pixels) = self
                            .page_source
                            .alpha_composite_get(&composite_key)
                        {
                            if let Some(page) = self.pages.get_mut(&handle) {
                                page.pixels = pixels;
                                page.alpha_masked = true;
                            }
                        } else {
                            self.refresh_alpha_links(alpha_page);
                            if let Some(page) = self.pages.get(&handle) {
                                let pixels = Arc::clone(&page.pixels);
                                self.page_source
                                    .alpha_composite_insert(composite_key, pixels);
                            }
                        }
                        if let Some(page) = self.pages.get_mut(&handle) {
                            page.presentation_seed = page.pixels.clone();
                        }
                        vm::text_trace!(
                            "[PAGE] paired alpha {:?} <- {:?} handle={}", resolved_name,
                            alpha_source, handle
                        );
                    } else {
                        vm::text_trace!(
                            "[PAGE] alpha pair size mismatch {:?} for {:?} {}x{}",
                            alpha_source, resolved_name, template.page.width, template
                            .page.height
                        );
                        if alpha_page != 0 {
                            self.pages.remove(&alpha_page);
                            self.dirty_pages.remove(&alpha_page);
                            self.released_pages.push(alpha_page);
                        }
                    }
                }
                None => {
                    vm::text_trace!(
                        "[PAGE] no antidata pair for {:?} handle={}", resolved_name,
                        handle
                    );
                }
            }
        }
        eprintln!(
            "[GFX] instantiated page {:?} as handle {}", template.resolved_name, handle
        );
        handle
    }
    fn load_page(&mut self, filename: &[u8]) -> u32 {
        self.load_page_with_antidata(filename, true)
    }
    fn paired_antidata_name(&self, resolved_name: &str) -> Option<String> {
        let lower = resolved_name.to_ascii_lowercase();
        let stem = lower.strip_suffix(".rct")?;
        let candidate = format!("{stem}_.rc8");
        (self.page_source.contains_template(&candidate)
            || self.vfs.find(&candidate).is_some())
            .then_some(candidate)
    }
    fn resolve_script(&mut self, name: &str) -> Option<LoadedScript> {
        let loaded = self.resolve_script_inner(name);
        if loaded.is_some() {
            self.push_frame_mark("script_load", name.to_owned());
        }
        loaded
    }
    fn absorb_ir_translations(
        &mut self,
        name: &str,
        ir_start: Instant,
        loaded: LoadedScript,
        translations: script_source::IrTranslations,
    ) -> LoadedScript {
        let ir_elapsed = ir_start.elapsed();
        if vm::diag_log_enabled() && ir_elapsed.as_millis() >= 50 {
            std::eprintln!(
                "[TIMING] IR assemble {:?} took {:?} ({}B code)", name, ir_elapsed,
                loaded.code.len()
            );
        }
        if !translations.text.is_empty() {
            let script = canonical_script_name(name.as_bytes());
            let by_offset: std::collections::BTreeMap<u32, _> = translations
                .text
                .iter()
                .map(|(offset, m)| (*offset, m.clone()))
                .collect();
            self.ir_replay_messages
                .extend(translations.text.iter().map(|(_, m)| m.clone()));
            self.ir_text_overrides.insert(script, by_offset);
        }
        let display_count = translations.display.len();
        self.ir_display_messages.extend(translations.display);
        eprintln!(
            "[IR] resolved {:?} ({} text + {} display translations)", name, translations
            .text.len(), display_count
        );
        loaded
    }
    fn resolve_script_inner(&mut self, name: &str) -> Option<LoadedScript> {
        use script_source::{IrSource, MemoryIrSource, MjoSource, ScriptSource};
        if let Some(scripts) = &self.ir_scripts.clone() {
            let source = MemoryIrSource {
                files: std::sync::Arc::clone(scripts),
            };
            let ir_start = Instant::now();
            if let Some((loaded, translations)) = source.resolve_with_translations(name)
            {
                return Some(
                    self.finish_ir_resolve(name, ir_start, loaded, translations),
                );
            }
        }
        if let Some(root) = &self.ir_dir.clone() {
            let source = IrSource { root };
            let ir_start = Instant::now();
            if let Some((loaded, translations)) = source.resolve_with_translations(name)
            {
                return Some(
                    self.finish_ir_resolve(name, ir_start, loaded, translations),
                );
            }
        }
        MjoSource {
            vfs: &self.vfs,
            patch: self.patch.as_deref(),
        }
            .resolve(name)
    }
    fn finish_ir_resolve(
        &mut self,
        name: &str,
        ir_start: Instant,
        mut loaded: LoadedScript,
        translations: script_source::IrTranslations,
    ) -> LoadedScript {
        if let Some(patch) = self.patch.as_deref() {
            match patch.apply_script_transitions(&loaded.name, &mut loaded.code) {
                Ok(0) => {}
                Ok(count) => {
                    eprintln!(
                        "[PATCH] {}: replaced {} dot-matrix transition request(s) with fade",
                        loaded.name, count
                    )
                }
                Err(error) => {
                    eprintln!(
                        "[PATCH] {}: transition override ignored: {}", loaded.name, error
                    )
                }
            }
            loaded.code_crc32 = formats::crypto::crc32(&loaded.code);
        }
        if translations.identity_expected {
            use script_source::{MjoSource, ScriptSource};
            let archive = MjoSource {
                vfs: &self.vfs,
                patch: self.patch.as_deref(),
            }
                .resolve(name);
            match archive {
                Some(archive) if archive.code == loaded.code => {}
                Some(archive) => {
                    eprintln!(
                        "[IR] ERROR {:?}: v1 override assembles to different bytes than the \
                         archive script (v1 files may only carry @tr) — loading archive bytes, \
                         translations dropped",
                        name
                    );
                    return archive;
                }
                None => {
                    eprintln!(
                        "[IR] WARN {:?}: v1 override has no archive counterpart; identity \
                         cannot be verified",
                        name
                    );
                }
            }
        }
        if !translations.identity_expected
            && !self.patch.as_ref().is_some_and(|patch| patch.one_way_save_titles())
        {
            eprintln!(
                "[IR] WARN {:?}: IRv2-modified script under [save_compatibility] bidirectional =                  true — its save titles fall back to the UTF-8 mirror and are not vanilla-compatible;                  consider bidirectional = false",
                name
            );
        }
        self.absorb_ir_translations(name, ir_start, loaded, translations)
    }
    pub fn localize_ir(
        &self,
        site: &vm::host::TextSite,
        raw: &[u8],
    ) -> Option<Vec<crate::patch::LocalizedToken>> {
        use crate::patch::{localize_replay_messages, localize_tokens};
        if site.render_offset == usize::MAX {
            return localize_replay_messages(&self.ir_replay_messages, raw);
        }
        let script = canonical_script_name(&site.script_name);
        let message = self
            .ir_text_overrides
            .get(&script)?
            .get(&(site.render_offset as u32))?;
        localize_tokens(raw, message)
    }
    pub fn localize_ir_display(&self, raw: &[u8]) -> Option<String> {
        crate::patch::localize_display_message(&self.ir_display_messages, raw)
            .or_else(|| crate::patch::localize_display_message(
                &self.ir_replay_messages,
                raw,
            ))
            .or_else(|| {
                crate::patch::localize_trimmed_replay_message(
                    &self.ir_replay_messages,
                    raw,
                )
            })
    }
}
fn native_script_filename(name: &str) -> String {
    let leaf = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let stem = leaf.rsplit_once('.').map_or(leaf, |(stem, _)| stem);
    format!("{stem}.mjo")
}
mod pixel_ops;
use pixel_ops::{
    blend_indexed_page_region, blend_page_region_additive, blend_page_region_lighten,
    blend_page_region_multiply, blend_page_region_over, blit_decoded_page,
    clipped_copy_rects, clipped_rect, copy_antidata_region, copy_page_color_region,
    copy_page_region_with_alpha_view, copy_page_region_within, fill_antidata_rect,
    fill_page_alpha_rect, fill_page_rect, hq_scale_copy_page_region, mosaic_page_region,
    multiply_alpha_region, reprioritize_sprite, scale_copy_page_region,
    scale_copy_page_region_channels, transform_copy_page_region,
};

mod adapter;
mod clock;
mod file_host;
mod frame;
mod input;
mod pages;
mod runtime;
mod save_host;
mod sprites;
use sprites::compose_presented_scene_snapshot;
mod text_host;
mod text_files;
use text_files::{
    parse_line_token, parse_raw_line, read_next_line, resource_candidates, sjis_to_string,
};

