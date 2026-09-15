use super::*;
impl EngineHost {
    pub(super) fn get_timestamp_impl(&mut self) -> i32 {
        self.clock_timestamp_ms
    }
    pub(super) fn begin_scheduler_tick_impl(&mut self) {
        let real = self.clock_start.elapsed().as_millis() as u32;
        self.clock_timestamp_ms = if self.clock_anchor_real_ms != 0 {
            let elapsed = real.wrapping_sub(self.clock_anchor_real_ms);
            self
                .clock_anchor_real_ms
                .wrapping_add(self.clock_offset_ms as u32)
                .wrapping_add((elapsed as f64 * self.clock_scale) as u32) as i32
        } else {
            real.wrapping_add(self.clock_offset_ms as u32) as i32
        };
        self.audio.refresh_default_output(&self.vfs);
        self.audio.tick(self.clock_timestamp_ms);
        self.snapshot_input_impl();
    }
    pub(super) fn set_native_time_scale_impl(&mut self, scale: f32) {
        let real = self.clock_start.elapsed().as_millis() as u32;
        if scale == 1.0 {
            self.clock_offset_ms = (self.clock_timestamp_ms as u32).wrapping_sub(real)
                as i32;
            self.clock_anchor_real_ms = 0;
        } else {
            self.clock_anchor_real_ms = if real == 0 { 1 } else { real };
        }
        self.clock_scale = scale as f64;
    }
    pub(super) fn cooperative_timers_impl(&self) -> bool {
        true
    }
    pub(super) fn normalize_scheduler_delta_impl(&mut self) {
        let real = self.clock_start.elapsed().as_millis() as u32;
        let virtual_now = if self.clock_anchor_real_ms != 0 {
            let elapsed = real.wrapping_sub(self.clock_anchor_real_ms);
            self
                .clock_anchor_real_ms
                .wrapping_add(self.clock_offset_ms as u32)
                .wrapping_add((elapsed as f64 * self.clock_scale) as u32) as i32
        } else {
            real.wrapping_add(self.clock_offset_ms as u32) as i32
        };
        let delta = virtual_now.wrapping_sub(self.clock_timestamp_ms);
        if delta < 0 {
            self.clock_offset_ms = self.clock_offset_ms.wrapping_sub(delta);
        }
    }
}
