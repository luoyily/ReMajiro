use super::*;
pub(super) struct SpritePasteReplay {
    pub source: u32,
    pub source_is_presented_scene: bool,
    pub source_rect: [i32; 4],
    pub destination_rect: [i32; 4],
    pub opacity: u8,
    pub draw_mode: u32,
    pub source_has_alpha: bool,
    pub destination_has_alpha: bool,
    pub transform: Option<(f32, f32, f32)>,
}
impl EngineHost {
    pub(super) fn sprite_create_file_impl(&mut self, filename: &[u8]) -> u32 {
        let loaded = self.load_page(filename);
        let page = if loaded == 0 {
            self.page_create_impl(1, 1, native_missing_page_is_indexed(filename))
        } else {
            loaded
        };
        if !self.pages.contains_key(&page) {
            return 0;
        }
        let handle = self.alloc_handle();
        self.sprites_created += 1;
        self.sprites
            .insert(
                handle,
                Sprite {
                    page,
                    target_page: self.frontbuffer,
                    group_id: handle,
                    opacity: 255,
                    source: None,
                    original_source_position: (0, 0),
                    overlay: false,
                    position: None,
                    position_animation: None,
                    alpha_animation: None,
                    frame_animation: None,
                },
            );
        self.sprite_order.push(handle);
        eprintln!("[GFX] sprite_create_file → handle {}", handle);
        handle
    }
    pub(super) fn sprite_create_raw_impl(
        &mut self,
        count: usize,
        args: &[vm::value::Value],
    ) -> u32 {
        let page_handle = args
            .get(count.saturating_sub(1))
            .and_then(|v| v.as_int())
            .unwrap_or(0) as u32;
        let source = (count >= 5)
            .then(|| SpriteSource {
                x: args.get(count - 2).and_then(vm::value::Value::as_int).unwrap_or(0),
                y: args.get(count - 3).and_then(vm::value::Value::as_int).unwrap_or(0),
                width: args
                    .get(count - 4)
                    .and_then(vm::value::Value::as_int)
                    .unwrap_or(0),
                height: args
                    .get(count - 5)
                    .and_then(vm::value::Value::as_int)
                    .unwrap_or(0),
            });
        if !self.pages.contains_key(&page_handle) {
            eprintln!(
                "[GFX] sprite_create_raw page={} (no image) → handle 0", page_handle
            );
            return 0;
        }
        let handle = self.alloc_handle();
        let target_page = if count >= 6 {
            args
                .first()
                .and_then(vm::value::Value::as_int)
                .unwrap_or(self.frontbuffer as i32) as u32
        } else {
            self.frontbuffer
        };
        self.sprites_created += 1;
        self.sprites
            .insert(
                handle,
                Sprite {
                    page: page_handle,
                    target_page,
                    group_id: handle,
                    opacity: 255,
                    source,
                    original_source_position: source
                        .map(|r| (r.x, r.y))
                        .unwrap_or((0, 0)),
                    overlay: false,
                    position: None,
                    position_animation: None,
                    alpha_animation: None,
                    frame_animation: None,
                },
            );
        self.sprite_order.push(handle);
        eprintln!(
            "[GFX] sprite_create_raw page={} source={:?} → sprite handle {}",
            page_handle, source.map(| r | (r.x, r.y, r.width, r.height)), handle
        );
        handle
    }
    pub(super) fn sprite_create_file_raw_impl(
        &mut self,
        count: usize,
        args: &[vm::value::Value],
    ) -> u32 {
        let filename = args
            .get(count.saturating_sub(1))
            .and_then(vm::value::Value::as_str_bytes)
            .unwrap_or(&[]);
        let loaded = self.load_page(filename);
        let page = if loaded == 0 {
            self.page_create_impl(1, 1, native_missing_page_is_indexed(filename))
        } else {
            loaded
        };
        let source = (count >= 5)
            .then(|| SpriteSource {
                x: args.get(count - 2).and_then(vm::value::Value::as_int).unwrap_or(0),
                y: args.get(count - 3).and_then(vm::value::Value::as_int).unwrap_or(0),
                width: args
                    .get(count - 4)
                    .and_then(vm::value::Value::as_int)
                    .unwrap_or(0),
                height: args
                    .get(count - 5)
                    .and_then(vm::value::Value::as_int)
                    .unwrap_or(0),
            });
        let handle = self.alloc_handle();
        let target_page = if count >= 6 {
            args
                .first()
                .and_then(vm::value::Value::as_int)
                .unwrap_or(self.frontbuffer as i32) as u32
        } else {
            self.frontbuffer
        };
        self.sprites_created += 1;
        self.sprites
            .insert(
                handle,
                Sprite {
                    page,
                    target_page,
                    group_id: handle,
                    opacity: 255,
                    source,
                    original_source_position: source
                        .map(|r| (r.x, r.y))
                        .unwrap_or((0, 0)),
                    overlay: false,
                    position: None,
                    position_animation: None,
                    alpha_animation: None,
                    frame_animation: None,
                },
            );
        self.sprite_order.push(handle);
        eprintln!(
            "[GFX] sprite_create_file_raw {:?} page={} source={:?} → handle {}",
            String::from_utf8_lossy(filename), page, source.map(| r | (r.x, r.y, r.width,
            r.height)), handle
        );
        handle
    }
    pub(super) fn sprite_set_clip_impl(
        &mut self,
        sprite: u32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) {
        let Some(sprite) = self.sprites.get_mut(&sprite) else {
            eprintln!("[GFX] sprite_clip skipped: invalid sprite={sprite}");
            return;
        };
        sprite.source = Some(SpriteSource {
            x,
            y,
            width,
            height,
        });
        self.render_dirty = true;
    }
    pub(super) fn compose_frontbuffer_snapshot(&self) -> Option<Page> {
        let frontbuffer = base_page_handle(self.frontbuffer);
        let mut destination = self.pages.get(&frontbuffer)?.clone();
        let destination_has_alpha = self.alpha_bindings.contains_key(&frontbuffer)
            || destination.alpha_masked;
        for handle in &self.sprite_order {
            if self.sprite_visibility_depths.get(handle).copied().unwrap_or(0) != 0 {
                continue;
            }
            let Some(sprite) = self.sprites.get(handle) else {
                continue;
            };
            if base_page_handle(sprite.target_page) != frontbuffer {
                continue;
            }
            let Some(anchor) = sprite.position else {
                continue;
            };
            let Some(source_page) = self.pages.get(&sprite.page) else {
                continue;
            };
            let Some((source_x, source_y, source_width, source_height)) = sprite_source_rect(
                sprite,
                source_page,
            ) else {
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
            let transformed = (self.sprite_rotations.contains_key(handle)
                || self.sprite_xmodifies.contains_key(handle)
                || self.sprite_ymodifies.contains_key(handle))
                && (rotation_degrees != 0.0 || scale_x != 1.0 || scale_y != 1.0);
            let (paste_page, source_rect, paste_position, use_source_alpha) = if transformed {
                let Some(transformed_page) = rasterize_transformed_sprite(
                    source_page,
                    source_x as i32,
                    source_y as i32,
                    source_width as i32,
                    source_height as i32,
                    rotation_degrees,
                    scale_x,
                    scale_y,
                    self.internal_w,
                    self.internal_h,
                ) else {
                    continue;
                };
                let position = (
                    anchor.0 - transformed_page.width as i32 / 2,
                    anchor.1 - transformed_page.height as i32 / 2,
                );
                let rect = (
                    0,
                    0,
                    transformed_page.width as i32,
                    transformed_page.height as i32,
                );
                (transformed_page, rect, position, true)
            } else {
                let position = if sprite.overlay {
                    (
                        anchor.0 - source_width as i32 / 2,
                        anchor.1 - source_height as i32 / 2,
                    )
                } else {
                    anchor
                };
                (
                    source_page.clone(),
                    (
                        source_x as i32,
                        source_y as i32,
                        source_width as i32,
                        source_height as i32,
                    ),
                    position,
                    source_page.alpha_masked,
                )
            };
            match paste_page.draw_mode {
                1 => {
                    blend_page_region_additive(
                        &paste_page,
                        &mut destination,
                        source_rect.0,
                        source_rect.1,
                        source_rect.2,
                        source_rect.3,
                        paste_position.0,
                        paste_position.1,
                        sprite.opacity,
                    )
                }
                3 => {
                    blend_page_region_multiply(
                        &paste_page,
                        &mut destination,
                        source_rect.0,
                        source_rect.1,
                        source_rect.2,
                        source_rect.3,
                        paste_position.0,
                        paste_position.1,
                        sprite.opacity,
                    )
                }
                4 => {
                    blend_page_region_lighten(
                        &paste_page,
                        &mut destination,
                        source_rect.0,
                        source_rect.1,
                        source_rect.2,
                        source_rect.3,
                        paste_position.0,
                        paste_position.1,
                        sprite.opacity,
                    )
                }
                _ => {
                    blend_page_region_over(
                        &paste_page,
                        &mut destination,
                        source_rect.0,
                        source_rect.1,
                        source_rect.2,
                        source_rect.3,
                        paste_position.0,
                        paste_position.1,
                        sprite.opacity,
                        use_source_alpha,
                        destination_has_alpha,
                    )
                }
            };
        }
        Some(destination)
    }
    pub(super) fn sprite_paste_impl(
        &mut self,
        sprite: u32,
        dst_page: u32,
        position: Option<(i32, i32)>,
    ) -> Option<SpritePasteReplay> {
        self.update_sprite_animations(true);
        let sp = self.sprites.get(&sprite).cloned()?;
        let source_is_presented_scene = self.reads_synthesized_frontbuffer(sp.page);
        let source_page = self.page_for_read(sp.page)?;
        let (source_x, source_y, source_width, source_height) = sprite_source_rect(
            &sp,
            &source_page,
        )?;
        let rotation_degrees = self
            .sprite_rotations
            .get(&sprite)
            .map(|animation| animation.current)
            .unwrap_or(0.0);
        let scale_x = self
            .sprite_xmodifies
            .get(&sprite)
            .map(|animation| animation.current)
            .unwrap_or(1.0);
        let scale_y = self
            .sprite_ymodifies
            .get(&sprite)
            .map(|animation| animation.current)
            .unwrap_or(1.0);
        let transformed = (self.sprite_rotations.contains_key(&sprite)
            || self.sprite_xmodifies.contains_key(&sprite)
            || self.sprite_ymodifies.contains_key(&sprite))
            && (rotation_degrees != 0.0 || scale_x != 1.0 || scale_y != 1.0);
        let anchor = position.or(sp.position).unwrap_or((100_000, 100_000));
        let (paste_page, source_rect, paste_position, use_source_alpha) = if transformed {
            let transformed_page = rasterize_transformed_sprite(
                &source_page,
                source_x as i32,
                source_y as i32,
                source_width as i32,
                source_height as i32,
                rotation_degrees,
                scale_x,
                scale_y,
                self.internal_w,
                self.internal_h,
            )?;
            let x = anchor.0 - transformed_page.width as i32 / 2;
            let y = anchor.1 - transformed_page.height as i32 / 2;
            let rect = (
                0,
                0,
                transformed_page.width as i32,
                transformed_page.height as i32,
            );
            (transformed_page, rect, (x, y), true)
        } else {
            let position = if sp.overlay {
                (anchor.0 - source_width as i32 / 2, anchor.1 - source_height as i32 / 2)
            } else {
                anchor
            };
            (
                source_page.clone(),
                (
                    source_x as i32,
                    source_y as i32,
                    source_width as i32,
                    source_height as i32,
                ),
                position,
                source_page.alpha_masked,
            )
        };
        let opacity = sp.opacity;
        let dst_page = base_page_handle(dst_page);
        let destination_has_alpha = self.alpha_bindings.contains_key(&dst_page)
            || self.pages.get(&dst_page).is_some_and(|page| page.alpha_masked);
        let destination = self.pages.get_mut(&dst_page)?;
        let changed = match paste_page.draw_mode {
            1 => {
                blend_page_region_additive(
                    &paste_page,
                    destination,
                    source_rect.0,
                    source_rect.1,
                    source_rect.2,
                    source_rect.3,
                    paste_position.0,
                    paste_position.1,
                    opacity,
                )
            }
            3 => {
                blend_page_region_multiply(
                    &paste_page,
                    destination,
                    source_rect.0,
                    source_rect.1,
                    source_rect.2,
                    source_rect.3,
                    paste_position.0,
                    paste_position.1,
                    opacity,
                )
            }
            4 => {
                blend_page_region_lighten(
                    &paste_page,
                    destination,
                    source_rect.0,
                    source_rect.1,
                    source_rect.2,
                    source_rect.3,
                    paste_position.0,
                    paste_position.1,
                    opacity,
                )
            }
            _ => {
                blend_page_region_over(
                    &paste_page,
                    destination,
                    source_rect.0,
                    source_rect.1,
                    source_rect.2,
                    source_rect.3,
                    paste_position.0,
                    paste_position.1,
                    opacity,
                    use_source_alpha,
                    destination_has_alpha,
                )
            }
        };
        if changed {
            self.mark_page_dirty(dst_page);
            self.refresh_alpha_links(dst_page);
        }
        if !changed {
            return None;
        }
        if transformed && matches!(paste_page.draw_mode, 1 | 3 | 4) {
            return None;
        }
        let (source_rect, destination_rect, source_has_alpha, transform) = if transformed {
            (
                [
                    source_x as i32,
                    source_y as i32,
                    source_width as i32,
                    source_height as i32,
                ],
                [
                    anchor.0 - source_width as i32 / 2,
                    anchor.1 - source_height as i32 / 2,
                    source_width as i32,
                    source_height as i32,
                ],
                source_page.alpha_masked,
                Some((rotation_degrees, scale_x, scale_y)),
            )
        } else {
            (
                [source_rect.0, source_rect.1, source_rect.2, source_rect.3],
                [paste_position.0, paste_position.1, source_rect.2, source_rect.3],
                use_source_alpha,
                None,
            )
        };
        Some(SpritePasteReplay {
            source: base_page_handle(sp.page),
            source_is_presented_scene,
            source_rect,
            destination_rect,
            opacity,
            draw_mode: paste_page.draw_mode,
            source_has_alpha,
            destination_has_alpha,
            transform,
        })
    }
    pub(super) fn sprite_set_alpha_impl(&mut self, sprite: u32, alpha: u8) {
        if let Some(sp) = self.sprites.get_mut(&sprite) {
            sp.opacity = 255 - alpha;
            sp.alpha_animation = None;
            self.render_dirty = true;
        }
    }
    pub(super) fn sprite_mark_overlay_impl(&mut self, sprite: u32) {
        if let Some(sp) = self.sprites.get_mut(&sprite) {
            sp.overlay = true;
            self.render_dirty = true;
            eprintln!("[GFX] sprite_overlay sp={} page={}", sprite, sp.page);
        }
    }
    pub(super) fn sprite_move_impl(
        &mut self,
        sprite: u32,
        x: i32,
        y: i32,
        timing: Option<i32>,
    ) {
        if let Some(sp) = self.sprites.get_mut(&sprite) {
            let current = match sp.position_animation.as_ref() {
                Some(animation) if animation.state == 1 && animation.duration_ms == 0 => {
                    (
                        animation.start.0.wrapping_add(animation.delta.0),
                        animation.start.1.wrapping_add(animation.delta.1),
                    )
                }
                _ => sp.position.unwrap_or((100_000, 100_000)),
            };
            let raw_timing = timing.unwrap_or(0) as u32;
            let duration_ms = (raw_timing & 0x84FF_FFFF) as i32;
            let easing_flags = (raw_timing & 0x7800_0000) as i32;
            sp.position = Some(current);
            sp.position_animation = Some(SpritePositionAnimation {
                state: 1,
                start: current,
                delta: (x.wrapping_sub(current.0), y.wrapping_sub(current.1)),
                started_ms: self.clock_timestamp_ms,
                duration_ms,
                easing_flags,
            });
            self.render_dirty = true;
            eprintln!(
                "[GFX] sprite_move sp={} target=({},{}) duration={} easing=0x{:08X}",
                sprite, x, y, duration_ms, easing_flags
            );
        }
    }
    pub(super) fn sprite_is_moving_impl(&mut self, sprite: u32) -> bool {
        self.sprites
            .get(&sprite)
            .and_then(|sprite| sprite.position_animation.as_ref())
            .is_some_and(|animation| animation.state == 1)
    }
    pub(super) fn sprite_is_alpha_animating_impl(&mut self, sprite: u32) -> bool {
        self.sprites.get(&sprite).is_some_and(|sprite| sprite.alpha_animation.is_some())
    }
    pub(super) fn sprite_is_frame_animating_impl(&mut self, sprite: u32) -> bool {
        self.sprites
            .get(&sprite)
            .and_then(|sprite| sprite.frame_animation.as_ref())
            .is_some_and(|animation| animation.state == 1)
    }
    pub(super) fn sprite_priority_high_impl(&mut self, sprite: u32) {
        if reprioritize_sprite(
            &mut self.sprites,
            &mut self.sprite_order,
            sprite,
            None,
            true,
            true,
        ) {
            self.render_dirty = true;
        }
    }
    pub(super) fn sprite_priority_high_group_impl(
        &mut self,
        sprite: u32,
        reference: Option<u32>,
    ) {
        if reprioritize_sprite(
            &mut self.sprites,
            &mut self.sprite_order,
            sprite,
            reference,
            true,
            true,
        ) {
            self.render_dirty = true;
        }
    }
    pub(super) fn sprite_priority_high_single_impl(
        &mut self,
        sprite: u32,
        reference: Option<u32>,
    ) {
        if reprioritize_sprite(
            &mut self.sprites,
            &mut self.sprite_order,
            sprite,
            reference,
            true,
            false,
        ) {
            self.render_dirty = true;
        }
    }
    pub(super) fn sprite_priority_low_impl(&mut self, sprite: u32) {
        if reprioritize_sprite(
            &mut self.sprites,
            &mut self.sprite_order,
            sprite,
            None,
            false,
            true,
        ) {
            self.render_dirty = true;
        }
    }
    pub(super) fn sprite_priority_low_group_impl(
        &mut self,
        sprite: u32,
        reference: Option<u32>,
    ) {
        if reprioritize_sprite(
            &mut self.sprites,
            &mut self.sprite_order,
            sprite,
            reference,
            false,
            true,
        ) {
            self.render_dirty = true;
        }
    }
    pub(super) fn sprite_priority_low_single_impl(
        &mut self,
        sprite: u32,
        reference: Option<u32>,
    ) {
        if reprioritize_sprite(
            &mut self.sprites,
            &mut self.sprite_order,
            sprite,
            reference,
            false,
            false,
        ) {
            self.render_dirty = true;
        }
    }
    pub(super) fn sprite_get_page_impl(&mut self, sprite: u32) -> u32 {
        self.sprites.get(&sprite).map_or(0, |sprite| sprite.page)
    }
    pub(super) fn sprite_width_impl(&mut self, sprite: u32) -> i32 {
        let Some(sprite) = self.sprites.get(&sprite) else {
            return 0;
        };
        sprite
            .source
            .map(|source| source.width)
            .or_else(|| self.pages.get(&sprite.page).map(|page| page.width as i32))
            .unwrap_or(0)
    }
    pub(super) fn sprite_height_impl(&mut self, sprite: u32) -> i32 {
        let Some(sprite) = self.sprites.get(&sprite) else {
            return 0;
        };
        sprite
            .source
            .map(|source| source.height)
            .or_else(|| self.pages.get(&sprite.page).map(|page| page.height as i32))
            .unwrap_or(0)
    }
    pub(super) fn sprite_pos_x_impl(&mut self, sprite: u32) -> i32 {
        self.sprites
            .get(&sprite)
            .and_then(native_sprite_query_position)
            .map_or(0, |position| position.0)
    }
    pub(super) fn sprite_pos_y_impl(&mut self, sprite: u32) -> i32 {
        self.sprites
            .get(&sprite)
            .and_then(native_sprite_query_position)
            .map_or(0, |position| position.1)
    }
    pub(super) fn sprite_release_impl(&mut self, sprite: u32) {
        if self.scene_resident_sprites.contains(&sprite) {
            return;
        }
        if self.sprites.contains_key(&sprite) {
            self.sprites_released += 1;
        }
        let smooth_page = self.sprite_smooth_pages.remove(&sprite);
        self.sprite_smooth_page_states.remove(&sprite);
        let auto_release_page = self
            .sprites
            .remove(&sprite)
            .filter(|_| self.sprite_auto_release_pages.remove(&sprite))
            .map(|removed| removed.page);
        self.sprite_visibility_depths.remove(&sprite);
        self.sprite_smooth_animation.remove(&sprite);
        self.sprite_rotations.remove(&sprite);
        self.sprite_xmodifies.remove(&sprite);
        self.sprite_ymodifies.remove(&sprite);
        self.sprite_order.retain(|handle| *handle != sprite);
        self.scene_resident_sprites.remove(&sprite);
        self.render_dirty = true;
        if let Some(page) = auto_release_page {
            self.page_release(page);
        }
        if let Some(page) = smooth_page {
            self.release_unowned_page(page);
        }
    }
    pub(super) fn sprite_visibility_push_impl(&mut self, sprite: Option<u32>) {
        if let Some(sprite) = sprite {
            if self.sprites.contains_key(&sprite) {
                let depth = self.sprite_visibility_depths.entry(sprite).or_default();
                *depth = depth.wrapping_sub(1);
            }
        } else {
            for sprite in self.sprites.keys().copied() {
                let depth = self.sprite_visibility_depths.entry(sprite).or_default();
                *depth = depth.wrapping_sub(1);
            }
        }
        self.render_dirty = true;
    }
    pub(super) fn sprite_visibility_pop_impl(&mut self, sprite: Option<u32>) {
        let mut pop = |sprite| {
            let depth = self.sprite_visibility_depths.entry(sprite).or_default();
            *depth = depth.wrapping_add(1).min(0);
        };
        if let Some(sprite) = sprite {
            if self.sprites.contains_key(&sprite) {
                pop(sprite);
            }
        } else {
            for sprite in self.sprites.keys().copied().collect::<Vec<_>>() {
                pop(sprite);
            }
        }
        self.render_dirty = true;
    }
    pub(super) fn sprite_page_auto_release_impl(&mut self, sprite: u32) {
        if self.sprites.contains_key(&sprite) {
            self.sprite_auto_release_pages.insert(sprite);
        }
    }
    pub(super) fn sprite_set_smooth_animation_impl(&mut self, sprite: u32) {
        if self.sprites.contains_key(&sprite) {
            self.sprite_smooth_animation.insert(sprite);
        }
    }
    pub(super) fn update_smooth_animation_pages(&mut self) {
        let handles = self.sprite_smooth_animation.iter().copied().collect::<Vec<_>>();
        for sprite_handle in handles {
            let Some(sprite) = self.sprites.get(&sprite_handle).cloned() else {
                continue;
            };
            let Some(source_page) = self.pages.get(&sprite.page).cloned() else {
                continue;
            };
            let Some((source_x, source_y, source_width, source_height)) = sprite_source_rect(
                &sprite,
                &source_page,
            ) else {
                continue;
            };
            let (source_x, source_y, width, height) = (
                source_x as usize,
                source_y as usize,
                source_width as usize,
                source_height as usize,
            );
            if width == 0 || height == 0 {
                continue;
            }
            let smooth_target = sprite
                .frame_animation
                .as_ref()
                .and_then(|animation| {
                    if animation.state == 0 || animation.segment_index < 0
                        || animation.segment_duration_ms <= 0
                        || animation.keyframes.len() <= 1
                    {
                        return None;
                    }
                    let elapsed = self
                        .clock_timestamp_ms
                        .wrapping_sub(animation.started_ms)
                        .wrapping_abs()
                        .clamp(0, animation.segment_duration_ms);
                    let current_weight = (255
                        - elapsed.wrapping_mul(255) / animation.segment_duration_ms)
                        .clamp(0, 255) as u32;
                    let next_index = (animation.segment_index as usize + 1)
                        % animation.keyframes.len();
                    let next = source_for_animation_frame(
                        &sprite,
                        &source_page,
                        animation.keyframes[next_index].0,
                    );
                    Some((next, current_weight))
                });
            let smooth_state = SmoothPageState {
                source_page: sprite.page,
                source_revision: source_page.revision,
                source: SpriteSource {
                    x: source_x as i32,
                    y: source_y as i32,
                    width: width as i32,
                    height: height as i32,
                },
                target: smooth_target,
            };
            let scratch_is_current = self
                .sprite_smooth_page_states
                .get(&sprite_handle)
                .is_some_and(|state| *state == smooth_state)
                && self
                    .sprite_smooth_pages
                    .get(&sprite_handle)
                    .is_some_and(|scratch| self.pages.contains_key(scratch));
            if scratch_is_current {
                continue;
            }
            let mut pixels = vec![0u8; width * height * 4];
            let mut indexed_samples = source_page
                .indexed_samples
                .as_ref()
                .map(|_| vec![0u8; width * height]);
            for y in 0..height {
                for x in 0..width {
                    let current_offset = ((source_y + y) * source_page.width as usize
                        + source_x + x) * 4;
                    let output_offset = (y * width + x) * 4;
                    let (next_offset, current_weight) = smooth_target
                        .map(|(next, weight)| {
                            let next_x = next.x.max(0) as usize + x;
                            let next_y = next.y.max(0) as usize + y;
                            ((next_y * source_page.width as usize + next_x) * 4, weight)
                        })
                        .filter(|(offset, _)| *offset + 3 < source_page.pixels.len())
                        .unwrap_or((current_offset, 255));
                    let next_weight = 256 - current_weight;
                    for channel in 0..3 {
                        pixels[output_offset + channel] = ((u32::from(
                            source_page.pixels[next_offset + channel],
                        ) * next_weight
                            + u32::from(source_page.pixels[current_offset + channel])
                                * (current_weight + 1)) >> 8) as u8;
                    }
                    pixels[output_offset + 3] = if source_page.alpha_masked {
                        let current_antidata = 255
                            - u32::from(source_page.pixels[current_offset + 3]);
                        let next_antidata = 255
                            - u32::from(source_page.pixels[next_offset + 3]);
                        let blended = (next_antidata * next_weight
                            + current_antidata * (current_weight + 1)) >> 8;
                        255 - blended as u8
                    } else {
                        255
                    };
                    if let Some(samples) = indexed_samples.as_mut() {
                        let current = native_8bit_sample(
                            &source_page,
                            source_x + x,
                            source_y + y,
                        );
                        let next_x = (next_offset / 4) % source_page.width as usize;
                        let next_y = (next_offset / 4) / source_page.width as usize;
                        let next = native_8bit_sample(&source_page, next_x, next_y);
                        samples[y * width + x] = ((u32::from(next) * next_weight
                            + u32::from(current) * (current_weight + 1)) >> 8) as u8;
                    }
                }
            }
            let scratch = if let Some(&scratch) = self
                .sprite_smooth_pages
                .get(&sprite_handle)
            {
                scratch
            } else {
                let scratch = self.alloc_handle();
                self.sprite_smooth_pages.insert(sprite_handle, scratch);
                scratch
            };
            let revision = self.alloc_page_revision();
            let pixels = Arc::new(pixels);
            self.pages
                .insert(
                    scratch,
                    Page {
                        width: width as u32,
                        height: height as u32,
                        pixels: pixels.clone(),
                        presentation_seed: pixels,
                        presentation: None,
                        indexed_samples: indexed_samples.map(Arc::new),
                        indexed_palette: source_page.indexed_palette.clone(),
                        revision,
                        alpha_masked: source_page.alpha_masked,
                        draw_mode: source_page.draw_mode,
                    },
                );
            self.dirty_pages.insert(scratch);
            if crate::diag_log_enabled() {
                std::eprintln!(
                    "[SMOOTH] sprite={} source_page={} scratch={} {}x{} source_alpha_masked={} blend={}",
                    sprite_handle, sprite.page, scratch, width, height, source_page
                    .alpha_masked, smooth_target.is_some(),
                );
            }
            self.sprite_smooth_page_states.insert(sprite_handle, smooth_state);
        }
    }
    pub(super) fn sprite_rotate_impl(
        &mut self,
        sprite: u32,
        target_degrees: f32,
        duration_ms: i32,
        extrapolate: bool,
    ) {
        if !self.sprites.contains_key(&sprite) {
            return;
        }
        let current = self
            .sprite_rotations
            .get(&sprite)
            .map(|animation| animation.current)
            .unwrap_or(0.0);
        let state = if duration_ms == 0 { 0 } else if extrapolate { 2 } else { 1 };
        self.sprite_rotations
            .insert(
                sprite,
                SpriteRotationAnimation {
                    state,
                    current: if duration_ms == 0 { target_degrees } else { current },
                    previous: if duration_ms == 0 { target_degrees } else { current },
                    target: target_degrees,
                    started_ms: self.clock_timestamp_ms,
                    duration_ms,
                },
            );
        self.render_dirty = true;
    }
    pub(super) fn sprite_xmodify_define_impl(
        &mut self,
        sprite: u32,
        keyframes: &[(f32, i32)],
    ) {
        if !self.sprites.contains_key(&sprite) || keyframes.is_empty() {
            return;
        }
        let current = self
            .sprite_xmodifies
            .get(&sprite)
            .map(|animation| animation.current)
            .unwrap_or(1.0);
        self.sprite_xmodifies
            .insert(
                sprite,
                SpriteScaleAnimation {
                    state: 1,
                    current,
                    started_ms: self.clock_timestamp_ms,
                    segment_duration_ms: 0,
                    segment_index: -1,
                    previous: current,
                    easing_flags: 0,
                    ease_out_mask: 0x4000_0000,
                    ease_in_mask: 0x2000_0000,
                    keyframes: keyframes.to_vec(),
                },
            );
        self.render_dirty = true;
    }
    pub(super) fn sprite_xmodify_set_impl(
        &mut self,
        sprite: u32,
        target: f32,
        duration_ms: i32,
    ) {
        if !self.sprites.contains_key(&sprite) {
            return;
        }
        let current = self
            .sprite_xmodifies
            .get(&sprite)
            .map(|animation| animation.current)
            .unwrap_or(1.0);
        self.sprite_xmodifies
            .insert(
                sprite,
                scale_set_animation(
                    current,
                    target,
                    duration_ms,
                    self.clock_timestamp_ms,
                    0x4000_0000,
                    0x2000_0000,
                ),
            );
        self.render_dirty = true;
    }
    pub(super) fn sprite_ymodify_define_impl(
        &mut self,
        sprite: u32,
        keyframes: &[(f32, i32)],
    ) {
        if !self.sprites.contains_key(&sprite) || keyframes.is_empty() {
            return;
        }
        let current = self
            .sprite_ymodifies
            .get(&sprite)
            .map(|animation| animation.current)
            .unwrap_or(1.0);
        self.sprite_ymodifies
            .insert(
                sprite,
                SpriteScaleAnimation {
                    state: 1,
                    current,
                    started_ms: self.clock_timestamp_ms,
                    segment_duration_ms: 0,
                    segment_index: -1,
                    previous: current,
                    easing_flags: 0,
                    ease_out_mask: 0x1000_0000,
                    ease_in_mask: 0x0800_0000,
                    keyframes: keyframes.to_vec(),
                },
            );
        self.render_dirty = true;
    }
    pub(super) fn sprite_ymodify_set_impl(
        &mut self,
        sprite: u32,
        target: f32,
        duration_ms: i32,
    ) {
        if !self.sprites.contains_key(&sprite) {
            return;
        }
        let current = self
            .sprite_ymodifies
            .get(&sprite)
            .map(|animation| animation.current)
            .unwrap_or(1.0);
        self.sprite_ymodifies
            .insert(
                sprite,
                scale_set_animation(
                    current,
                    target,
                    duration_ms,
                    self.clock_timestamp_ms,
                    0x1000_0000,
                    0x0800_0000,
                ),
            );
        self.render_dirty = true;
    }
    pub(super) fn sprite_exists_impl(&mut self, handle: u32) -> bool {
        self.sprites.contains_key(&handle)
    }
    pub(super) fn sprite_alfa_set_impl(
        &mut self,
        sprite: u32,
        alpha: i32,
        animate: i32,
    ) {
        if let Some(sp) = self.sprites.get_mut(&sprite) {
            let native_transparency = alpha.clamp(0, 255) as u8;
            if animate > 0 {
                sp.alpha_animation = Some(SpriteAlphaAnimation {
                    started_ms: self.clock_timestamp_ms,
                    segment_duration_ms: 0,
                    segment_index: -1,
                    previous_transparency: 255 - sp.opacity,
                    keyframes: vec![
                        (native_transparency, animate), (native_transparency, - 1)
                    ],
                });
            } else {
                sp.opacity = 255 - native_transparency;
                sp.alpha_animation = None;
            }
            if sprite <= 40 {
                eprintln!(
                    "[GFX-DIAG] sprite_alfa_set sp={} native={} duration={} opacity={}",
                    sprite, native_transparency, animate, sp.opacity
                );
            }
            self.render_dirty = true;
        }
    }
    pub(super) fn sprite_alfa_define_impl(
        &mut self,
        sprite: u32,
        keyframes: &[(i32, i32)],
    ) {
        if let Some(sp) = self.sprites.get_mut(&sprite) {
            let keyframes = keyframes
                .iter()
                .map(|&(target, duration)| (target.clamp(0, 255) as u8, duration))
                .collect::<Vec<_>>();
            sp.alpha_animation = Some(SpriteAlphaAnimation {
                started_ms: self.clock_timestamp_ms,
                segment_duration_ms: 0,
                segment_index: -1,
                previous_transparency: 255 - sp.opacity,
                keyframes,
            });
            self.render_dirty = true;
        }
    }
    pub(super) fn sprite_animate_define_aligned_impl(
        &mut self,
        sprite: u32,
        keyframes: &[(i32, i32)],
        total_duration: i32,
    ) {
        let cycle = if total_duration == 0 { 1 } else { total_duration };
        let quotient = self.clock_timestamp_ms.checked_div(cycle).unwrap_or(i32::MIN);
        let started_ms = cycle.wrapping_mul(quotient);
        if let Some(sp) = self.sprites.get_mut(&sprite) {
            sp.frame_animation = Some(SpriteFrameAnimation {
                state: 1,
                started_ms,
                segment_duration_ms: 0,
                segment_index: -1,
                keyframes: keyframes.to_vec(),
            });
            self.render_dirty = true;
        }
    }
    pub(super) fn sprite_animate_define_impl(
        &mut self,
        sprite: u32,
        keyframes: &[(i32, i32)],
    ) {
        let Some(sp) = self.sprites.get_mut(&sprite) else {
            return;
        };
        if keyframes.is_empty() {
            sp.frame_animation = None;
        } else if keyframes.len() < 64 {
            sp.frame_animation = Some(SpriteFrameAnimation {
                state: 1,
                started_ms: self.clock_timestamp_ms,
                segment_duration_ms: 0,
                segment_index: -1,
                keyframes: keyframes.to_vec(),
            });
        }
        self.render_dirty = true;
    }
    pub(super) fn sprite_animate_add_impl(
        &mut self,
        sprite: u32,
        keyframes: &[(i32, i32)],
    ) {
        if keyframes.is_empty() {
            return;
        }
        let Some(sp) = self.sprites.get_mut(&sprite) else {
            return;
        };
        match sp.frame_animation.as_mut() {
            Some(animation) => {
                if animation.keyframes.len().saturating_add(keyframes.len()) < 64 {
                    animation.keyframes.extend_from_slice(keyframes);
                }
            }
            None if keyframes.len() < 64 => {
                sp.frame_animation = Some(SpriteFrameAnimation {
                    state: 1,
                    started_ms: self.clock_timestamp_ms,
                    segment_duration_ms: 0,
                    segment_index: -1,
                    keyframes: keyframes.to_vec(),
                });
            }
            None => {}
        }
        self.render_dirty = true;
    }
}
pub(super) fn compose_presented_scene_snapshot(
    scene: &PresentedScene,
    internal_w: u32,
    internal_h: u32,
) -> Option<Page> {
    let mut destination = match scene
        .display_page
        .and_then(|handle| scene.pages.get(&base_page_handle(handle)))
    {
        Some(page) if page.width == internal_w && page.height == internal_h => {
            page.clone()
        }
        Some(page) => {
            let mut scaled = Page {
                width: internal_w,
                height: internal_h,
                pixels: Arc::new(vec![0; internal_w as usize * internal_h as usize * 4]),
                presentation_seed: page.presentation_seed.clone(),
                presentation: None,
                indexed_samples: None,
                indexed_palette: None,
                revision: page.revision,
                alpha_masked: false,
                draw_mode: 0,
            };
            scale_copy_page_region(
                page,
                &mut scaled,
                0,
                0,
                page.width as i32,
                page.height as i32,
                0,
                0,
                internal_w as i32,
                internal_h as i32,
            );
            scaled.presentation_seed = scaled.pixels.clone();
            scaled
        }
        None => {
            Page {
                width: internal_w,
                height: internal_h,
                pixels: Arc::new(vec![0; internal_w as usize * internal_h as usize * 4]),
                presentation_seed: Arc::new(
                    vec![0; internal_w as usize * internal_h as usize * 4],
                ),
                presentation: None,
                indexed_samples: None,
                indexed_palette: None,
                revision: 0,
                alpha_masked: false,
                draw_mode: 0,
            }
        }
    };
    for quad in &scene.quads {
        let Some(source_page) = scene.pages.get(&base_page_handle(quad.page)) else {
            continue;
        };
        let source_rect = (
            quad.source_x.round() as i32,
            quad.source_y.round() as i32,
            quad.source_width.round() as i32,
            quad.source_height.round() as i32,
        );
        if source_rect.2 <= 0 || source_rect.3 <= 0 {
            continue;
        }
        let transformed = quad.rotation_degrees != 0.0 || quad.scale_x != 1.0
            || quad.scale_y != 1.0;
        let (paste_page, paste_rect, paste_position, source_has_alpha) = if transformed {
            let Some(page) = rasterize_transformed_sprite(
                source_page,
                source_rect.0,
                source_rect.1,
                source_rect.2,
                source_rect.3,
                quad.rotation_degrees,
                quad.scale_x,
                quad.scale_y,
                internal_w,
                internal_h,
            ) else {
                continue;
            };
            let width = page.width as i32;
            let height = page.height as i32;
            let centre_x = quad.x + quad.width * 0.5;
            let centre_y = quad.y + quad.height * 0.5;
            (
                page,
                (0, 0, width, height),
                (
                    (centre_x - width as f32 * 0.5).round() as i32,
                    (centre_y - height as f32 * 0.5).round() as i32,
                ),
                true,
            )
        } else {
            (
                source_page.clone(),
                source_rect,
                (quad.x.round() as i32, quad.y.round() as i32),
                quad.source_has_alpha,
            )
        };
        let opacity = (quad.alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
        let destination_has_alpha = destination.alpha_masked;
        match quad.draw_mode {
            1 => {
                blend_page_region_additive(
                    &paste_page,
                    &mut destination,
                    paste_rect.0,
                    paste_rect.1,
                    paste_rect.2,
                    paste_rect.3,
                    paste_position.0,
                    paste_position.1,
                    opacity,
                )
            }
            3 => {
                blend_page_region_multiply(
                    &paste_page,
                    &mut destination,
                    paste_rect.0,
                    paste_rect.1,
                    paste_rect.2,
                    paste_rect.3,
                    paste_position.0,
                    paste_position.1,
                    opacity,
                )
            }
            4 => {
                blend_page_region_lighten(
                    &paste_page,
                    &mut destination,
                    paste_rect.0,
                    paste_rect.1,
                    paste_rect.2,
                    paste_rect.3,
                    paste_position.0,
                    paste_position.1,
                    opacity,
                )
            }
            _ => {
                blend_page_region_over(
                    &paste_page,
                    &mut destination,
                    paste_rect.0,
                    paste_rect.1,
                    paste_rect.2,
                    paste_rect.3,
                    paste_position.0,
                    paste_position.1,
                    opacity,
                    source_has_alpha,
                    destination_has_alpha,
                )
            }
        };
    }
    Some(destination)
}
#[allow(clippy::too_many_arguments)]
fn rasterize_transformed_sprite(
    source: &Page,
    source_x: i32,
    source_y: i32,
    source_width: i32,
    source_height: i32,
    rotation_degrees: f32,
    scale_x: f32,
    scale_y: f32,
    clip_w: u32,
    clip_h: u32,
) -> Option<Page> {
    if source_width <= 0 || source_height <= 0 || !rotation_degrees.is_finite()
        || !scale_x.is_finite() || !scale_y.is_finite()
        || f64::from(scale_x).abs() <= 0.002 || f64::from(scale_y).abs() <= 0.002
    {
        return None;
    }
    let radians = f64::from(rotation_degrees).to_radians();
    let (sin, cos) = radians.sin_cos();
    let scaled_width = f64::from(source_width) * f64::from(scale_x).abs();
    let scaled_height = f64::from(source_height) * f64::from(scale_y).abs();
    let transformed_width = (cos.abs() * scaled_width + sin.abs() * scaled_height)
        .ceil();
    let transformed_height = (sin.abs() * scaled_width + cos.abs() * scaled_height)
        .ceil();
    if transformed_width < 1.0 || transformed_height < 1.0 {
        return None;
    }
    let width = transformed_width.min(f64::from(clip_w)) as u32;
    let height = transformed_height.min(f64::from(clip_h)) as u32;
    let byte_len = (width as usize).checked_mul(height as usize)?.checked_mul(4)?;
    let mut transformed = Page {
        width,
        height,
        pixels: Arc::new(vec![0; byte_len]),
        presentation_seed: Arc::new(vec![0; byte_len]),
        presentation: None,
        indexed_samples: None,
        indexed_palette: None,
        revision: 0,
        alpha_masked: true,
        draw_mode: source.draw_mode,
    };
    let changed = transform_copy_page_region(
        source,
        &mut transformed,
        source_x,
        source_y,
        source_width,
        source_height,
        0,
        0,
        width as i32,
        height as i32,
        rotation_degrees,
        scale_x,
        scale_y,
        source.alpha_masked,
        true,
    );
    transformed.presentation_seed = transformed.pixels.clone();
    changed.then_some(transformed)
}
fn native_sprite_query_position(sprite: &Sprite) -> Option<(i32, i32)> {
    match sprite.position_animation.as_ref() {
        Some(animation) if animation.duration_ms == 0 => {
            Some((
                animation.start.0.wrapping_add(animation.delta.0),
                animation.start.1.wrapping_add(animation.delta.1),
            ))
        }
        _ => sprite.position,
    }
}
