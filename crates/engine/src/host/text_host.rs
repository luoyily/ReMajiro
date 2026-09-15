use super::*;
impl EngineHost {
    pub(super) fn text_line_impl(&mut self, site: &vm::TextSite, bytes: &[u8]) {
        let localized = match crate::text::unicode_override_text(bytes) {
            Some(text) => {
                Some(vec![crate ::patch::LocalizedToken::Text(text.to_owned())])
            }
            None => {
                self.localize_ir(site, bytes)
                    .or_else(|| {
                        self.patch.as_ref().and_then(|patch| patch.localize(site, bytes))
                    })
            }
        };
        let line = match localized {
            Some(tokens) => self.text_renderer.consume_localized_line(&tokens),
            None => self.text_renderer.consume_line(bytes),
        };
        vm::text_trace!(
            "[HOST] TEXT_LINE layout={line:?} state={:?}", self.text_renderer
            .trace_state()
        );
        eprintln!("[TEXT] {}", line);
    }
    pub(super) fn text_render_impl(&mut self) -> Option<TextRenderInfo> {
        vm::text_trace!(
            "[HOST] TEXT_RENDER_BEGIN target={} state={:?}", self.text_renderer
            .target_page(), self.text_renderer.trace_state()
        );
        let result = self.render_text_page();
        vm::text_trace!(
            "[HOST] TEXT_RENDER_END result={result:?} state={:?}", self.text_renderer
            .trace_state()
        );
        result
    }
    pub(super) fn text_control_impl(&mut self, control: &vm::text::ControlCode) {
        vm::text_trace!(
            "[HOST] TEXT_CONTROL_BEGIN kind={:?} state={:?}", control.kind, self
            .text_renderer.trace_state()
        );
        self.text_renderer.consume_control(control);
        vm::text_trace!(
            "[HOST] TEXT_CONTROL_END kind={:?} state={:?}", control.kind, self
            .text_renderer.trace_state()
        );
    }
    pub(super) fn text_configure_impl(
        &mut self,
        page: u32,
        base_x: i32,
        base_y: i32,
        layout_a: i32,
        layout_b: i32,
    ) {
        self.text_renderer.configure(page, base_x, base_y, layout_a, layout_b);
        eprintln!(
            "[TEXT] configure page={} origin=({},{}) layout=({},{})", page, base_x,
            base_y, layout_a, layout_b
        );
    }
    pub(super) fn text_set_font_impl(
        &mut self,
        size: i32,
        width: i32,
        line_height: i32,
        flags: i32,
        face: &[u8],
    ) {
        self.text_renderer.set_font(size, width, line_height, flags, face);
        let (decoded, _, _) = encoding_rs::SHIFT_JIS
            .decode(face.split(|byte| *byte == 0).next().unwrap_or_default());
        eprintln!(
            "[TEXT] font size={} width={} line_height={} flags={} face={:?}", size,
            width, line_height, flags, decoded
        );
    }
    pub(super) fn text_set_colors_impl(&mut self, foreground: u32, background: i32) {
        self.text_renderer.set_colors(foreground, background);
        eprintln!(
            "[TEXT] colors foreground=0x{:08X} background=0x{:08X}", foreground,
            background as u32
        );
    }
    pub(super) fn text_set_position_impl(&mut self, x: i32, y: i32) {
        self.text_renderer.set_position(x, y);
        eprintln!("[TEXT] position ({},{})", x, y);
    }
    pub(super) fn text_font_size_impl(&mut self) -> i32 {
        self.text_renderer.font_size()
    }
    pub(super) fn text_line_height_impl(&mut self) -> i32 {
        self.text_renderer.line_height()
    }
    pub(super) fn get_text_pos_x_impl(&mut self) -> i32 {
        self.text_renderer.cursor().0
    }
    pub(super) fn get_text_pos_y_impl(&mut self) -> i32 {
        self.text_renderer.cursor().1
    }
    pub(super) fn font_locate_impl(
        &mut self,
        context_id: u32,
        page: u32,
        x: i32,
        y: i32,
    ) {
        self.text_renderer.font_locate(context_id, page, x, y);
    }
    pub(super) fn fontout_set_style_impl(
        &mut self,
        context_id: u32,
        face: &[u8],
        size: i32,
        width: i32,
        line_height: i32,
        flags: i32,
    ) {
        self.text_renderer
            .fontout_set_style(context_id, face, size, width, line_height, flags);
    }
    pub(super) fn fontout_set_colors_impl(
        &mut self,
        context_id: u32,
        foreground: u32,
        background: i32,
    ) {
        self.text_renderer.fontout_set_colors(context_id, foreground, background);
    }
    pub(super) fn font_render_impl(
        &mut self,
        _site: &vm::DisplayTextSite,
        context_id: u32,
        text: &[u8],
        width: Option<i32>,
        height: Option<i32>,
        alignment: Option<i32>,
    ) {
        let localized = crate::text::unicode_override_text(text)
            .map(str::to_owned)
            .or_else(|| self.localize_ir_display(text));
        let page_handle = self.text_renderer.fontout_page(context_id);
        let presentation_scale = self
            .patch
            .as_ref()
            .map(|patch| patch.presentation().scale)
            .unwrap_or(1);
        let destination_has_alpha = self
            .alpha_bindings
            .contains_key(&super::base_page_handle(page_handle))
            || self
                .pages
                .get(&super::base_page_handle(page_handle))
                .is_some_and(|page| page.alpha_masked);
        let (changed, presentation) = if let Some(page) = self
            .pages
            .get_mut(&page_handle)
        {
            let pixels = Arc::make_mut(&mut page.pixels);
            let layout = FontoutBox {
                width,
                height,
                alignment,
            };
            if let Some(localized) = localized.as_deref() {
                self.text_renderer
                    .render_fontout_unicode_scaled(
                        context_id,
                        localized,
                        layout,
                        page.width,
                        page.height,
                        pixels,
                        presentation_scale,
                    )
            } else {
                self.text_renderer
                    .render_fontout_scaled(
                        context_id,
                        text,
                        layout,
                        page.width,
                        page.height,
                        pixels,
                        presentation_scale,
                    )
            }
        } else {
            (false, None)
        };
        if changed {
            self.mark_page_dirty(page_handle);
            if let Some(presentation) = presentation {
                self.pending_page_ops
                    .push(crate::render_model::PageOp::PresentationOverlay {
                        destination: page_handle,
                        physical_x: presentation.physical_x,
                        physical_y: presentation.physical_y,
                        width: presentation.width,
                        height: presentation.height,
                        pixels: Arc::new(presentation.pixels),
                        destination_has_alpha,
                    });
                self.page_op_destinations.insert(page_handle);
            }
        }
        let rendered = localized
            .unwrap_or_else(|| encoding_rs::SHIFT_JIS.decode(text).0.into());
        eprintln!("[FONT] render page={} {:?}", page_handle, rendered);
    }
}
