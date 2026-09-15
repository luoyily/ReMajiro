use super::*;
const INDEX_LEFT: usize = 130;
const INDEX_RIGHT: usize = 131;
const INDEX_UP: usize = 132;
const INDEX_DOWN: usize = 133;
const INDEX_MIDDLE: usize = 136;
const INDEX_AUX: usize = 147;
const INDEX_PGUP: usize = 33;
const INDEX_PGDN: usize = 34;
impl EngineHost {
    pub(super) fn reset_input_for_load_impl(&mut self) {
        self.key_live_held.fill(false);
        self.key_live_pressed_ms.fill(0);
        self.key_snapshot_held.fill(false);
        self.key_snapshot_pressed_ms.fill(0);
        self.input_snapshot_generation = self.input_snapshot_generation.wrapping_add(1);
        self.snapshot_button_edges = [false; 3];
        self.input_contexts.clear();
        self.viewport_offset = (0, 0);
    }
    pub(super) fn mouse_x_impl(&mut self) -> i32 {
        self.logical_pointer_override
            .map(|position| position.0)
            .unwrap_or_else(|| self.mouse_pos.0.saturating_add(self.viewport_offset.0))
    }
    pub(super) fn mouse_y_impl(&mut self) -> i32 {
        self.logical_pointer_override
            .map(|position| position.1)
            .unwrap_or_else(|| self.mouse_pos.1.saturating_add(self.viewport_offset.1))
    }
    pub fn pointer_client_position(&self) -> (i32, i32) {
        self.mouse_pos
    }
    pub(super) fn cursor_inside_impl(&mut self) -> bool {
        self.cursor_inside && self.mouse_x_impl() >= 0 && self.mouse_y_impl() >= 0
            && self.mouse_x_impl() < self.internal_w as i32
            && self.mouse_y_impl() < self.internal_h as i32
    }
    pub(super) fn set_viewport_offset_impl(&mut self, x: i32, y: i32) {
        self.viewport_offset = (x, y);
        self.render_dirty = true;
    }
    pub(super) fn scene_set_origin_impl(&mut self, x: i32, y: i32) {
        self.set_logical_pointer(x, y);
    }
    pub(super) fn warp_cursor_impl(&mut self, x: i32, y: i32) {
        let target = if x == -10_000 {
            self.saved_mouse_pos_before_warp.take().unwrap_or(self.mouse_pos)
        } else {
            self.saved_mouse_pos_before_warp.get_or_insert(self.mouse_pos);
            (
                x.saturating_sub(self.viewport_offset.0),
                y.saturating_sub(self.viewport_offset.1),
            )
        };
        self.logical_pointer_override = None;
        self.mouse_pos = target;
        self.pending_cursor_warp = Some(target);
    }
    fn set_logical_pointer(&mut self, x: i32, y: i32) {
        self.logical_pointer_override = (x != -10_000).then_some((x, y));
    }
    pub fn take_cursor_warp(&mut self) -> Option<(i32, i32)> {
        self.pending_cursor_warp.take()
    }
    pub fn take_cursor_visibility(&mut self) -> Option<bool> {
        self.cursor_visibility_dirty
            .then(|| {
                self.cursor_visibility_dirty = false;
                self.cursor_visible
            })
    }
    pub(super) fn cursor_show_or_hide_impl(&mut self, show: bool) -> bool {
        let was_visible = self.cursor_visible;
        self.cursor_visible = show;
        self.cursor_visibility_dirty = true;
        was_visible
    }
    pub(super) fn set_capslock_state_impl(&mut self, desired_on: bool) {
        self.capslock_on = desired_on;
    }
    pub(super) fn input_suppression_bypassed_impl(&mut self) -> bool {
        self.save_last_command_active
    }
    pub(super) fn input_is_held_or_recent_impl(&mut self, index: usize) -> bool {
        index < self.key_snapshot_held.len() && self.snapshot_index_active(index, 15)
    }
    pub(super) fn input_consume_recent_press_impl(&mut self, index: usize) {
        if index < self.key_snapshot_pressed_ms.len() {
            let _ = self.consume_snapshot_edge(index, 60);
        }
    }
    pub(super) fn set_native_mode_flags_impl(&mut self, bits: u8, special: bool) {
        self.native_mode_bits = bits & 0b111;
        self.native_mode_special = special;
    }
    pub(super) fn effect_skip_active_impl(&mut self) -> bool {
        self.native_mode_bits & 0b001 != 0 || self.native_mode_special
    }
    pub(super) fn set_click_suppression_mask_impl(&mut self, mask: i32) {
        self.click_suppression_mask = (mask as u8) & 0b111;
        self.click_suppression_remaining = 3;
    }
    pub(super) fn trigger_middle_input_pulse_impl(&mut self) {
        self.set_live_index(INDEX_MIDDLE, true);
        self.set_live_index(INDEX_MIDDLE, false);
    }
    pub(super) fn key_modifier_mask_impl(&mut self) -> u32 {
        [INDEX_LEFT, INDEX_RIGHT, INDEX_MIDDLE, INDEX_PGUP, INDEX_PGDN]
            .into_iter()
            .enumerate()
            .fold(
                0,
                |mask, (bit, index)| {
                    mask | (u32::from(self.snapshot_index_active(index, 15)) << bit)
                },
            )
    }
    pub(super) fn recent_key_event_mask_impl(&mut self, context_id: u32) -> u32 {
        let mut mask = u32::from(self.scheduler_button_latches_impl(context_id));
        for (bit, index) in [(3, INDEX_PGUP), (4, INDEX_PGDN), (5, INDEX_AUX)] {
            if self.consume_snapshot_edge(index, 60) {
                mask |= 1 << bit;
            }
        }
        mask
    }
    pub(super) fn scheduler_button_latches_impl(&mut self, context_id: u32) -> u8 {
        let generation = self.input_snapshot_generation;
        let edges = self.snapshot_button_edges;
        let state = self.input_contexts.entry(context_id).or_default();
        if state.latch_generation != generation {
            state.latch_generation = generation;
            for (latched, edge) in state.button_latches.iter_mut().zip(edges) {
                *latched |= edge;
            }
        }
        state
            .button_latches
            .iter()
            .enumerate()
            .fold(0, |mask, (bit, &latched)| mask | (u8::from(latched) << bit))
    }
    pub(super) fn consume_scheduler_button_latches_impl(
        &mut self,
        context_id: u32,
        mask: u8,
    ) {
        let state = self.input_contexts.entry(context_id).or_default();
        for (bit, latched) in state.button_latches.iter_mut().enumerate() {
            if mask & (1 << bit) != 0 {
                *latched = false;
            }
        }
    }
    pub(super) fn four_key_wait_active_impl(&mut self) -> bool {
        [INDEX_AUX, INDEX_MIDDLE, INDEX_RIGHT, INDEX_LEFT]
            .into_iter()
            .any(|index| self.snapshot_index_active(index, 15))
    }
    pub(super) fn movie_wait_event_flags_impl(&mut self, context_id: u32) -> u8 {
        self.scheduler_button_latches_impl(context_id)
    }
    pub(super) fn movie_wait_cancel_pressed_impl(&mut self) -> bool {
        self.snapshot_index_active(0x1B, 15)
    }
    pub(super) fn clear_movie_wait_input_impl(&mut self, context_id: u32) {
        self.key_live_pressed_ms[INDEX_PGUP] = 0;
        self.key_live_pressed_ms[INDEX_PGDN] = 0;
        let state = self.input_contexts.entry(context_id).or_default();
        state.button_latches = [false; 3];
        state.latch_generation = self.input_snapshot_generation;
    }
    pub(super) fn key_capslock_on_impl(&mut self) -> bool {
        self.capslock_on
    }
    pub(super) fn key_shift_pressed_impl(&mut self) -> bool {
        self.key_snapshot_held[0x10]
    }
    pub(super) fn add_hotspot_impl(
        &mut self,
        context_id: u32,
        frame_depth: usize,
        hotspot: Hotspot,
    ) {
        eprintln!(
            "[HOTSPOT] add ctx={} depth={} id={} rect=({},{},{},{}) callbacks=({:08X},{:08X},{:08X}) group={}",
            context_id, frame_depth, hotspot.id, hotspot.x, hotspot.y, hotspot.width,
            hotspot.height, hotspot.on_enter, hotspot.on_leave, hotspot.on_click, hotspot
            .group
        );
        self.input_contexts
            .entry(context_id)
            .or_default()
            .hotspots
            .push(HotspotRuntime {
                hotspot,
                state: 0,
                created_frame_depth: frame_depth,
                keyboard_marked: false,
            });
    }
    pub(super) fn set_frame_input_callbacks_impl(
        &mut self,
        context_id: u32,
        frame_depth: usize,
        callbacks: FrameInputCallbacks,
    ) {
        self
            .input_contexts
            .entry(context_id)
            .or_default()
            .frames
            .entry(frame_depth)
            .or_default()
            .callbacks = callbacks;
        eprintln!(
            "[HOTSPOT] set_frame_callbacks ctx={} depth={} a=0x{:X} b=0x{:X} c=0x{:X} key=0x{:X}",
            context_id, frame_depth, callbacks.event_a, callbacks.event_b, callbacks
            .event_c, callbacks.key_event
        );
    }
    pub(super) fn set_hotspot_origin_impl(&mut self, context_id: u32, x: i32, y: i32) {
        self.input_contexts.entry(context_id).or_default().origin = (x, y);
        eprintln!("[HOTSPOT] set_origin ctx={} ({},{})", context_id, x, y);
    }
    pub(super) fn hotspot_process_impl(
        &mut self,
        context_id: u32,
        frame_depth: usize,
    ) -> Option<HotspotEvent> {
        let mut button_latches = self.take_context_button_latches(context_id);
        let direction = std::mem::take(&mut self.last_key_down);
        let pointer_moved = self.pointer_moved_since_keyboard;
        let now = self.clock_timestamp_ms;
        button_latches[0]
            |= self
                .prepare_hotspot_keyboard(
                    context_id,
                    frame_depth,
                    direction,
                    pointer_moved,
                    now,
                );
        let absolute = (self.mouse_x_impl(), self.mouse_y_impl());
        let (origin, callbacks) = {
            let state = self.input_contexts.entry(context_id).or_default();
            (
                state.origin,
                state
                    .frames
                    .get(&frame_depth)
                    .map(|frame| frame.callbacks)
                    .unwrap_or(FrameInputCallbacks {
                        event_a: 0,
                        event_b: 0,
                        event_c: 0,
                        key_event: 0,
                    }),
            )
        };
        let relative = (
            absolute.0.saturating_sub(origin.0),
            absolute.1.saturating_sub(origin.1),
        );
        if callbacks.key_event != 0 {
            let mut mask = 0u32;
            for (bit, index) in [
                (0, INDEX_PGUP),
                (1, INDEX_PGDN),
                (2, INDEX_AUX),
                (3, INDEX_UP),
                (4, INDEX_DOWN),
            ] {
                if self.consume_snapshot_edge(index, 60) {
                    mask |= 1 << bit;
                }
            }
            if mask != 0 {
                return Some(HotspotEvent {
                    callback_hash: callbacks.key_event,
                    args: vec![mask as i32, absolute.1, absolute.0],
                });
            }
        }
        let hit = self
            .input_contexts
            .get(&context_id)
            .and_then(|state| hotspot_hit(&state.hotspots, relative));
        if button_latches[0] {
            let clicked = self
                .input_contexts
                .get(&context_id)
                .and_then(|state| {
                    state
                        .hotspots
                        .iter()
                        .enumerate()
                        .find_map(|(index, node)| {
                            (node.created_frame_depth >= frame_depth && node.state == 1)
                                .then_some(index)
                        })
                });
            if let Some(index) = clicked {
                let state = self.input_contexts.get_mut(&context_id).unwrap();
                let node = &mut state.hotspots[index];
                node.state = 2;
                let event = hotspot_node_event(node.hotspot, node.hotspot.on_click);
                self.set_logical_pointer(-10_000, -10_000);
                if event.is_some() {
                    return event;
                }
            } else if callbacks.event_a != 0 {
                self.set_logical_pointer(-10_000, -10_000);
                return Some(HotspotEvent {
                    callback_hash: callbacks.event_a,
                    args: vec![relative.1, relative.0],
                });
            }
        }
        for (latched, callback) in [
            (button_latches[1], callbacks.event_b),
            (button_latches[2], callbacks.event_c),
        ] {
            if latched && callback != 0 {
                self.set_logical_pointer(-10_000, -10_000);
                return Some(HotspotEvent {
                    callback_hash: callback,
                    args: vec![relative.1, relative.0],
                });
            }
        }
        let state = self.input_contexts.entry(context_id).or_default();
        for (index, node) in state.hotspots.iter_mut().enumerate() {
            if node.state >= 1 && Some(index) != hit {
                node.state = 0;
                if pointer_moved {
                    state.frames.entry(frame_depth).or_default().keyboard_selected = None;
                }
                return hotspot_node_event(node.hotspot, node.hotspot.on_leave);
            }
        }
        if state.hotspots.iter().any(|node| node.state > 0) {
            return None;
        }
        if let Some(index) = hit {
            let node = &mut state.hotspots[index];
            if node.created_frame_depth >= frame_depth {
                let group = node.hotspot.group;
                node.state = 1;
                let frame = state.frames.entry(frame_depth).or_default();
                frame.keyboard_selected = Some(index);
                frame.keyboard_anchor = (group != 0).then_some(index);
                return hotspot_node_event(node.hotspot, node.hotspot.on_enter);
            }
        }
        None
    }
    pub(super) fn deliver_input_impl(&mut self, input: Input) {
        vm::text_trace!(
            "[HOST] INPUT_DELIVER now={} event={input:?}", self.clock_timestamp_ms
        );
        match input {
            Input::Click { x, y } => {
                self.logical_pointer_override = None;
                self.mouse_pos = (x, y);
                self.cursor_inside = true;
                self.pointer_moved_since_keyboard = true;
                self.set_live_index(INDEX_LEFT, true);
                self.set_live_index(INDEX_LEFT, false);
            }
            Input::PointerMove { x, y } => {
                self.logical_pointer_override = None;
                if self.mouse_pos != (x, y) {
                    self.pointer_moved_since_keyboard = true;
                }
                self.mouse_pos = (x, y);
                self.cursor_inside = true;
            }
            Input::PointerButton { button, pressed, x, y } => {
                self.logical_pointer_override = None;
                self.mouse_pos = (x, y);
                self.cursor_inside = true;
                self.pointer_moved_since_keyboard = true;
                let index = match button {
                    PointerButton::Left => INDEX_LEFT,
                    PointerButton::Right => INDEX_RIGHT,
                    PointerButton::Middle => INDEX_MIDDLE,
                };
                self.set_live_index(index, pressed);
            }
            Input::Key { virtual_key, pressed } => self.deliver_key(virtual_key, pressed),
            Input::Wheel { up } => {
                self.stamp_live_edge(if up { INDEX_PGUP } else { INDEX_PGDN });
            }
            Input::Focused(focused) => {
                if !focused {
                    self.key_live_held.fill(false);
                }
            }
            Input::PointerInside(inside) => {
                self.cursor_inside = inside;
            }
            Input::Select(_) | Input::Tick | Input::Quit => {}
        }
    }
    pub(super) fn script_reset_impl(&mut self, context_id: u32, frame_depth: usize) {
        self.pointer_moved_since_keyboard = true;
        let generation = self.input_snapshot_generation;
        let state = self.input_contexts.entry(context_id).or_default();
        let old_latches = state.button_latches;
        state.hotspots.clear();
        state.origin = (0, 0);
        state.button_latches = [false; 3];
        state.latch_generation = generation;
        state.navigation_reset_ms = self.clock_timestamp_ms;
        if let Some(frame) = state.frames.get_mut(&frame_depth) {
            frame.callbacks.event_a = 0;
            frame.callbacks.event_b = 0;
            frame.callbacks.event_c = 0;
            frame.keyboard_selected = None;
            frame.keyboard_anchor = None;
        }
        vm::text_trace!(
            "[HOST] SCRIPT_RESET ctx=0x{context_id:08X} depth={frame_depth} generation={generation} old_latches={old_latches:?}"
        );
        eprintln!("[SCENE] script_reset ctx={context_id} depth={frame_depth}");
    }
    pub(super) fn snapshot_input_impl(&mut self) {
        self.key_snapshot_held = self.key_live_held;
        self.key_snapshot_pressed_ms = self.key_live_pressed_ms;
        self.input_snapshot_generation = self.input_snapshot_generation.wrapping_add(1);
        self.snapshot_button_edges = [INDEX_LEFT, INDEX_RIGHT, INDEX_MIDDLE]
            .map(|index| {
                let active = self.snapshot_edge_recent(index, 60);
                if active {
                    self.key_live_pressed_ms[index] = 0;
                }
                active
            });
        if self.snapshot_button_edges.iter().any(|edge| *edge) {
            vm::text_trace!(
                "[HOST] INPUT_SNAPSHOT generation={} edges={:?} now={}", self
                .input_snapshot_generation, self.snapshot_button_edges, self
                .clock_timestamp_ms
            );
        }
        for state in self.input_contexts.values_mut() {
            state.latch_generation = self.input_snapshot_generation;
            for (latched, edge) in state
                .button_latches
                .iter_mut()
                .zip(self.snapshot_button_edges)
            {
                *latched |= edge;
            }
        }
    }
    fn deliver_key(&mut self, virtual_key: u8, pressed: bool) {
        let was_held = self.key_live_held[virtual_key as usize];
        if pressed {
            self.last_key_down = virtual_key;
            if !native_pointer_activity_whitelisted(virtual_key) {
                self.pointer_moved_since_keyboard = false;
            }
            if virtual_key == 0x14 && !was_held {
                self.capslock_on = !self.capslock_on;
            }
        }
        if virtual_key == 0x09 {
            if pressed {
                if self.key_live_held[0x10] {
                    self.set_live_index(129, true);
                    self.set_live_index(134, true);
                } else {
                    self.set_live_index(128, true);
                    self.set_live_index(135, true);
                }
            } else {
                for index in [128, 129, 134, 135] {
                    self.set_live_index(index, false);
                }
            }
        }
        let mapping = native_key_mapping_mask(virtual_key);
        for (bit, index) in [
            (0x001, virtual_key as usize),
            (0x002, INDEX_LEFT),
            (0x004, INDEX_RIGHT),
            (0x008, INDEX_UP),
            (0x010, INDEX_DOWN),
            (0x020, 134),
            (0x040, 135),
            (0x080, INDEX_MIDDLE),
            (0x100, INDEX_AUX),
        ] {
            if mapping & bit != 0 {
                self.set_live_index(index, pressed);
            }
        }
    }
    fn set_live_index(&mut self, index: usize, pressed: bool) {
        if pressed && self.suppress_synthetic_press(index) {
            return;
        }
        self.key_live_held[index] = pressed;
        if pressed {
            self.stamp_live_edge(index);
        }
    }
    fn stamp_live_edge(&mut self, index: usize) {
        self.key_live_pressed_ms[index] = if self.clock_timestamp_ms == 0 {
            1
        } else {
            self.clock_timestamp_ms
        };
    }
    fn suppress_synthetic_press(&mut self, index: usize) -> bool {
        let bit = match index {
            INDEX_LEFT => 0,
            INDEX_RIGHT => 1,
            INDEX_MIDDLE => 2,
            _ => return false,
        };
        let now = self.clock_timestamp_ms;
        let armed = self.click_suppression_mask & (1 << bit) != 0;
        let within_window = now
            .wrapping_sub(self.click_suppression_last_ms)
            .wrapping_abs() < 350;
        self.click_suppression_last_ms = now;
        if armed && within_window {
            self.click_suppression_remaining -= 1;
            if self.click_suppression_remaining <= 0 {
                self.click_suppression_mask = 0;
            }
            true
        } else {
            self.click_suppression_mask = 0;
            false
        }
    }
    fn snapshot_edge_recent(&self, index: usize, window_ms: i32) -> bool {
        let timestamp = self.key_snapshot_pressed_ms[index];
        timestamp != 0
            && self.clock_timestamp_ms.wrapping_sub(timestamp).wrapping_abs() < window_ms
    }
    fn snapshot_index_active(&self, index: usize, window_ms: i32) -> bool {
        self.key_snapshot_held[index] || self.snapshot_edge_recent(index, window_ms)
    }
    fn consume_snapshot_edge(&mut self, index: usize, window_ms: i32) -> bool {
        let active = self.snapshot_edge_recent(index, window_ms);
        if active {
            self.key_live_pressed_ms[index] = 0;
        }
        active
    }
    fn take_context_button_latches(&mut self, context_id: u32) -> [bool; 3] {
        let generation = self.input_snapshot_generation;
        let edges = self.snapshot_button_edges;
        let state = self.input_contexts.entry(context_id).or_default();
        if state.latch_generation != generation {
            state.latch_generation = generation;
            for (latched, edge) in state.button_latches.iter_mut().zip(edges) {
                *latched |= edge;
            }
        }
        std::mem::take(&mut state.button_latches)
    }
    fn prepare_hotspot_keyboard(
        &mut self,
        context_id: u32,
        frame_depth: usize,
        key: u8,
        pointer_moved: bool,
        now: i32,
    ) -> bool {
        let gate_open = now
            .wrapping_sub(
                self.input_contexts.entry(context_id).or_default().navigation_reset_ms,
            )
            .wrapping_abs() >= 400 && !pointer_moved;
        if !gate_open {
            for node in &mut self.input_contexts.entry(context_id).or_default().hotspots
            {
                node.keyboard_marked = false;
            }
            return false;
        }
        self.restore_keyboard_selection(context_id, frame_depth);
        self.apply_hotspot_navigation(context_id, frame_depth, key, now);
        let absolute = (self.mouse_x_impl(), self.mouse_y_impl());
        let group = if key != 0 {
            i32::from(key)
        } else {
            let state = self.input_contexts.entry(context_id).or_default();
            let relative = (
                absolute.0.saturating_sub(state.origin.0),
                absolute.1.saturating_sub(state.origin.1),
            );
            let mut group = 0;
            for node in &mut state.hotspots {
                if !node.keyboard_marked {
                    continue;
                }
                if hotspot_contains(node.hotspot, relative) {
                    group = node.hotspot.group;
                } else {
                    node.keyboard_marked = false;
                }
            }
            group
        };
        if group == 0 {
            return false;
        }
        let (target, forced_click) = {
            let state = self.input_contexts.entry(context_id).or_default();
            let mut target = None;
            let mut forced_click = false;
            for (index, node) in state.hotspots.iter_mut().enumerate() {
                if node.hotspot.group == group {
                    target = Some(index);
                    forced_click |= node.state != 0;
                    node.keyboard_marked = true;
                }
            }
            if let Some(target) = target {
                let frame = state.frames.entry(frame_depth).or_default();
                frame.keyboard_selected = Some(target);
                frame.keyboard_anchor = None;
            }
            (target, forced_click)
        };
        if let Some(index) = target {
            let hotspot = self.input_contexts[&context_id].hotspots[index].hotspot;
            self.set_logical_pointer(
                hotspot.x + hotspot.width / 2,
                hotspot.y + hotspot.height / 2,
            );
        }
        forced_click
    }
    fn restore_keyboard_selection(&mut self, context_id: u32, frame_depth: usize) {
        let (selected, logical) = {
            let state = self.input_contexts.entry(context_id).or_default();
            let frame = state.frames.entry(frame_depth).or_default();
            let selected = frame
                .keyboard_selected
                .or(frame.keyboard_anchor)
                .or_else(|| {
                    state.hotspots.iter().position(|node| node.hotspot.group != 0)
                });
            frame.keyboard_selected = selected;
            if frame.keyboard_anchor.is_none() {
                frame.keyboard_anchor = selected;
            }
            let logical = selected
                .map(|index| {
                    let hotspot = state.hotspots[index].hotspot;
                    (
                        hotspot.x + state.origin.0 + hotspot.width / 2,
                        hotspot.y + state.origin.1 + hotspot.height / 2,
                    )
                });
            (selected, logical)
        };
        if selected.is_some() {
            let logical = logical.unwrap();
            self.set_logical_pointer(logical.0, logical.1);
        }
    }
    fn apply_hotspot_navigation(
        &mut self,
        context_id: u32,
        frame_depth: usize,
        direction: u8,
        now: i32,
    ) {
        if !matches!(direction, 0x25..= 0x28 | 0x62 | 0x64 | 0x66 | 0x68) {
            return;
        }
        let state = self.input_contexts.entry(context_id).or_default();
        if now.wrapping_sub(state.navigation_reset_ms).wrapping_abs() < 400 {
            return;
        }
        let frame = state.frames.entry(frame_depth).or_default();
        let current = frame
            .keyboard_selected
            .or(frame.keyboard_anchor)
            .or_else(|| {
                state.hotspots.iter().position(|node| node.hotspot.group != 0)
            });
        let Some(current) = current else {
            return;
        };
        let current_hotspot = state.hotspots[current].hotspot;
        let candidate = navigation_candidate(
            &state.hotspots,
            current_hotspot,
            direction,
        );
        let Some(candidate) = candidate else {
            return;
        };
        let frame = state.frames.entry(frame_depth).or_default();
        frame.keyboard_selected = Some(candidate);
        frame.keyboard_anchor = Some(candidate);
        let hotspot = state.hotspots[candidate].hotspot;
        let logical = (
            hotspot.x + state.origin.0 + hotspot.width / 2,
            hotspot.y + state.origin.1 + hotspot.height / 2,
        );
        self.set_logical_pointer(logical.0, logical.1);
    }
}
pub(super) fn native_key_mapping_mask(virtual_key: u8) -> u16 {
    match virtual_key {
        0x0D | 0x20 | 0x58 => 0x003,
        0x10 | 0x11 | 0x14 => 0x081,
        0x1B | 0x23 | 0x2D | 0x2E | 0x5A | 0x60 => 0x005,
        0x25 | 0x66 => 0x041,
        0x26 | 0x68 => 0x009,
        0x27 | 0x64 => 0x021,
        0x28 | 0x62 => 0x011,
        0x05
        | 0x09
        | 0x0C
        | 0x17
        | 0x21
        | 0x22
        | 0x24
        | 0x2F
        | 0x30..=0x38
        | 0x40..=0x57
        | 0x59
        | 0x61
        | 0x63
        | 0x65
        | 0x67
        | 0x69..=0x6F
        | 0x90
        | 0x91 => 0x001,
        _ => 0,
    }
}
pub(super) fn native_pointer_activity_whitelisted(virtual_key: u8) -> bool {
    matches!(virtual_key, 0x10 | 0x11 | 0x20 | 0x70..= 0x87)
}
fn hotspot_hit(nodes: &[HotspotRuntime], point: (i32, i32)) -> Option<usize> {
    nodes.iter().position(|node| hotspot_contains(node.hotspot, point))
}
fn hotspot_contains(hotspot: Hotspot, point: (i32, i32)) -> bool {
    let x = i64::from(point.0);
    let y = i64::from(point.1);
    let left = i64::from(hotspot.x);
    let top = i64::from(hotspot.y);
    let right = left + i64::from(hotspot.width.max(0));
    let bottom = top + i64::from(hotspot.height.max(0));
    x >= left && x < right && y >= top && y < bottom
}
fn hotspot_node_event(hotspot: Hotspot, callback_hash: u32) -> Option<HotspotEvent> {
    (callback_hash != 0)
        .then(|| HotspotEvent {
            callback_hash,
            args: vec![hotspot.height, hotspot.width, hotspot.y, hotspot.x, hotspot.id,],
        })
}
fn navigation_candidate(
    nodes: &[HotspotRuntime],
    current: Hotspot,
    direction: u8,
) -> Option<usize> {
    let horizontal = matches!(direction, 0x25 | 0x27 | 0x64 | 0x66);
    let negative = matches!(direction, 0x25 | 0x26 | 0x64 | 0x68);
    let current_primary = if horizontal { current.x } else { current.y };
    let current_cross = if horizontal { current.y } else { current.x };
    let current_cross_size = if horizontal { current.height } else { current.width };
    let choose = |require_overlap: bool| {
        nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| node.hotspot.group != 0 && node.hotspot.id != current.id)
            .filter_map(|(index, node)| {
                let candidate = node.hotspot;
                let primary = if horizontal { candidate.x } else { candidate.y };
                let distance = if negative {
                    current_primary - primary
                } else {
                    primary - current_primary
                };
                if distance <= 0 || distance >= 9_999
                    || (!require_overlap && distance <= 16)
                {
                    return None;
                }
                if require_overlap {
                    let cross = if horizontal { candidate.y } else { candidate.x };
                    let cross_size = if horizontal {
                        candidate.height
                    } else {
                        candidate.width
                    };
                    let overlap = (current_cross + current_cross_size)
                        .min(cross + cross_size) - current_cross.max(cross);
                    if overlap <= current_cross_size / 2 && overlap <= cross_size / 2 {
                        return None;
                    }
                }
                Some((distance, index))
            })
            .min_by_key(|(distance, _)| *distance)
            .map(|(_, index)| index)
    };
    choose(true).or_else(|| choose(false))
}
