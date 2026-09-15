use std::collections::HashMap;
use std::path::{Path, PathBuf};
use fontdue::{Font, FontSettings};
use vm::host::TextRenderAction;
use vm::text::{tokenize, ControlCode, ControlCodeKind, TextToken};
#[derive(Debug, Clone, Copy, Default)]
pub struct FontoutBox {
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub alignment: Option<i32>,
}
pub struct TextRenderResult {
    pub changed: bool,
    pub action: TextRenderAction,
    pub presentation: Option<PresentationText>,
    pub console_rect: Option<[i32; 4]>,
}
pub struct PresentationText {
    pub physical_x: i32,
    pub physical_y: i32,
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}
#[derive(Debug, Clone)]
struct FontStyle {
    size: i32,
    width: i32,
    line_height: i32,
    flags: i32,
    foreground: u32,
    background: i32,
    face: Vec<u8>,
}
impl Default for FontStyle {
    fn default() -> Self {
        Self {
            size: 20,
            width: 10,
            line_height: 25,
            flags: 0,
            foreground: 0x00FF_FFFF,
            background: -1,
            face: vec![
                0x82, 0x6C, 0x82, 0x72, 0x20, 0x83, 0x53, 0x83, 0x56, 0x83, 0x62, 0x83,
                0x4E,
            ],
        }
    }
}
impl FontStyle {
    fn set_dialog_font(
        &mut self,
        size: i32,
        width: i32,
        line_height: i32,
        flags: i32,
        face: &[u8],
    ) {
        if size != -1 && size > 0 {
            self.size = size;
        }
        self.width = if width == -1 {
            (self.size / 2).max(1)
        } else if width > 0 {
            width
        } else {
            self.width
        };
        if line_height != -1 {
            self.line_height = line_height;
        }
        if flags != -1 {
            self.flags = flags;
        }
        let face = c_bytes(face);
        if !face.is_empty() {
            self.face = face.to_vec();
        }
    }
    fn set_fontout_style(
        &mut self,
        size: i32,
        width: i32,
        line_height: i32,
        flags: i32,
        face: &[u8],
    ) {
        if size > 0 {
            self.size = size;
        }
        if width != -1 && width > 0 {
            self.width = width;
        }
        if line_height != -1 {
            self.line_height = line_height;
        }
        if flags != -1 {
            self.flags = flags;
        }
        let face = c_bytes(face);
        if !face.is_empty() {
            self.face = face.to_vec();
        }
    }
    fn horizontal_scale(&self) -> f32 {
        let native_default_width = (self.size.max(1) as f32 / 2.0).max(1.0);
        (self.width.max(1) as f32 / native_default_width).clamp(0.25, 4.0)
    }
    fn effective_line_height(&self) -> i32 {
        if self.line_height > 0 {
            self.line_height
        } else {
            ((self.size.max(1) as f32) * 1.25).ceil() as i32
        }
    }
    fn is_mingcho(&self) -> bool {
        let (face, _, _) = encoding_rs::SHIFT_JIS.decode(&self.face);
        face.contains("明朝")
    }
}
#[derive(Clone, Copy)]
struct PendingGlyph {
    ch: char,
    x: i32,
    y: i32,
    size: f32,
    horizontal_scale: f32,
    advance: i32,
    foreground: u32,
    background: i32,
    pseudo_bold: bool,
    page_index: u32,
}
#[derive(Clone)]
struct DialogSnapshot {
    target_page: u32,
    base_x: i32,
    base_y: i32,
    box_width: i32,
    box_height: i32,
    x: i32,
    y: i32,
    hanging_indent: i32,
    style: FontStyle,
}
struct DialogueState {
    target_page: u32,
    base_x: i32,
    base_y: i32,
    box_width: i32,
    box_height: i32,
    x: i32,
    y: i32,
    hanging_indent: i32,
    style: FontStyle,
    saved: Option<DialogSnapshot>,
    pending: Vec<PendingGlyph>,
    clear_before_render: bool,
    last_ink_bounds: Option<(i32, i32, i32, i32)>,
    visible_bounds: Option<(i32, i32, i32, i32)>,
    layout_page: u32,
    awaiting_page_clear: bool,
    final_cursor: Option<(i32, i32, i32)>,
}
impl Default for DialogueState {
    fn default() -> Self {
        Self {
            target_page: 0,
            base_x: 70,
            base_y: 400,
            box_width: 640,
            box_height: 480,
            x: 70,
            y: 400,
            hanging_indent: 0,
            style: FontStyle::default(),
            saved: None,
            pending: Vec::new(),
            clear_before_render: false,
            last_ink_bounds: None,
            visible_bounds: None,
            layout_page: 0,
            awaiting_page_clear: false,
            final_cursor: None,
        }
    }
}
impl DialogueState {
    fn snapshot(&self) -> DialogSnapshot {
        DialogSnapshot {
            target_page: self.target_page,
            base_x: self.base_x,
            base_y: self.base_y,
            box_width: self.box_width,
            box_height: self.box_height,
            x: self.x,
            y: self.y,
            hanging_indent: self.hanging_indent,
            style: self.style.clone(),
        }
    }
    fn restore(&mut self, snapshot: DialogSnapshot) {
        self.target_page = snapshot.target_page;
        self.base_x = snapshot.base_x;
        self.base_y = snapshot.base_y;
        self.box_width = snapshot.box_width;
        self.box_height = snapshot.box_height;
        self.x = snapshot.x;
        self.y = snapshot.y;
        self.hanging_indent = snapshot.hanging_indent;
        self.style = snapshot.style;
    }
    fn newline(&mut self) {
        self.x = self.base_x.saturating_add(self.hanging_indent.max(0));
        self.y = self.y.saturating_add(self.style.effective_line_height());
        if self.box_height > 0
            && self.y
                > self
                    .base_y
                    .saturating_add(self.box_height)
                    .saturating_sub(self.style.size.max(1))
        {
            self.layout_page = self.layout_page.saturating_add(1);
            self.hanging_indent = 0;
            self.x = self.base_x;
            self.y = self.base_y;
        }
    }
}
#[derive(Clone, Default)]
struct FontoutState {
    page: u32,
    x: i32,
    y: i32,
    style: FontStyle,
}
#[derive(Clone)]
struct RasterGlyph {
    width: usize,
    height: usize,
    xmin: i32,
    ymin: i32,
    mask: Vec<u8>,
}
pub type TextTraceState = (
    bool,
    Option<(i32, i32, i32, i32)>,
    Option<(i32, i32, i32, i32)>,
    usize,
    (i32, i32),
);
pub struct TextRenderer {
    fonts: Vec<Font>,
    font_source: Option<PathBuf>,
    dialogue: DialogueState,
    fontout: HashMap<u32, FontoutState>,
    font_shadow: bool,
    fuchidori: bool,
    font_bold: bool,
    font_anti: bool,
    glyph_cache: HashMap<GlyphRasterKey, std::sync::Arc<RasterGlyph>>,
}
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct GlyphRasterKey {
    font: usize,
    ch: char,
    size: u32,
    horizontal_scale: u32,
    raster_scale: u32,
    anti: bool,
    dilate: bool,
}
const GLYPH_CACHE_CAP: usize = 2048;
impl TextRenderer {
    pub fn disabled() -> Self {
        Self::from_fonts(Vec::new(), None)
    }
    pub fn load(requested: Option<&Path>) -> Result<Self, String> {
        Self::load_with_fallbacks(requested, &[])
    }
    pub fn load_with_fallbacks(
        requested: Option<&Path>,
        fallbacks: &[PathBuf],
    ) -> Result<Self, String> {
        let primary = requested
            .and_then(|path| resolve_font_path(Some(path)))
            .or_else(|| fallbacks.first().cloned())
            .or_else(|| resolve_font_path(None))
            .ok_or_else(|| {
                requested
                    .map_or_else(
                        || {
                            "no usable CJK system font found; pass --font <TTF/TTC>"
                                .to_string()
                        },
                        |path| format!("font file does not exist: {}", path.display()),
                    )
            })?;
        let mut paths = if requested.is_some() {
            vec![primary.clone()]
        } else {
            let mut paths = fallbacks.to_vec();
            if !paths.contains(&primary) {
                paths.push(primary.clone());
            }
            if let Some(system) = resolve_font_path(None) {
                if !paths.contains(&system) {
                    paths.push(system);
                }
            }
            paths
        };
        paths.dedup();
        let mut fonts = Vec::with_capacity(paths.len());
        for path in &paths {
            let bytes = std::fs::read(path)
                .map_err(|e| format!("failed to read font {}: {e}", path.display()))?;
            fonts
                .push(
                    Font::from_bytes(bytes, FontSettings::default())
                        .map_err(|e| {
                            format!("failed to parse font {}: {e}", path.display())
                        })?,
                );
        }
        Ok(Self::from_fonts(fonts, Some(primary)))
    }
    pub fn from_font_bytes(
        primary: Vec<u8>,
        fallbacks: Vec<Vec<u8>>,
    ) -> Result<Self, String> {
        let mut fonts = Vec::with_capacity(1 + fallbacks.len());
        fonts
            .push(
                Font::from_bytes(primary, FontSettings::default())
                    .map_err(|e| format!("failed to parse primary font: {e}"))?,
            );
        for (index, bytes) in fallbacks.into_iter().enumerate() {
            fonts
                .push(
                    Font::from_bytes(bytes, FontSettings::default())
                        .map_err(|e| {
                            format!("failed to parse fallback font {index}: {e}")
                        })?,
                );
        }
        Ok(Self::from_fonts(fonts, None))
    }
    fn from_fonts(fonts: Vec<Font>, font_source: Option<PathBuf>) -> Self {
        Self {
            fonts,
            font_source,
            dialogue: DialogueState::default(),
            fontout: HashMap::new(),
            font_shadow: crate::diag::env_knob_bool("OSTB_FONT_SHADOW", true),
            fuchidori: crate::diag::env_knob_bool("OSTB_FONT_OUTLINE", true),
            font_bold: crate::diag::env_knob_bool("OSTB_FONT_BOLD", false),
            font_anti: crate::diag::env_knob_bool("OSTB_FONT_ANTI", true),
            glyph_cache: HashMap::new(),
        }
    }
    pub fn font_source(&self) -> Option<&Path> {
        self.font_source.as_deref()
    }
    pub fn font_size(&self) -> i32 {
        self.dialogue.style.size
    }
    pub fn line_height(&self) -> i32 {
        self.dialogue.style.effective_line_height()
    }
    pub fn target_page(&self) -> u32 {
        self.dialogue.target_page
    }
    pub fn cursor(&self) -> (i32, i32) {
        (self.dialogue.x, self.dialogue.y)
    }
    pub fn last_ink_bounds(&self) -> Option<(i32, i32, i32, i32)> {
        self.dialogue.last_ink_bounds
    }
    pub fn trace_state(&self) -> TextTraceState {
        (
            self.dialogue.clear_before_render,
            self.dialogue.last_ink_bounds,
            self.dialogue.visible_bounds,
            self.dialogue.pending.len(),
            (self.dialogue.x, self.dialogue.y),
        )
    }
    pub fn configure(
        &mut self,
        page: u32,
        base_x: i32,
        base_y: i32,
        box_width: i32,
        box_height: i32,
    ) {
        self.dialogue.target_page = page;
        self.dialogue.base_x = base_x;
        self.dialogue.base_y = base_y;
        self.dialogue.box_width = box_width;
        self.dialogue.box_height = box_height;
        self.dialogue.x = base_x;
        self.dialogue.y = base_y;
        self.dialogue.hanging_indent = 0;
    }
    pub fn set_font(
        &mut self,
        size: i32,
        width: i32,
        line_height: i32,
        flags: i32,
        face: &[u8],
    ) {
        self.dialogue.style.set_dialog_font(size, width, line_height, flags, face);
    }
    pub fn set_colors(&mut self, foreground: u32, background: i32) {
        self.dialogue.style.foreground = foreground;
        self.dialogue.style.background = background;
    }
    pub fn set_position(&mut self, x: i32, y: i32) {
        self.dialogue.x = x;
        self.dialogue.y = y;
    }
    pub fn consume_line(&mut self, bytes: &[u8]) -> String {
        let tokens = tokenize(bytes);
        let mut readable = String::new();
        for token in tokens {
            match token {
                TextToken::Text(raw) => {
                    let raw = transform_inline_bytes(c_bytes(&raw));
                    let (decoded, _, _) = encoding_rs::SHIFT_JIS.decode(&raw.bytes);
                    readable.push_str(&decoded);
                    self.push_text(&decoded, raw.extended_brackets);
                }
                TextToken::Control(control) => self.consume_control(&control),
            }
        }
        readable
    }
    pub fn consume_localized_line(
        &mut self,
        tokens: &[crate::patch::LocalizedToken],
    ) -> String {
        let mut readable = String::new();
        for token in tokens {
            match token {
                crate::patch::LocalizedToken::Text(text) => {
                    readable.push_str(text);
                    self.push_text(text, false);
                }
                crate::patch::LocalizedToken::Control(control) => {
                    self.consume_control(control);
                }
            }
        }
        readable
    }
    fn push_text(&mut self, text: &str, extended_brackets: bool) {
        let mut chars = text.chars().peekable();
        if self.dialogue.hanging_indent != 0
            && self.dialogue.x
                == self.dialogue.base_x.saturating_add(self.dialogue.hanging_indent)
        {
            while matches!(chars.peek(), Some(' ' | '\u{3000}')) {
                chars.next();
            }
        }
        let mut line_start = self.dialogue.pending.len();
        for ch in chars {
            if ch == '\n' || ch == '\r' {
                self.dialogue.newline();
                line_start = self.dialogue.pending.len();
                continue;
            }
            let style = self.dialogue.style.clone();
            let size = style.size.max(1) as f32;
            let horizontal_scale = style.horizontal_scale();
            let advance = self.measure_advance(ch, &style);
            let right = self
                .dialogue
                .base_x
                .saturating_add(self.dialogue.box_width.max(0));
            let over_width = self.dialogue.box_width > 0
                && self.dialogue.x.saturating_add(advance) > right;
            if over_width && !is_line_head_prohibited(ch) {
                if self.dialogue.pending.len() > line_start {
                    let move_open = self
                        .dialogue
                        .pending
                        .last()
                        .is_some_and(|glyph| is_line_end_prohibited(glyph.ch));
                    if move_open {
                        let mut glyph = self.dialogue.pending.pop().expect("last glyph");
                        self.dialogue.newline();
                        line_start = self.dialogue.pending.len();
                        glyph.x = self.dialogue.x;
                        glyph.y = self.dialogue.y;
                        let moved_advance = self.measure_advance(glyph.ch, &style);
                        self.dialogue.x = self.dialogue.x.saturating_add(moved_advance);
                        self.dialogue.pending.push(glyph);
                    } else {
                        self.dialogue.newline();
                        line_start = self.dialogue.pending.len();
                    }
                } else {
                    self.dialogue.newline();
                    line_start = self.dialogue.pending.len();
                }
            }
            self.dialogue
                .pending
                .push(PendingGlyph {
                    ch,
                    x: self.dialogue.x,
                    y: self.dialogue.y,
                    size,
                    horizontal_scale,
                    advance,
                    foreground: style.foreground,
                    background: style.background,
                    pseudo_bold: style.is_mingcho(),
                    page_index: self.dialogue.layout_page,
                });
            self.dialogue.x = self.dialogue.x.saturating_add(advance.max(1));
            if ch == '「' || (extended_brackets && matches!(ch, '（' | '『')) {
                self.dialogue.hanging_indent = self
                    .dialogue
                    .x
                    .saturating_sub(self.dialogue.base_x)
                    .max(0);
            } else if ch == '」' || (extended_brackets && matches!(ch, '）' | '』')) {
                self.dialogue.hanging_indent = 0;
            }
        }
        if self.dialogue.layout_page != 0 {
            self.dialogue.final_cursor = Some((
                self.dialogue.x,
                self.dialogue.y,
                self.dialogue.hanging_indent,
            ));
        }
    }
    fn measure_advance(&self, ch: char, style: &FontStyle) -> i32 {
        self.font_for(ch)
            .map(|font| {
                (font.metrics(ch, style.size.max(1) as f32).advance_width
                    * style.horizontal_scale())
                    .ceil() as i32
            })
            .unwrap_or(style.size.max(1))
            .max(1)
    }
    fn font_for(&self, ch: char) -> Option<&Font> {
        self.fonts
            .iter()
            .find(|font| font.lookup_glyph_index(ch) != 0)
            .or_else(|| self.fonts.first())
    }
    fn font_index(&self, ch: char) -> Option<usize> {
        self.fonts
            .iter()
            .position(|font| font.lookup_glyph_index(ch) != 0)
            .or_else(|| (!self.fonts.is_empty()).then_some(0))
    }
    fn cached_raster(
        &mut self,
        font_index: usize,
        glyph: &PendingGlyph,
        raster_scale: f32,
    ) -> std::sync::Arc<RasterGlyph> {
        let dilate = self.font_bold || glyph.pseudo_bold;
        let key = GlyphRasterKey {
            font: font_index,
            ch: glyph.ch,
            size: glyph.size.to_bits(),
            horizontal_scale: glyph.horizontal_scale.to_bits(),
            raster_scale: raster_scale.to_bits(),
            anti: self.font_anti,
            dilate,
        };
        if let Some(raster) = self.glyph_cache.get(&key) {
            return std::sync::Arc::clone(raster);
        }
        let font = &self.fonts[font_index];
        let mut raster = if raster_scale == 1.0 {
            raster_glyph(font, *glyph, self.font_anti)
        } else {
            raster_glyph_scaled(font, *glyph, self.font_anti, raster_scale)
        };
        if dilate {
            dilate_mask(&mut raster.mask, raster.width, raster.height);
        }
        let raster = std::sync::Arc::new(raster);
        if self.glyph_cache.len() >= GLYPH_CACHE_CAP {
            self.glyph_cache.clear();
        }
        self.glyph_cache.insert(key, std::sync::Arc::clone(&raster));
        raster
    }
    pub fn consume_control(&mut self, control: &ControlCode) {
        match control.kind {
            ControlCodeKind::Newline | ControlCodeKind::NewlineRelative => {
                self.dialogue.newline()
            }
            ControlCodeKind::Wait => {
                self.dialogue.clear_before_render = false;
                self.dialogue.hanging_indent = 0;
                self.dialogue.x = self.dialogue.base_x;
                self.dialogue.y = self.dialogue.base_y;
                self.dialogue.saved = None;
            }
            ControlCodeKind::PageClear => {
                self.dialogue.clear_before_render = false;
            }
            ControlCodeKind::PagePause => {
                self.dialogue.clear_before_render = true;
            }
            ControlCodeKind::Color => {
                if let Some((foreground, background)) = control.colors() {
                    self.set_colors(foreground, background as i32);
                }
            }
            ControlCodeKind::Font => {
                if let Some((size, width, line_height, flags, face)) = control.font() {
                    self.set_font(
                        size.map(|v| v as i32).unwrap_or(-1),
                        width.map(|v| v as i32).unwrap_or(-1),
                        line_height.map(|v| v as i32).unwrap_or(-1),
                        flags.map(|v| v as i32).unwrap_or(-1),
                        face,
                    );
                }
            }
            ControlCodeKind::Position => {
                if let Some((x, y)) = control.position() {
                    self.set_position(x, y);
                }
            }
            ControlCodeKind::OffsetSave => {
                if let Some((x, y)) = control.position() {
                    self.dialogue.saved = Some(self.dialogue.snapshot());
                    self.dialogue.x = self.dialogue.x.saturating_add(x);
                    self.dialogue.y = self.dialogue.y.saturating_add(y);
                }
            }
            ControlCodeKind::Restore => {
                if let Some(snapshot) = self.dialogue.saved.take() {
                    self.dialogue.restore(snapshot);
                }
            }
            ControlCodeKind::Speed
            | ControlCodeKind::Delay
            | ControlCodeKind::Voice
            | ControlCodeKind::ExecScript
            | ControlCodeKind::Continue
            | ControlCodeKind::Graphics => {}
        }
    }
    pub fn render_pending(
        &mut self,
        width: u32,
        height: u32,
        pixels: &mut [u8],
    ) -> TextRenderResult {
        self.render_pending_scaled(width, height, pixels, 1)
    }
    pub fn render_pending_scaled(
        &mut self,
        width: u32,
        height: u32,
        pixels: &mut [u8],
        presentation_scale: u32,
    ) -> TextRenderResult {
        if self.dialogue.awaiting_page_clear {
            for glyph in &mut self.dialogue.pending {
                glyph.page_index = glyph.page_index.saturating_sub(1);
            }
            self.dialogue.layout_page = self.dialogue.layout_page.saturating_sub(1);
            self.dialogue.awaiting_page_clear = false;
            return TextRenderResult {
                changed: false,
                action: TextRenderAction::PageOverflow,
                presentation: None,
                console_rect: None,
            };
        }
        let all = std::mem::take(&mut self.dialogue.pending);
        let first_y = all
            .first()
            .filter(|glyph| glyph.page_index == 0)
            .map(|glyph| glyph.y);
        let line_end = first_y
            .and_then(|first_y| {
                all.iter().position(|glyph| glyph.page_index != 0 || glyph.y != first_y)
            })
            .unwrap_or(all.len());
        let mut remaining = all;
        let glyphs = remaining.drain(..line_end).collect::<Vec<_>>();
        self.dialogue.pending = remaining;
        let native_rect = native_console_rect(&glyphs);
        let presentation = (presentation_scale > 1)
            .then(|| {
                self
                    .render_presentation_glyphs(
                        &glyphs,
                        width,
                        height,
                        presentation_scale,
                    )
            })
            .flatten();
        let changed = self.render_glyphs(&glyphs, width, height, pixels);
        let console_rect = raster_console_rect(
                self.dialogue.last_ink_bounds,
                presentation.as_ref(),
                presentation_scale,
            )
            .or(native_rect);
        let action = if self.dialogue.pending.is_empty() {
            self.dialogue.layout_page = 0;
            if let Some((x, y, indent)) = self.dialogue.final_cursor.take() {
                self.dialogue.x = x;
                self.dialogue.y = y;
                self.dialogue.hanging_indent = indent;
            }
            TextRenderAction::Complete
        } else {
            self.dialogue.awaiting_page_clear = self
                .dialogue
                .pending
                .first()
                .is_some_and(|glyph| glyph.page_index != 0);
            TextRenderAction::Continue
        };
        TextRenderResult {
            changed,
            action,
            presentation,
            console_rect,
        }
    }
    fn render_presentation_glyphs(
        &mut self,
        glyphs: &[PendingGlyph],
        logical_width: u32,
        logical_height: u32,
        scale: u32,
    ) -> Option<PresentationText> {
        if self.fonts.is_empty() {
            return None;
        }
        let scale_f = scale as f32;
        let mut layers = Vec::new();
        let mut bounds = None;
        for glyph in glyphs {
            let font_index = self.font_index(glyph.ch)?;
            let raster = self.cached_raster(font_index, glyph, scale_f);
            let gx = (glyph.x as f32 * scale_f).round() as i32 + raster.xmin;
            let baseline = ((glyph.y as f32 + glyph.size.ceil()) * scale_f).round()
                as i32;
            let gy = baseline - raster.ymin - raster.height as i32;
            for (dx, dy, color) in effect_offsets(
                glyph.background,
                (glyph.size * scale_f).round() as i32,
                self.font_shadow,
                self.fuchidori,
            ) {
                let layer = (gx + dx, gy + dy, std::sync::Arc::clone(&raster), color);
                bounds = union_bounds(
                    bounds,
                    (
                        layer.0,
                        layer.1,
                        layer.0 + layer.2.width as i32,
                        layer.1 + layer.2.height as i32,
                    ),
                );
                layers.push(layer);
            }
            bounds = union_bounds(
                bounds,
                (gx, gy, gx + raster.width as i32, gy + raster.height as i32),
            );
            layers.push((gx, gy, raster, glyph.foreground));
        }
        let (left, top, right, bottom) = bounds?;
        let physical_width = logical_width.saturating_mul(scale) as i32;
        let physical_height = logical_height.saturating_mul(scale) as i32;
        let left = left.clamp(0, physical_width);
        let top = top.clamp(0, physical_height);
        let right = right.clamp(0, physical_width);
        let bottom = bottom.clamp(0, physical_height);
        if right <= left || bottom <= top {
            return None;
        }
        let width = (right - left) as u32;
        let height = (bottom - top) as u32;
        let mut pixels = vec![0; width as usize * height as usize * 4];
        for (x, y, raster, color) in layers {
            blend_mask(
                &mut pixels,
                width,
                height,
                x - left,
                y - top,
                raster.width,
                raster.height,
                &raster.mask,
                color,
            );
        }
        Some(PresentationText {
            physical_x: left,
            physical_y: top,
            width,
            height,
            pixels,
        })
    }
    fn render_glyphs(
        &mut self,
        glyphs: &[PendingGlyph],
        width: u32,
        height: u32,
        pixels: &mut [u8],
    ) -> bool {
        if self.fonts.is_empty() {
            self.record_render_bounds(None);
            return false;
        }
        let mut changed = false;
        if self.dialogue.clear_before_render {
            if let Some(bounds) = self.dialogue.visible_bounds.take() {
                changed |= clear_rect(pixels, width, height, bounds);
            }
            self.dialogue.clear_before_render = false;
        }
        let mut bounds = None;
        for glyph in glyphs {
            let Some(font_index) = self.font_index(glyph.ch) else {
                continue;
            };
            let raster = self.cached_raster(font_index, glyph, 1.0);
            let gx = glyph.x.saturating_add(raster.xmin);
            let baseline = glyph.y.saturating_add(glyph.size.ceil() as i32);
            let gy = baseline
                .saturating_sub(raster.ymin)
                .saturating_sub(raster.height as i32);
            let effect = effect_offsets(
                glyph.background,
                glyph.size as i32,
                self.font_shadow,
                self.fuchidori,
            );
            for (dx, dy, color) in effect {
                changed
                    |= blend_mask(
                        pixels,
                        width,
                        height,
                        gx.saturating_add(dx),
                        gy.saturating_add(dy),
                        raster.width,
                        raster.height,
                        &raster.mask,
                        color,
                    );
                bounds = union_bounds(
                    bounds,
                    (
                        gx + dx,
                        gy + dy,
                        gx + dx + raster.width as i32,
                        gy + dy + raster.height as i32,
                    ),
                );
            }
            changed
                |= blend_mask(
                    pixels,
                    width,
                    height,
                    gx,
                    gy,
                    raster.width,
                    raster.height,
                    &raster.mask,
                    glyph.foreground,
                );
            bounds = union_bounds(
                bounds,
                (gx, gy, gx + raster.width as i32, gy + raster.height as i32),
            );
        }
        self.record_render_bounds(bounds);
        changed
    }
    fn record_render_bounds(&mut self, bounds: Option<(i32, i32, i32, i32)>) {
        self.dialogue.last_ink_bounds = bounds;
        if let Some(bounds) = bounds {
            self.dialogue.visible_bounds = Some(
                union_bounds(self.dialogue.visible_bounds, bounds).unwrap_or(bounds),
            );
        }
    }
    fn fontout_state_mut(&mut self, context_id: u32) -> &mut FontoutState {
        self.fontout.entry(context_id).or_default()
    }
    pub fn font_locate(&mut self, context_id: u32, page: u32, x: i32, y: i32) {
        let node = self.fontout_state_mut(context_id);
        node.page = page;
        node.x = x;
        node.y = y;
    }
    pub fn fontout_set_style(
        &mut self,
        context_id: u32,
        face: &[u8],
        size: i32,
        width: i32,
        line_height: i32,
        flags: i32,
    ) {
        self.fontout_state_mut(context_id)
            .style
            .set_fontout_style(size, width, line_height, flags, face);
    }
    pub fn fontout_set_colors(
        &mut self,
        context_id: u32,
        foreground: u32,
        background: i32,
    ) {
        let style = &mut self.fontout_state_mut(context_id).style;
        style.foreground = foreground;
        style.background = background;
    }
    pub fn fontout_page(&mut self, context_id: u32) -> u32 {
        self.fontout_state_mut(context_id).page
    }
    #[allow(clippy::too_many_arguments)]
    pub fn render_fontout(
        &mut self,
        context_id: u32,
        text: &[u8],
        layout: FontoutBox,
        width: u32,
        height: u32,
        pixels: &mut [u8],
    ) -> bool {
        self.render_fontout_scaled(context_id, text, layout, width, height, pixels, 1).0
    }
    #[allow(clippy::too_many_arguments)]
    pub fn render_fontout_scaled(
        &mut self,
        context_id: u32,
        text: &[u8],
        layout: FontoutBox,
        width: u32,
        height: u32,
        pixels: &mut [u8],
        presentation_scale: u32,
    ) -> (bool, Option<PresentationText>) {
        let (decoded, _, _) = encoding_rs::SHIFT_JIS.decode(c_bytes(text));
        self.render_fontout_unicode_scaled(
            context_id,
            &decoded,
            layout,
            width,
            height,
            pixels,
            presentation_scale,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub fn render_fontout_unicode_scaled(
        &mut self,
        context_id: u32,
        text: &str,
        layout: FontoutBox,
        width: u32,
        height: u32,
        pixels: &mut [u8],
        presentation_scale: u32,
    ) -> (bool, Option<PresentationText>) {
        if self.fonts.is_empty() {
            return (false, None);
        }
        let state = self.fontout.get(&context_id).cloned().unwrap_or_default();
        let measured = text
            .chars()
            .map(|ch| self.measure_advance(ch, &state.style))
            .sum::<i32>();
        let mut x = state.x;
        let mut y = state.y;
        if let Some(box_width) = layout.width {
            x = x
                .saturating_add(
                    match layout.alignment.unwrap_or(0) {
                        -2 => box_width.saturating_sub(measured),
                        -1 => 0,
                        _ => box_width.saturating_sub(measured) / 2,
                    },
                );
        }
        if let Some(box_height) = layout.height {
            y = y.saturating_add(box_height.saturating_sub(state.style.size.max(1)) / 2);
        }
        let mut changed = false;
        let mut advance_total = 0;
        let mut glyphs = Vec::new();
        for ch in text.chars() {
            let Some(font_index) = self.font_index(ch) else {
                continue;
            };
            let advance = self.measure_advance(ch, &state.style);
            let pending = PendingGlyph {
                ch,
                x: x + advance_total,
                y,
                size: state.style.size.max(1) as f32,
                horizontal_scale: state.style.horizontal_scale(),
                advance,
                foreground: state.style.foreground,
                background: state.style.background,
                pseudo_bold: state.style.is_mingcho(),
                page_index: 0,
            };
            glyphs.push(pending);
            let raster = self.cached_raster(font_index, &pending, 1.0);
            let gx = pending.x + raster.xmin;
            let baseline = pending.y + pending.size.ceil() as i32;
            let gy = baseline - raster.ymin - raster.height as i32;
            for (dx, dy, color) in effect_offsets(
                pending.background,
                pending.size as i32,
                self.font_shadow,
                self.fuchidori,
            ) {
                changed
                    |= blend_mask(
                        pixels,
                        width,
                        height,
                        gx + dx,
                        gy + dy,
                        raster.width,
                        raster.height,
                        &raster.mask,
                        color,
                    );
            }
            changed
                |= blend_mask(
                    pixels,
                    width,
                    height,
                    gx,
                    gy,
                    raster.width,
                    raster.height,
                    &raster.mask,
                    pending.foreground,
                );
            advance_total += advance;
        }
        self.fontout_state_mut(context_id).x = state.x.saturating_add(advance_total);
        let presentation = (presentation_scale > 1)
            .then(|| {
                self
                    .render_presentation_glyphs(
                        &glyphs,
                        width,
                        height,
                        presentation_scale,
                    )
            })
            .flatten();
        (changed, presentation)
    }
}
fn raster_glyph(font: &Font, glyph: PendingGlyph, anti: bool) -> RasterGlyph {
    if !anti {
        let (metrics, mut mask) = font.rasterize(glyph.ch, glyph.size);
        threshold_mask(&mut mask);
        let (width, mask) = horizontal_resample(
            metrics.width,
            metrics.height,
            &mask,
            glyph.horizontal_scale,
        );
        return RasterGlyph {
            width,
            height: metrics.height,
            xmin: (metrics.xmin as f32 * glyph.horizontal_scale).floor() as i32,
            ymin: metrics.ymin,
            mask,
        };
    }
    let scale = 4usize;
    let (metrics, high) = font.rasterize(glyph.ch, glyph.size * scale as f32);
    if metrics.width == 0 || metrics.height == 0 {
        return RasterGlyph {
            width: 0,
            height: 0,
            xmin: 0,
            ymin: 0,
            mask: Vec::new(),
        };
    }
    let down_width = metrics.width.div_ceil(scale);
    let down_height = metrics.height.div_ceil(scale);
    let mut down = vec![0u8; down_width * down_height];
    for y in 0..down_height {
        for x in 0..down_width {
            let mut sum = 0u32;
            let mut weights = 0u32;
            for sy in 0..scale {
                for sx in 0..scale {
                    let hx = x * scale + sx;
                    let hy = y * scale + sy;
                    if hx >= metrics.width || hy >= metrics.height {
                        continue;
                    }
                    let weight = if (1..=2).contains(&sx) && (1..=2).contains(&sy) {
                        2
                    } else {
                        1
                    };
                    sum += u32::from(high[hy * metrics.width + hx]) * weight;
                    weights += weight;
                }
            }
            down[y * down_width + x] = (sum / weights.max(1)).min(255) as u8;
        }
    }
    let (width, mask) = horizontal_resample(
        down_width,
        down_height,
        &down,
        glyph.horizontal_scale,
    );
    RasterGlyph {
        width,
        height: down_height,
        xmin: ((metrics.xmin as f32 / scale as f32) * glyph.horizontal_scale).floor()
            as i32,
        ymin: (metrics.ymin as f32 / scale as f32).floor() as i32,
        mask,
    }
}
fn raster_glyph_scaled(
    font: &Font,
    glyph: PendingGlyph,
    anti: bool,
    scale: f32,
) -> RasterGlyph {
    let (metrics, mut mask) = font.rasterize(glyph.ch, glyph.size * scale);
    if !anti {
        threshold_mask(&mut mask);
    }
    let (width, mask) = horizontal_resample(
        metrics.width,
        metrics.height,
        &mask,
        glyph.horizontal_scale,
    );
    RasterGlyph {
        width,
        height: metrics.height,
        xmin: (metrics.xmin as f32 * glyph.horizontal_scale).floor() as i32,
        ymin: metrics.ymin,
        mask,
    }
}
fn horizontal_resample(
    width: usize,
    height: usize,
    mask: &[u8],
    scale: f32,
) -> (usize, Vec<u8>) {
    if width == 0 || height == 0 || (scale - 1.0).abs() < 0.001 {
        return (width, mask.to_vec());
    }
    let out_width = ((width as f32 * scale).ceil() as usize).max(1);
    let mut out = vec![0u8; out_width * height];
    for y in 0..height {
        for x in 0..out_width {
            let source_x = ((x as f32 / scale).floor() as usize).min(width - 1);
            out[y * out_width + x] = mask[y * width + source_x];
        }
    }
    (out_width, out)
}
fn threshold_mask(mask: &mut [u8]) {
    for sample in mask {
        *sample = if *sample >= 128 { 255 } else { 0 };
    }
}
fn dilate_mask(mask: &mut [u8], width: usize, height: usize) {
    if width < 2 || height < 2 {
        return;
    }
    let source = mask.to_vec();
    for y in 0..height - 1 {
        for x in 0..width - 1 {
            mask[y * width + x] = source[y * width + x]
                .max(source[y * width + x + 1])
                .max(source[(y + 1) * width + x])
                .max(source[(y + 1) * width + x + 1]);
        }
    }
}
fn effect_offsets(
    background: i32,
    size: i32,
    font_shadow: bool,
    fuchidori: bool,
) -> Vec<(i32, i32, u32)> {
    let raw = background as u32;
    let mode = ((raw >> 24) & 0x7F) as u8;
    let d1 = (size.max(1) / 9).max(1);
    let d2 = (size.max(1) / 18).max(1);
    let mut out = Vec::new();
    match mode {
        0 => out.push((d1, d2, raw)),
        1 => push_outline(&mut out, d2, raw),
        2 => {
            push_outline(&mut out, d2, raw);
            out.extend([(2 * d2, d2, raw), (2 * d2, 2 * d2, raw)]);
        }
        3 => {
            push_outline(&mut out, d2, raw);
            out.extend([(d2, 2 * d2, raw), (2 * d2, 2 * d2, raw)]);
        }
        4..=7 => {
            if font_shadow {
                let shadow = 0x0050_5050 | (raw & 0x00FF_FFFF);
                out.extend([(2 * d2, 2 * d2, shadow), (2 * d2, 2 * d2, shadow)]);
            }
            if fuchidori {
                push_outline(&mut out, d2, raw);
            }
        }
        _ => {}
    }
    out
}
fn native_console_rect(glyphs: &[PendingGlyph]) -> Option<[i32; 4]> {
    let first = glyphs.first()?;
    let measured_width = glyphs
        .iter()
        .fold(0i32, |width, glyph| width.saturating_add(glyph.advance));
    let font_height = first.size.ceil().max(1.0) as i32;
    let (extra_width, extra_height) = native_effect_extent(
        first.background,
        font_height,
    );
    Some([
        first.x,
        first.y,
        measured_width.saturating_add(extra_width),
        font_height.saturating_add(extra_height),
    ])
}
fn raster_console_rect(
    ink_bounds: Option<(i32, i32, i32, i32)>,
    presentation: Option<&PresentationText>,
    presentation_scale: u32,
) -> Option<[i32; 4]> {
    let mut bounds = ink_bounds;
    if let Some(presentation) = presentation {
        let scale = presentation_scale.max(1) as i32;
        let physical_right = presentation
            .physical_x
            .saturating_add(presentation.width.min(i32::MAX as u32) as i32);
        let physical_bottom = presentation
            .physical_y
            .saturating_add(presentation.height.min(i32::MAX as u32) as i32);
        let presentation_bounds = (
            presentation.physical_x.div_euclid(scale),
            presentation.physical_y.div_euclid(scale),
            div_ceil_nonnegative(physical_right, scale),
            div_ceil_nonnegative(physical_bottom, scale),
        );
        bounds = union_bounds(bounds, presentation_bounds);
    }
    bounds
        .and_then(|(left, top, right, bottom)| {
            (right > left && bottom > top)
                .then_some([
                    left,
                    top,
                    right.saturating_sub(left),
                    bottom.saturating_sub(top),
                ])
        })
}
fn div_ceil_nonnegative(value: i32, divisor: i32) -> i32 {
    value.saturating_add(divisor.saturating_sub(1)).div_euclid(divisor)
}
fn native_effect_extent(background: i32, size: i32) -> (i32, i32) {
    let mode = ((background as u32 >> 24) & 0x7F) as u8;
    let wide = (size.max(1) / 9).max(1);
    let narrow = (size.max(1) / 18).max(1);
    match mode {
        0 | 1 => (wide.saturating_add(1), narrow),
        2..=7 => (wide.saturating_mul(2).saturating_add(1), narrow.saturating_mul(2)),
        _ => (0, 0),
    }
}
fn push_outline(out: &mut Vec<(i32, i32, u32)>, distance: i32, color: u32) {
    out.extend([
        (-distance, -distance, color),
        (0, -distance, color),
        (distance, -distance, color),
        (-distance, 0, color),
        (distance, 0, color),
        (-distance, distance, color),
        (0, distance, color),
        (distance, distance, color),
    ]);
}
struct InlineBytes {
    bytes: Vec<u8>,
    extended_brackets: bool,
}
fn transform_inline_bytes(bytes: &[u8]) -> InlineBytes {
    const OPEN_QUOTE: &[u8] = &[0x81, 0x75];
    const CLOSE_QUOTE: &[u8] = &[0x81, 0x76];
    const OPEN_PAREN: &[u8] = &[0x81, 0x69];
    const CLOSE_PAREN: &[u8] = &[0x81, 0x6A];
    const OPEN_DOUBLE: &[u8] = &[0x81, 0x77];
    const CLOSE_DOUBLE: &[u8] = &[0x81, 0x78];
    let mut bytes = bytes.to_vec();
    let mut extended = false;
    for pattern in [
        [OPEN_QUOTE, OPEN_PAREN].concat(),
        [OPEN_QUOTE, OPEN_DOUBLE].concat(),
    ] {
        while let Some(index) = find_bytes(&bytes, &pattern) {
            bytes.drain(index..index + 2);
            extended = true;
        }
    }
    for pattern in [
        [CLOSE_PAREN, CLOSE_QUOTE].concat(),
        [CLOSE_DOUBLE, CLOSE_QUOTE].concat(),
    ] {
        while let Some(index) = find_bytes(&bytes, &pattern) {
            bytes.drain(index + 2..index + 4);
            extended = true;
        }
    }
    for pattern in [
        [OPEN_QUOTE, OPEN_QUOTE].concat(),
        [CLOSE_QUOTE, CLOSE_QUOTE].concat(),
    ] {
        while let Some(index) = find_bytes(&bytes, &pattern) {
            bytes.drain(index..index + 4);
        }
    }
    for byte in &mut bytes {
        if *byte == b'|' {
            *byte = b' ';
        }
    }
    let mut index = 0;
    while index + 1 < bytes.len() {
        if bytes[index] == 0x81 && bytes[index + 1] == 0xF1 {
            bytes[index + 1] = 0x40;
        }
        index += if is_sjis_lead(bytes[index]) { 2 } else { 1 };
    }
    InlineBytes {
        bytes,
        extended_brackets: extended,
    }
}
fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}
fn is_sjis_lead(byte: u8) -> bool {
    matches!(byte, 0x81..= 0x9F | 0xE0..= 0xFC)
}
fn is_line_end_prohibited(ch: char) -> bool {
    "「『（【'\"〔［｛〈《".contains(ch)
}
fn is_line_head_prohibited(ch: char) -> bool {
    "ー、。，．」』）？！ぁぃぅぇぉっゃゅょゎァィゥェォャュョヮヵヶ゛゜ゝゞヽヾ〟・…：；‐】〕］｝〉"
        .contains(ch)
}
fn resolve_font_path(requested: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = requested {
        return path.is_file().then(|| path.to_path_buf());
    }
    SYSTEM_FONT_CANDIDATES.iter().copied().map(PathBuf::from).find(|path| path.is_file())
}
const SYSTEM_FONT_CANDIDATES: &[&str] = &[
    r"C:\Windows\Fonts\msyh.ttc",
    r"C:\Windows\Fonts\msgothic.ttc",
    r"C:\Windows\Fonts\meiryo.ttc",
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
    "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
];
fn c_bytes(bytes: &[u8]) -> &[u8] {
    bytes.split(|byte| *byte == 0).next().unwrap_or(bytes)
}
pub fn unicode_override_text(bytes: &[u8]) -> Option<&str> {
    let bytes = c_bytes(bytes);
    let text = std::str::from_utf8(bytes).ok()?;
    (!text.is_ascii()).then_some(text)
}
fn union_bounds(
    a: Option<(i32, i32, i32, i32)>,
    b: (i32, i32, i32, i32),
) -> Option<(i32, i32, i32, i32)> {
    Some(
        match a {
            Some(a) => (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)),
            None => b,
        },
    )
}
fn clear_rect(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    bounds: (i32, i32, i32, i32),
) -> bool {
    let x0 = bounds.0.max(0).min(width as i32);
    let y0 = bounds.1.max(0).min(height as i32);
    let x1 = bounds.2.max(0).min(width as i32);
    let y1 = bounds.3.max(0).min(height as i32);
    if x0 >= x1 || y0 >= y1 {
        return false;
    }
    for y in y0..y1 {
        let start = ((y as u32 * width + x0 as u32) * 4) as usize;
        let end = ((y as u32 * width + x1 as u32) * 4) as usize;
        pixels[start..end].fill(0);
    }
    true
}
#[allow(clippy::too_many_arguments)]
fn blend_mask(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    glyph_width: usize,
    glyph_height: usize,
    mask: &[u8],
    color: u32,
) -> bool {
    if glyph_width == 0 || glyph_height == 0 {
        return false;
    }
    let rgb = [
        (color & 0xFF) as u8,
        ((color >> 8) & 0xFF) as u8,
        ((color >> 16) & 0xFF) as u8,
    ];
    let mut changed = false;
    for row in 0..glyph_height {
        let py = y + row as i32;
        if !(0..height as i32).contains(&py) {
            continue;
        }
        for col in 0..glyph_width {
            let px = x + col as i32;
            if !(0..width as i32).contains(&px) {
                continue;
            }
            let coverage = mask[row * glyph_width + col];
            if coverage == 0 {
                continue;
            }
            let dst = ((py as u32 * width + px as u32) * 4) as usize;
            let old_alpha = pixels[dst + 3] as u16;
            let src_alpha = coverage as u16;
            let out_alpha = src_alpha + old_alpha * (255 - src_alpha) / 255;
            for channel in 0..3 {
                let old = pixels[dst + channel] as u16;
                pixels[dst + channel] = ((u16::from(rgb[channel]) * src_alpha
                    + old * (255 - src_alpha)) / 255) as u8;
            }
            pixels[dst + 3] = out_alpha.min(255) as u8;
            changed = true;
        }
    }
    changed
}


