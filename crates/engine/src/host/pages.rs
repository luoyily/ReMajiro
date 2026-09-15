#![allow(clippy::too_many_arguments)]
use super::*;
impl EngineHost {
    pub(super) fn page_set_draw_mode_impl(&mut self, page: u32, mode: u32) {
        let page = base_page_handle(page);
        let Some(target) = self.pages.get_mut(&page) else {
            return;
        };
        if target.indexed_samples.is_some() {
            return;
        }
        if target.draw_mode != mode {
            target.draw_mode = mode;
            self.render_dirty = true;
        }
    }
    pub(super) fn pic_unpack_impl(&mut self, filename: &[u8]) {
        let _ = self.cached_page_template(filename);
    }
    pub(super) fn pic_unpack_into_impl(
        &mut self,
        dst_page: u32,
        filename: &[u8],
        dst_x: i32,
        dst_y: i32,
    ) {
        if dst_page == 0 {
            let _ = self.cached_page_template(filename);
            return;
        }
        let name = sjis_to_string(filename);
        if name.is_empty() {
            return;
        }
        let h = self.load_page(filename);
        if h == 0 {
            return;
        }
        let image = match self.pages.get(&h) {
            Some(page) => page.clone(),
            None => return,
        };
        let destination = base_page_handle(dst_page);
        let before = self.page_revision(destination);
        let destination_has_alpha = self.page_has_alpha_semantics(destination);
        let changed = self
            .pages
            .get_mut(&destination)
            .is_some_and(|destination| blit_decoded_page(
                &image,
                destination,
                dst_x,
                dst_y,
            ));
        if changed {
            self.clear_composite_provenance(destination);
            self.mark_page_dirty_rect(
                destination,
                dst_x,
                dst_y,
                image.width as i32,
                image.height as i32,
            );
            if image.alpha_masked {
                let _ = self
                    .sync_bound_alpha_from_color_rect(
                        destination,
                        dst_x,
                        dst_y,
                        image.width as i32,
                        image.height as i32,
                    );
            } else {
                self.refresh_alpha_links_rect(
                    destination,
                    dst_x,
                    dst_y,
                    image.width as i32,
                    image.height as i32,
                );
            }
        }
        self.queue_page_op_if_changed(
            before,
            destination,
            crate::render_model::PageOp::Copy {
                source: h,
                source_is_presented_scene: false,
                destination,
                source_rect: [0, 0, image.width as i32, image.height as i32],
                destination_rect: [
                    dst_x,
                    dst_y,
                    image.width as i32,
                    image.height as i32,
                ],
                mode: if image.alpha_masked {
                    crate::render_model::PageCopyMode::Alpha
                } else {
                    crate::render_model::PageCopyMode::Replace
                },
                parameter: 0,
                source_has_alpha: image.alpha_masked,
                destination_has_alpha,
                source_alpha_view: false,
                destination_alpha_view: false,
            },
        );
        if destination == self.frontbuffer {
            self.display_page = Some(destination);
            self.display_epoch = self.display_epoch.wrapping_add(1);
            self.render_dirty = true;
        }
        eprintln!(
            "[GFX] pic_unpack_into page={} at=({},{}) → {:?} (loaded handle {}, mode={})",
            dst_page, dst_x, dst_y, name, h, if image.alpha_masked { "alpha-over" } else
            { "copy" }
        );
        self.release_unowned_page(h);
    }
    pub(super) fn page_create_file_impl(&mut self, filename: &[u8]) -> u32 {
        let page = self.load_page_with_antidata(filename, false);
        if page != 0 {
            page
        } else {
            self.page_create_impl(1, 1, native_missing_page_is_indexed(filename))
        }
    }
    pub(super) fn page_create_file_alpha_impl(&mut self, filename: &[u8]) -> u32 {
        let handle = self.load_page_with_antidata(filename, true);
        if handle != 0 && !self.alpha_bindings.contains_key(&handle) {
            let (width, height) = match self.pages.get(&handle) {
                Some(page) => (page.width as i32, page.height as i32),
                None => (1, 1),
            };
            let antidata = self.page_create_impl(width, height, true);
            self.alpha_bindings.insert(handle, antidata);
            self.owned_alpha_bindings.insert(handle);
            self.refresh_alpha_links(antidata);
            eprintln!(
                "[GFX] page_create_file_alpha fallback antidata page={} alpha_page={}",
                handle, antidata
            );
        }
        handle
    }
    pub(super) fn pic_get_width_impl(&mut self, filename: &[u8]) -> i32 {
        self.cached_page_template(filename)
            .map(|template| template.page.width as i32)
            .unwrap_or(1)
    }
    pub(super) fn pic_get_height_impl(&mut self, filename: &[u8]) -> i32 {
        self.cached_page_template(filename)
            .map(|template| template.page.height as i32)
            .unwrap_or(1)
    }
    pub(super) fn set_frontbuffer_impl(&mut self, page: u32) -> u32 {
        let old = self.frontbuffer;
        let page = base_page_handle(page);
        if !self.pages.contains_key(&page) {
            eprintln!(
                "[GFX] set_frontbuffer skipped: invalid page={} (old={})", page, old
            );
            return old;
        }
        self.frontbuffer = page;
        if let Some(image) = self.pages.get(&page) {
            if image.width == self.internal_w && image.height == self.internal_h {
                self.display_page = Some(page);
                self.display_epoch = self.display_epoch.wrapping_add(1);
                self.render_dirty = true;
            }
        }
        eprintln!("[GFX] set_frontbuffer page={} (old={})", page, old);
        old
    }
    pub(super) fn page_get_width_impl(&mut self, page: u32) -> i32 {
        self.pages.get(&base_page_handle(page)).map(|img| img.width as i32).unwrap_or(0)
    }
    pub(super) fn page_get_height_impl(&mut self, page: u32) -> i32 {
        self.pages.get(&base_page_handle(page)).map(|img| img.height as i32).unwrap_or(0)
    }
    pub(super) fn page_get_pixel_impl(&mut self, page: u32, x: i32, y: i32) -> i32 {
        let Some(image) = self.page_for_read(page) else {
            return 0;
        };
        if x < 0 || y < 0 || x >= image.width as i32 || y >= image.height as i32 {
            return -1;
        }
        let offset = (y as usize * image.width as usize + x as usize) * 4;
        let pixel = &image.pixels[offset..offset + 4];
        if is_alpha_page(page) {
            let anti = 255u8.wrapping_sub(pixel[3]);
            i32::from(anti) | (i32::from(anti) << 8) | (i32::from(anti) << 16)
        } else {
            i32::from(pixel[0]) | (i32::from(pixel[1]) << 8)
                | (i32::from(pixel[2]) << 16)
        }
    }
    pub(super) fn page_get_alpha_impl(&mut self, page_or_sprite: u32) -> u32 {
        let page = self
            .sprites
            .get(&page_or_sprite)
            .map_or(page_or_sprite, |sprite| sprite.page);
        let base = base_page_handle(page);
        self.alpha_bindings.get(&base).copied().unwrap_or(0)
    }
    pub(super) fn page_release_impl(&mut self, page: u32) {
        if is_alpha_page(page) {
            return;
        }
        let base = base_page_handle(page);
        let sprite_owned = self.sprites.values().any(|sprite| sprite.page == base);
        if sprite_owned {
            return;
        }
        if base != self.frontbuffer && base != self.base_page {
            self.release_unowned_page(base);
        }
    }
    pub(super) fn release_unowned_page(&mut self, base: u32) {
        self.scene_resident_pages.remove(&base);
        let antidata = self.alpha_bindings.remove(&base);
        let owns_antidata = self.owned_alpha_bindings.remove(&base);
        let page_is_queued_source = self
            .pending_page_ops
            .iter()
            .any(|operation| match operation {
                crate::render_model::PageOp::Copy {
                    source,
                    source_is_presented_scene,
                    ..
                }
                | crate::render_model::PageOp::TransformCopy {
                    source,
                    source_is_presented_scene,
                    ..
                } => !source_is_presented_scene && *source == base,
                crate::render_model::PageOp::Swap { source, destination, .. } => {
                    *source == base || *destination == base
                }
                _ => false,
            });
        if page_is_queued_source {
            self.ensure_page_presentation(base);
        }
        if let Some(page) = self.pages.remove(&base) {
            if page_is_queued_source {
                self.released_page_snapshots.insert(base, page);
            }
        }
        self.page_presentation_names.remove(&base);
        self.dirty_pages.remove(&base);
        self.dirty_regions.remove(&base);
        self.released_pages.push(base);
        let released_antidata = antidata.filter(|_| owns_antidata);
        if let Some(alpha) = released_antidata {
            self.scene_resident_pages.remove(&alpha);
            let alpha_is_queued_source = self
                .pending_page_ops
                .iter()
                .any(|operation| match operation {
                    crate::render_model::PageOp::Copy {
                        source,
                        source_is_presented_scene,
                        ..
                    }
                    | crate::render_model::PageOp::TransformCopy {
                        source,
                        source_is_presented_scene,
                        ..
                    } => !source_is_presented_scene && *source == alpha,
                    crate::render_model::PageOp::Swap { source, destination, .. } => {
                        *source == alpha || *destination == alpha
                    }
                    _ => false,
                });
            if alpha_is_queued_source {
                self.ensure_page_presentation(alpha);
            }
            if let Some(page) = self.pages.remove(&alpha) {
                if alpha_is_queued_source {
                    self.released_page_snapshots.insert(alpha, page);
                }
            }
            self.page_presentation_names.remove(&alpha);
            self.dirty_pages.remove(&alpha);
            self.dirty_regions.remove(&alpha);
            self.released_pages.push(alpha);
        }
        let detached_colors: Vec<u32> = self
            .alpha_bindings
            .iter()
            .filter_map(|(&color, &alpha)| {
                (alpha == base || Some(alpha) == released_antidata).then_some(color)
            })
            .collect();
        for color in detached_colors {
            self.alpha_bindings.remove(&color);
            self.owned_alpha_bindings.remove(&color);
            self.make_page_opaque(color);
        }
        self.render_dirty = true;
    }
    pub(super) fn page_create_impl(&mut self, w: i32, h: i32, indexed: bool) -> u32 {
        let w = w.max(1) as u32;
        let h = h.max(1) as u32;
        let pixel_count = (w as usize) * (h as usize);
        let mut pixels = vec![0u8; pixel_count * 4];
        for alpha in pixels[3..].iter_mut().step_by(4) {
            *alpha = 255;
        }
        let indexed_samples = indexed.then(|| vec![0u8; pixel_count]);
        let handle = self.alloc_handle();
        self.insert_page(handle, w, h, pixels, indexed_samples, false);
        eprintln!("[GFX] page_create {}x{} → handle {}", w, h, handle);
        handle
    }
    pub(super) fn page_create_with_antidata_impl(
        &mut self,
        w: i32,
        h: i32,
        indexed: bool,
    ) -> u32 {
        let color_page = self.page_create_impl(w, h, indexed);
        let antidata_page = self.page_create_impl(w, h, true);
        self.alpha_bindings.insert(color_page, antidata_page);
        self.owned_alpha_bindings.insert(color_page);
        self.refresh_alpha_links(antidata_page);
        eprintln!(
            "[GFX] page_create_with_antidata page={} alpha_page={}", color_page,
            antidata_page
        );
        color_page
    }
    pub(super) fn grp_boxfill_impl(
        &mut self,
        page: u32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        color: u32,
    ) {
        let base = base_page_handle(page);
        let explicit_antidata = self
            .alpha_bindings
            .values()
            .any(|alpha_page| *alpha_page == base);
        let Some(image) = self.pages.get_mut(&base) else {
            eprintln!("[GFX] grp_boxfill skipped: unknown page {}", page);
            return;
        };
        let changed = if is_alpha_page(page) {
            fill_page_alpha_rect(image, x, y, w, h, color as u8)
        } else if explicit_antidata {
            fill_antidata_rect(image, x, y, w, h, color as u8)
        } else {
            fill_page_rect(image, x, y, w, h, color)
        };
        if changed {
            self.mark_page_dirty_rect(base, x, y, w, h);
            self.refresh_alpha_links_rect(base, x, y, w, h);
        }
    }
    pub(super) fn grp_copy_impl(
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
        if base_page_handle(src_page) <= 34 || base_page_handle(dst_page) <= 34 {
            eprintln!(
                "[GFX-DIAG] grp_copy src={} ({},{}) {}x{} -> dst={} ({},{})", src_page,
                src_x, src_y, w, h, dst_page, dst_x, dst_y
            );
        }
        self.note_composite_writeback("grp_copy", src_page, dst_page);
        let src_base = base_page_handle(src_page);
        let dst_base = base_page_handle(dst_page);
        let dst_antidata = self.alpha_bindings.get(&dst_base).copied();
        if src_base == dst_base && !is_alpha_page(src_page) && !is_alpha_page(dst_page)
            && dst_antidata.is_none()
        {
            let changed = self
                .pages
                .get_mut(&dst_base)
                .is_some_and(|page| {
                    copy_page_region_within(page, src_x, src_y, w, h, dst_x, dst_y)
                });
            if changed {
                self.mark_page_dirty_rect(dst_base, dst_x, dst_y, w, h);
                self.refresh_alpha_links_rect(dst_base, dst_x, dst_y, w, h);
            }
            return;
        }
        let Some(src) = self.page_for_read(src_page) else {
            eprintln!("[GFX] grp_copy skipped: unknown source page {}", src_page);
            return;
        };
        let src_is_indexed = src.indexed_samples.is_some();
        let dst_is_indexed = self
            .pages
            .get(&dst_base)
            .is_some_and(|page| page.indexed_samples.is_some());
        if dst_is_indexed && !src_is_indexed {
            eprintln!(
                "[GFX] grp_copy rejected native 24-bit to 8-bit copy: src={} dst={}",
                src_page, dst_page
            );
            return;
        }
        let src_antidata = self.alpha_bindings.get(&src_base).copied();
        let Some(dst) = self.pages.get_mut(&dst_base) else {
            eprintln!("[GFX] grp_copy skipped: unknown destination page {}", dst_page);
            return;
        };
        let changed = if is_alpha_page(src_page) || is_alpha_page(dst_page) {
            copy_page_region_with_alpha_view(
                &src,
                dst,
                src_x,
                src_y,
                w,
                h,
                dst_x,
                dst_y,
                is_alpha_page(src_page),
                is_alpha_page(dst_page),
            )
        } else {
            copy_page_color_region(&src, dst, src_x, src_y, w, h, dst_x, dst_y)
        };
        if changed {
            self.mark_page_dirty_rect(dst_base, dst_x, dst_y, w, h);
            if !is_alpha_page(dst_page) {
                if let (Some(src_antidata), Some(dst_antidata)) = (
                    src_antidata,
                    dst_antidata,
                ) {
                    let source = self.pages.get(&src_antidata).cloned();
                    let antidata_changed = source
                        .is_some_and(|source| {
                            self.pages
                                .get_mut(&dst_antidata)
                                .is_some_and(|destination| {
                                    copy_antidata_region(
                                        &source,
                                        destination,
                                        src_x,
                                        src_y,
                                        w,
                                        h,
                                        dst_x,
                                        dst_y,
                                    )
                                })
                        });
                    if antidata_changed {
                        self.mark_page_dirty_rect(dst_antidata, dst_x, dst_y, w, h);
                        self.refresh_alpha_links_rect(dst_antidata, dst_x, dst_y, w, h);
                    }
                }
            }
            self.refresh_alpha_links_rect(dst_base, dst_x, dst_y, w, h);
        }
    }
    pub(super) fn grp_extcopy_impl(
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
        eprintln!(
            "[GFX-DIAG] grp_extcopy src=page{} ({},{}) {}x{} -> dst=page{} ({},{}) alpha={}",
            src_page, src_x, src_y, w, h, dst_page, dst_x, dst_y, alpha
        );
        self.note_composite_writeback("grp_extcopy", src_page, dst_page);
        let dst_base = base_page_handle(dst_page);
        let Some(src) = self.page_for_read(src_page) else {
            eprintln!("[GFX] grp_extcopy skipped: unknown source page {src_page}");
            return;
        };
        let src_base = base_page_handle(src_page);
        let src_is_indexed = src.indexed_samples.is_some();
        let src_has_antidata = self.alpha_bindings.contains_key(&src_base)
            || src.alpha_masked;
        let dst_is_indexed = self
            .pages
            .get(&dst_base)
            .is_some_and(|page| page.indexed_samples.is_some());
        if dst_is_indexed && (!src_is_indexed || src_has_antidata) {
            return;
        }
        let destination_has_alpha = self.alpha_bindings.contains_key(&dst_base)
            || self.pages.get(&dst_base).is_some_and(|page| page.alpha_masked);
        let Some(dst) = self.pages.get_mut(&dst_base) else {
            eprintln!("[GFX] grp_extcopy skipped: unknown destination page {dst_page}");
            return;
        };
        let transparency = alpha.clamp(0, 255) as u8;
        let changed = if dst_is_indexed {
            blend_indexed_page_region(
                &src,
                dst,
                src_x,
                src_y,
                w,
                h,
                dst_x,
                dst_y,
                transparency,
            )
        } else {
            let opacity = 255_u8.saturating_sub(transparency);
            blend_page_region_over(
                &src,
                dst,
                src_x,
                src_y,
                w,
                h,
                dst_x,
                dst_y,
                opacity,
                src.alpha_masked,
                destination_has_alpha,
            )
        };
        if changed {
            self.mark_page_dirty_rect(dst_base, dst_x, dst_y, w, h);
            if self.alpha_bindings.contains_key(&dst_base) {
                let _ = self
                    .sync_bound_alpha_from_color_rect(dst_base, dst_x, dst_y, w, h);
            }
        }
    }
    pub(super) fn grp_mulcopy_impl(
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
        self.note_composite_writeback("grp_mulcopy", src_page, dst_page);
        let dst_base = base_page_handle(dst_page);
        let Some(src) = self.page_for_read(src_page) else {
            return;
        };
        if src.indexed_samples.is_none()
            || self
                .pages
                .get(&dst_base)
                .is_none_or(|page| page.indexed_samples.is_none())
        {
            return;
        }
        let Some(dst) = self.pages.get_mut(&dst_base) else {
            return;
        };
        if multiply_alpha_region(
            &src,
            dst,
            src_x,
            src_y,
            w,
            h,
            dst_x,
            dst_y,
            false,
            false,
        ) {
            self.mark_page_dirty_rect(dst_base, dst_x, dst_y, w, h);
            self.refresh_alpha_links_rect(dst_base, dst_x, dst_y, w, h);
        }
    }
    pub(super) fn grp_point_set_impl(&mut self, page: u32, x: i32, y: i32, color: u32) {
        let base = base_page_handle(page);
        let explicit_antidata = self.alpha_bindings.values().any(|value| *value == base);
        let Some(image) = self.pages.get_mut(&base) else {
            return;
        };
        if x < 0 || y < 0 || x >= image.width as i32 || y >= image.height as i32 {
            return;
        }
        let offset = (y as usize * image.width as usize + x as usize) * 4;
        let pixels = Arc::make_mut(&mut image.pixels);
        if is_alpha_page(page) {
            pixels[offset + 3] = 255 - color as u8;
        } else if explicit_antidata || image.indexed_samples.is_some() {
            pixels[offset..offset + 3].fill(color as u8);
            pixels[offset + 3] = 255;
        } else {
            pixels[offset] = color as u8;
            pixels[offset + 1] = (color >> 8) as u8;
            pixels[offset + 2] = (color >> 16) as u8;
            pixels[offset + 3] = 255;
        }
        if !is_alpha_page(page) {
            if let Some(samples) = image.indexed_samples.as_mut() {
                Arc::make_mut(samples)[y as usize * image.width as usize + x as usize] = color
                    as u8;
            }
        }
        self.mark_page_dirty_rect(base, x, y, 1, 1);
        self.refresh_alpha_links_rect(base, x, y, 1, 1);
    }
    pub(super) fn grp_reverse_impl(
        &mut self,
        page: u32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
    ) {
        let base = base_page_handle(page);
        let Some(image) = self.pages.get_mut(&base) else {
            return;
        };
        if is_alpha_page(page) || image.indexed_samples.is_some() {
            return;
        }
        let Some((x0, y0, x1, y1)) = clipped_rect(image, x, y, w, h) else {
            return;
        };
        let stride = image.width as usize * 4;
        let pixels = Arc::make_mut(&mut image.pixels);
        for row in y0..y1 {
            for pixel in pixels[row * stride + x0 * 4..row * stride + x1 * 4]
                .chunks_exact_mut(4)
            {
                pixel[0] = 255 - pixel[0];
                pixel[1] = 255 - pixel[1];
                pixel[2] = 255 - pixel[2];
            }
        }
        self.mark_page_dirty(base);
    }
    pub(super) fn grp_mulboxfill_impl(
        &mut self,
        page: u32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        color: u32,
    ) {
        let base = base_page_handle(page);
        let Some(image) = self.pages.get_mut(&base) else {
            return;
        };
        if is_alpha_page(page) || image.indexed_samples.is_some() {
            return;
        }
        let Some((x0, y0, x1, y1)) = clipped_rect(image, x, y, w, h) else {
            return;
        };
        let factors = [color as u8, (color >> 8) as u8, (color >> 16) as u8];
        let stride = image.width as usize * 4;
        let pixels = Arc::make_mut(&mut image.pixels);
        for row in y0..y1 {
            for pixel in pixels[row * stride + x0 * 4..row * stride + x1 * 4]
                .chunks_exact_mut(4)
            {
                for channel in 0..3 {
                    pixel[channel] = ((u16::from(pixel[channel])
                        * (u16::from(factors[channel]) + 1)) >> 8) as u8;
                }
            }
        }
        self.mark_page_dirty(base);
    }
    pub(super) fn grp_alphablend_impl(
        &mut self,
        page: u32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        color: u32,
    ) {
        let base = base_page_handle(page);
        let Some(image) = self.pages.get_mut(&base) else {
            return;
        };
        if is_alpha_page(page) || image.indexed_samples.is_some() {
            return;
        }
        let Some((x0, y0, x1, y1)) = clipped_rect(image, x, y, w, h) else {
            return;
        };
        let blend = [color as u8, (color >> 8) as u8, (color >> 16) as u8];
        let stride = image.width as usize * 4;
        let pixels = Arc::make_mut(&mut image.pixels);
        for row in y0..y1 {
            for pixel in pixels[row * stride + x0 * 4..row * stride + x1 * 4]
                .chunks_exact_mut(4)
            {
                for channel in 0..3 {
                    let source = u32::from(pixel[channel]);
                    let factor = u32::from(blend[channel]);
                    let value = if source < 128 {
                        2 * factor * source / 255
                    } else {
                        2 * (factor + source - factor * source / 255) + 1
                    };
                    pixel[channel] = value as u8;
                }
            }
        }
        self.mark_page_dirty(base);
    }
    pub(super) fn grp_sepia_impl(
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
        let base = base_page_handle(page);
        let Some(image) = self.pages.get_mut(&base) else {
            return;
        };
        if is_alpha_page(page) {
            return;
        }
        let Some((x0, y0, x1, y1)) = clipped_rect(image, x, y, w, h) else {
            return;
        };
        let dark = [
            (dark_color & 0xFF) as i32,
            ((dark_color >> 8) & 0xFF) as i32,
            ((dark_color >> 16) & 0xFF) as i32,
        ];
        let light = [
            (light_color & 0xFF) as i32,
            ((light_color >> 8) & 0xFF) as i32,
            ((light_color >> 16) & 0xFF) as i32,
        ];
        let weights = mix
            .map(|value| (value.wrapping_add(1), 256i32.wrapping_sub(value)));
        if image.indexed_samples.is_some() {
            let Some(palette) = image.indexed_palette.as_mut() else {
                return;
            };
            for entry in Arc::make_mut(palette) {
                let original = *entry;
                let luma = i32::from(original[2])
                    + 2 * (i32::from(original[0]) + 2 * i32::from(original[1])) + 1;
                for channel in 0..3 {
                    let sepia = (dark[channel] * (2049 - luma) + light[channel] * luma)
                        / 2048;
                    entry[channel] = if let Some((old_weight, new_weight)) = weights {
                        ((old_weight * i32::from(original[channel]) + new_weight * sepia)
                            / 256) as u8
                    } else {
                        sepia as u8
                    };
                }
            }
            self.mark_indexed_page_dirty(base);
            return;
        }
        let stride = image.width as usize * 4;
        let pixels = Arc::make_mut(&mut image.pixels);
        for row in y0..y1 {
            for pixel in pixels[row * stride + x0 * 4..row * stride + x1 * 4]
                .chunks_exact_mut(4)
            {
                let luma = i32::from(pixel[2])
                    + 2 * (i32::from(pixel[0]) + 2 * i32::from(pixel[1])) + 1;
                for channel in 0..3 {
                    let sepia = (dark[channel] * (2049 - luma) + light[channel] * luma)
                        / 2048;
                    pixel[channel] = if let Some((old_weight, new_weight)) = weights {
                        ((old_weight * i32::from(pixel[channel]) + new_weight * sepia)
                            / 256) as u8
                    } else {
                        sepia as u8
                    };
                }
            }
        }
        self.mark_page_dirty(base);
    }
    pub(super) fn grp_extboxfill_impl(
        &mut self,
        page: u32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        color: u32,
        alpha: i32,
    ) {
        let base = base_page_handle(page);
        let explicit_antidata = self.alpha_bindings.values().any(|value| *value == base);
        let Some(image) = self.pages.get_mut(&base) else {
            return;
        };
        let Some((x0, y0, x1, y1)) = clipped_rect(image, x, y, w, h) else {
            return;
        };
        let old_weight = alpha.saturating_add(1).clamp(1, 256) as u16;
        let new_weight = 257 - old_weight;
        let channels = [color as u8, (color >> 8) as u8, (color >> 16) as u8];
        let width = image.width as usize;
        let stride = width * 4;
        let mut indexed_samples = image.indexed_samples.as_deref().cloned();
        let pixels = Arc::make_mut(&mut image.pixels);
        for row in y0..y1 {
            for (column, pixel) in pixels[row * stride + x0 * 4..row * stride + x1 * 4]
                .chunks_exact_mut(4)
                .enumerate()
            {
                if is_alpha_page(page) {
                    let old = 255 - pixel[3];
                    let value = (u16::from(color as u8) * new_weight
                        + u16::from(old) * old_weight) >> 8;
                    pixel[3] = 255 - value as u8;
                } else if let Some(samples) = indexed_samples.as_mut() {
                    let pixel_index = row * width + x0 + column;
                    let old = samples[pixel_index];
                    let value = (u16::from(color as u8) * new_weight
                        + u16::from(old) * old_weight) >> 8;
                    samples[pixel_index] = value as u8;
                    pixel[..3].fill(value as u8);
                } else if explicit_antidata {
                    let value = (u16::from(color as u8) * new_weight
                        + u16::from(pixel[0]) * old_weight) >> 8;
                    pixel[..3].fill(value as u8);
                } else {
                    for channel in 0..3 {
                        pixel[channel] = ((u16::from(channels[channel]) * new_weight
                            + u16::from(pixel[channel]) * old_weight) >> 8) as u8;
                    }
                }
            }
        }
        if let Some(samples) = indexed_samples {
            image.indexed_samples = Some(Arc::new(samples));
        }
        self.mark_page_dirty_rect(base, x, y, w, h);
        self.refresh_alpha_links_rect(base, x, y, w, h);
    }
    pub(super) fn grp_revmulcopy_impl(
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
        self.note_composite_writeback("grp_revmulcopy", src_page, dst_page);
        let dst_base = base_page_handle(dst_page);
        let Some(src) = self.page_for_read(src_page) else {
            return;
        };
        let Some(dst_snapshot) = self.pages.get(&dst_base) else {
            return;
        };
        if src.indexed_samples.is_none() || dst_snapshot.indexed_samples.is_none() {
            return;
        }
        let Some((sx, sy, dx, dy, width, height)) = clipped_copy_rects(
            &src,
            dst_snapshot,
            src_x,
            src_y,
            w,
            h,
            dst_x,
            dst_y,
        ) else {
            return;
        };
        let mut dst_samples = dst_snapshot.indexed_samples.as_deref().cloned().unwrap();
        let Some(dst) = self.pages.get_mut(&dst_base) else {
            return;
        };
        let dst_pixels = Arc::make_mut(&mut dst.pixels);
        for row in 0..height {
            for column in 0..width {
                let doff = ((dy + row) * dst.width as usize + dx + column) * 4;
                let source = native_8bit_sample(&src, sx + column, sy + row);
                let sample_offset = (dy + row) * dst.width as usize + dx + column;
                let target = dst_samples[sample_offset];
                let value = 255
                    - (((256 - u16::from(source)) * (255 - u16::from(target))) >> 8)
                        as u8;
                dst_samples[sample_offset] = value;
                dst_pixels[doff..doff + 3].fill(value);
                dst_pixels[doff + 3] = 255;
            }
        }
        dst.indexed_samples = Some(Arc::new(dst_samples));
        self.mark_page_dirty_rect(dst_base, dst_x, dst_y, w, h);
        self.refresh_alpha_links_rect(dst_base, dst_x, dst_y, w, h);
    }
    pub(super) fn grp_swap_impl(
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
        self.note_composite_writeback("grp_swap", src_page, dst_page);
        let src_base = base_page_handle(src_page);
        let dst_base = base_page_handle(dst_page);
        let Some(src_snapshot) = self.page_for_read(src_page) else {
            return;
        };
        let Some(dst_snapshot) = self.page_for_read(dst_page) else {
            return;
        };
        let indexed = src_snapshot.indexed_samples.is_some();
        if indexed != dst_snapshot.indexed_samples.is_some() {
            return;
        }
        let Some((sx, sy, dx, dy, width, height)) = clipped_copy_rects(
            &src_snapshot,
            &dst_snapshot,
            src_x,
            src_y,
            w,
            h,
            dst_x,
            dst_y,
        ) else {
            return;
        };
        let source_values = read_native_plane_region(
            &src_snapshot,
            sx,
            sy,
            width,
            height,
            indexed,
        );
        let target_values = read_native_plane_region(
            &dst_snapshot,
            dx,
            dy,
            width,
            height,
            indexed,
        );
        {
            let src = self.pages.get_mut(&src_base).unwrap();
            write_native_plane_region(
                src,
                sx,
                sy,
                width,
                height,
                indexed,
                &target_values,
            );
        }
        {
            let dst = self.pages.get_mut(&dst_base).unwrap();
            write_native_plane_region(
                dst,
                dx,
                dy,
                width,
                height,
                indexed,
                &source_values,
            );
        }
        self.mark_page_dirty_rect(
            src_base,
            sx as i32,
            sy as i32,
            width as i32,
            height as i32,
        );
        self.mark_page_dirty_rect(
            dst_base,
            dx as i32,
            dy as i32,
            width as i32,
            height as i32,
        );
        self.refresh_alpha_links_rect(
            src_base,
            sx as i32,
            sy as i32,
            width as i32,
            height as i32,
        );
        self.refresh_alpha_links_rect(
            dst_base,
            dx as i32,
            dy as i32,
            width as i32,
            height as i32,
        );
        let alpha_pair = self
            .alpha_bindings
            .get(&src_base)
            .copied()
            .zip(self.alpha_bindings.get(&dst_base).copied());
        if let Some((src_alpha, dst_alpha)) = alpha_pair {
            let Some(src_alpha_snapshot) = self.pages.get(&src_alpha).cloned() else {
                return;
            };
            let Some(dst_alpha_snapshot) = self.pages.get(&dst_alpha).cloned() else {
                return;
            };
            let source_alpha = read_native_plane_region(
                &src_alpha_snapshot,
                sx,
                sy,
                width,
                height,
                true,
            );
            let target_alpha = read_native_plane_region(
                &dst_alpha_snapshot,
                dx,
                dy,
                width,
                height,
                true,
            );
            write_native_plane_region(
                self.pages.get_mut(&src_alpha).unwrap(),
                sx,
                sy,
                width,
                height,
                true,
                &target_alpha,
            );
            write_native_plane_region(
                self.pages.get_mut(&dst_alpha).unwrap(),
                dx,
                dy,
                width,
                height,
                true,
                &source_alpha,
            );
            self.mark_page_dirty_rect(
                src_alpha,
                sx as i32,
                sy as i32,
                width as i32,
                height as i32,
            );
            self.mark_page_dirty_rect(
                dst_alpha,
                dx as i32,
                dy as i32,
                width as i32,
                height as i32,
            );
            self.refresh_alpha_links_rect(
                src_alpha,
                sx as i32,
                sy as i32,
                width as i32,
                height as i32,
            );
            self.refresh_alpha_links_rect(
                dst_alpha,
                dx as i32,
                dy as i32,
                width as i32,
                height as i32,
            );
        }
    }
    pub(super) fn page_set_antidata_impl(&mut self, page: u32, alpha_page: u32) {
        let page = base_page_handle(page);
        let alpha_page = base_page_handle(alpha_page);
        let Some((page_width, page_height)) = self
            .pages
            .get(&page)
            .map(|page| (page.width, page.height)) else {
            eprintln!("[GFX] page_set_antidata skipped: invalid page {}", page);
            return;
        };
        if alpha_page != 0 {
            let Some(alpha) = self.pages.get(&alpha_page) else {
                eprintln!(
                    "[GFX] page_set_antidata skipped: invalid alpha_page {}", alpha_page
                );
                return;
            };
            if alpha.indexed_samples.is_none() {
                eprintln!(
                    "[GFX] page_set_antidata skipped: alpha_page {} is not 8-bit indexed",
                    alpha_page
                );
                return;
            }
            if alpha.width != page_width || alpha.height != page_height {
                eprintln!(
                    "[GFX] page_set_antidata skipped: size mismatch page={} {}x{} alpha_page={} {}x{}",
                    page, page_width, page_height, alpha_page, alpha.width, alpha.height
                );
                return;
            }
        }
        let old_alpha = self.alpha_bindings.remove(&page);
        let old_was_owned = self.owned_alpha_bindings.remove(&page);
        if old_was_owned {
            if let Some(old_alpha) = old_alpha.filter(|old| *old != alpha_page) {
                self.pages.remove(&old_alpha);
                self.dirty_pages.remove(&old_alpha);
                self.released_pages.push(old_alpha);
            }
        }
        if alpha_page == 0 {
            self.make_page_opaque(page);
            eprintln!("[GFX] page_set_antidata page={} detached", page);
            return;
        }
        self.alpha_bindings.insert(page, alpha_page);
        self.refresh_alpha_links(alpha_page);
        eprintln!("[GFX] page_set_antidata page={} alpha_page={}", page, alpha_page);
    }
    pub(super) fn get_render_page_impl(&mut self) -> i32 {
        self.frontbuffer as i32
    }
    pub(super) fn grp_modify_copy_impl(
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
        self.note_composite_writeback("grp_modify_copy", src_page, dst_page);
        let source_handle = src_page;
        let source_base = base_page_handle(src_page);
        let dst_page = base_page_handle(dst_page);
        let Some(source) = self.page_for_read(source_handle) else {
            return;
        };
        let source_has_alpha = source.alpha_masked;
        let destination_has_alpha = self.alpha_bindings.contains_key(&dst_page)
            || self.pages.get(&dst_page).is_some_and(|page| page.alpha_masked);
        let angle_milli = (f64::from(angle_degrees) * 1000.0).trunc() as i32;
        let hq_scale = angle_milli % 360_000 == 0 && (scale_x != 1.0 || scale_y != 1.0)
            && scale_x > 0.0 && scale_y > 0.0;
        let Some(destination) = self.pages.get_mut(&dst_page) else {
            return;
        };
        let mut changed = if angle_milli % 360_000 == 0 && scale_x == 1.0
            && scale_y == 1.0
        {
            let centred_x = dst_x
                .wrapping_add(dst_w.wrapping_sub(src_w).wrapping_add(1) / 2);
            let centred_y = dst_y
                .wrapping_add(dst_h.wrapping_sub(src_h).wrapping_add(1) / 2);
            scale_copy_page_region_channels(
                &source,
                destination,
                src_x,
                src_y,
                src_w,
                src_h,
                centred_x,
                centred_y,
                src_w,
                src_h,
                source_has_alpha && destination_has_alpha,
                false,
            )
        } else if hq_scale {
            hq_scale_copy_page_region(
                &source,
                destination,
                src_x,
                src_y,
                src_w,
                src_h,
                dst_x,
                dst_y,
                dst_w,
                dst_h,
                scale_x,
                scale_y,
            )
        } else {
            transform_copy_page_region(
                &source,
                destination,
                src_x,
                src_y,
                src_w,
                src_h,
                dst_x,
                dst_y,
                dst_w,
                dst_h,
                angle_degrees,
                scale_x,
                scale_y,
                source_has_alpha,
                destination_has_alpha,
            )
        };
        let mut alpha_changed = false;
        let destination_alpha = self.alpha_bindings.get(&dst_page).copied();
        if hq_scale {
            let source_alpha = self.alpha_bindings.get(&source_base).copied();
            match (source_alpha, destination_alpha) {
                (Some(source_alpha), Some(destination_alpha)) => {
                    let alpha_source = self.pages.get(&source_alpha).cloned();
                    if let (Some(alpha_source), Some(alpha_destination)) = (
                        alpha_source,
                        self.pages.get_mut(&destination_alpha),
                    ) {
                        alpha_changed = hq_scale_copy_page_region(
                            &alpha_source,
                            alpha_destination,
                            src_x,
                            src_y,
                            src_w,
                            src_h,
                            dst_x,
                            dst_y,
                            dst_w,
                            dst_h,
                            scale_x,
                            scale_y,
                        );
                    }
                }
                (None, Some(destination_alpha)) => {
                    if let Some(alpha_destination) = self
                        .pages
                        .get_mut(&destination_alpha)
                    {
                        alpha_changed = fill_antidata_rect(
                            alpha_destination,
                            dst_x,
                            dst_y,
                            dst_w,
                            dst_h,
                            0,
                        );
                    }
                }
                _ => {}
            }
        }
        changed |= alpha_changed;
        if !changed {
            return;
        }
        self.mark_page_dirty(dst_page);
        if hq_scale {
            if let Some(destination_alpha) = destination_alpha {
                if alpha_changed {
                    self.mark_page_dirty(destination_alpha);
                }
                self.refresh_alpha_links_rect(
                    destination_alpha,
                    dst_x,
                    dst_y,
                    dst_w,
                    dst_h,
                );
            }
        } else if self.alpha_bindings.contains_key(&dst_page) {
            let _ = self
                .sync_bound_alpha_from_color_rect(dst_page, dst_x, dst_y, dst_w, dst_h);
        }
        self.refresh_alpha_links_rect(dst_page, dst_x, dst_y, dst_w, dst_h);
        if dst_page == base_page_handle(self.frontbuffer) {
            self.display_page = Some(dst_page);
            self.display_epoch = self.display_epoch.wrapping_add(1);
            self.render_dirty = true;
        }
    }
    pub(super) fn grp_make_mosaic_impl(
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
        mut rng_state: u32,
    ) -> u32 {
        self.note_composite_writeback("grp_make_mosaic", src_page, dst_page);
        let source_handle = src_page;
        let dst_page = base_page_handle(dst_page);
        let Some(source) = self.page_for_read(source_handle) else {
            return rng_state;
        };
        if source.indexed_samples.is_some()
            || self
                .pages
                .get(&dst_page)
                .is_some_and(|page| page.indexed_samples.is_some())
        {
            return rng_state;
        }
        let Some(destination) = self.pages.get_mut(&dst_page) else {
            return rng_state;
        };
        if mosaic_page_region(
            &source,
            destination,
            src_x,
            src_y,
            width,
            height,
            dst_x,
            dst_y,
            block_size,
            &mut rng_state,
        ) {
            self.mark_page_dirty(dst_page);
            if dst_page == base_page_handle(self.frontbuffer) {
                self.display_page = Some(dst_page);
                self.display_epoch = self.display_epoch.wrapping_add(1);
                self.render_dirty = true;
            }
        }
        rng_state
    }
    pub(super) fn grp_modcopy_impl(
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
        self.note_composite_writeback("grp_modcopy", src_page, dst_page);
        let source_handle = src_page;
        let src_page = base_page_handle(src_page);
        let dst_page = base_page_handle(dst_page);
        let Some(source) = self.page_for_read(source_handle) else {
            eprintln!("[GFX] grp_modcopy skipped: unknown source page {src_page}");
            return;
        };
        let source_has_alpha = source.alpha_masked;
        let destination_has_alpha = self.alpha_bindings.contains_key(&dst_page)
            || self.pages.get(&dst_page).is_some_and(|page| page.alpha_masked);
        let Some(destination) = self.pages.get_mut(&dst_page) else {
            eprintln!("[GFX] grp_modcopy skipped: unknown destination page {dst_page}");
            return;
        };
        if !scale_copy_page_region_channels(
            &source,
            destination,
            src_x,
            src_y,
            src_w,
            src_h,
            dst_x,
            dst_y,
            dst_w,
            dst_h,
            source_has_alpha && destination_has_alpha,
            destination_has_alpha,
        ) {
            return;
        }
        self.mark_page_dirty(dst_page);
        if self.alpha_bindings.contains_key(&dst_page) {
            let _ = self
                .sync_bound_alpha_from_color_rect(dst_page, dst_x, dst_y, dst_w, dst_h);
        }
        if dst_page == base_page_handle(self.frontbuffer) {
            self.display_page = Some(dst_page);
            self.display_epoch = self.display_epoch.wrapping_add(1);
            self.render_dirty = true;
        }
        eprintln!(
            "[GFX] grp_modcopy page{}({},{}) {}x{} → page{}({},{}) {}x{}", src_page,
            src_x, src_y, src_w, src_h, dst_page, dst_x, dst_y, dst_w, dst_h
        );
    }
    pub(super) fn make_alfa_table_impl(
        &mut self,
        source_page: u32,
        destination_page: u32,
        table: &[u8; 256],
    ) {
        self.note_composite_writeback("make_alfa_table", source_page, destination_page);
        let destination_base = base_page_handle(destination_page);
        let Some(source) = self.page_for_read(source_page) else {
            return;
        };
        let Some(source_samples) = source.indexed_samples.clone() else {
            return;
        };
        let source_width = source.width as usize;
        let width = self
            .pages
            .get(&destination_base)
            .map(|destination| source.width.min(destination.width) as usize)
            .unwrap_or(0);
        let height = self
            .pages
            .get(&destination_base)
            .map(|destination| source.height.min(destination.height) as usize)
            .unwrap_or(0);
        let Some(destination) = self.pages.get_mut(&destination_base) else {
            return;
        };
        if destination.indexed_samples.is_none() {
            return;
        }
        let mut mapped = destination
            .indexed_samples
            .as_ref()
            .map(|samples| samples.as_ref().clone())
            .unwrap_or_else(|| {
                vec![0; destination.width as usize * destination.height as usize]
            });
        let pixels = Arc::make_mut(&mut destination.pixels);
        for y in 0..height {
            for x in 0..width {
                let value = table[source_samples[y * source_width + x] as usize];
                let destination_index = y * destination.width as usize + x;
                mapped[destination_index] = value;
                let offset = destination_index * 4;
                pixels[offset..offset + 3].fill(value);
                pixels[offset + 3] = 255;
            }
        }
        destination.indexed_samples = Some(Arc::new(mapped));
        self.mark_page_dirty_rect(destination_base, 0, 0, width as i32, height as i32);
        self.refresh_alpha_links_rect(
            destination_base,
            0,
            0,
            width as i32,
            height as i32,
        );
        self.push_frame_mark(
            "make_alfa_table",
            format!("src=page{source_page} dst=page{destination_page} {width}x{height}"),
        );
    }
}
fn read_native_plane_region(
    page: &Page,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    indexed: bool,
) -> Vec<u8> {
    let channels = if indexed { 1 } else { 3 };
    let mut values = Vec::with_capacity(width * height * channels);
    for row in 0..height {
        for column in 0..width {
            if indexed {
                values.push(native_8bit_sample(page, x + column, y + row));
            } else {
                let offset = ((y + row) * page.width as usize + x + column) * 4;
                values.extend_from_slice(&page.pixels[offset..offset + 3]);
            }
        }
    }
    values
}
fn write_native_plane_region(
    page: &mut Page,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    indexed: bool,
    values: &[u8],
) {
    if indexed {
        let samples = Arc::make_mut(
            page
                .indexed_samples
                .as_mut()
                .expect("native indexed swap requires an 8-bit page"),
        );
        for row in 0..height {
            let source = row * width;
            let destination = (y + row) * page.width as usize + x;
            samples[destination..destination + width]
                .copy_from_slice(&values[source..source + width]);
        }
        return;
    }
    let pixels = Arc::make_mut(&mut page.pixels);
    for (index, value) in values.chunks_exact(3).enumerate() {
        let row = index / width;
        let column = index % width;
        let offset = ((y + row) * page.width as usize + x + column) * 4;
        pixels[offset..offset + 3].copy_from_slice(value);
    }
}
