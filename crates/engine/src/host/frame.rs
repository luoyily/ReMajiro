use super::*;
impl EngineHost {
    pub(super) fn present_epoch_impl(&self) -> u64 {
        self.present_epoch
    }
    pub(super) fn request_present_impl(&mut self) {
        self.render_dirty = true;
    }
    pub(super) fn scene_freeze_begin_impl(&mut self) {
        if vm::frame_probe_enabled() {
            vm::text_trace!(
                "[FRAME-PROBE] HOST_FREEZE_BEGIN depth={} next={}", self
                .scene_dirty_freeze, self.scene_dirty_freeze.wrapping_add(1)
            );
        }
        self.push_frame_mark(
            "freeze_begin",
            format!("depth={}", self.scene_dirty_freeze),
        );
        self.scene_dirty_freeze = self.scene_dirty_freeze.wrapping_add(1);
    }
    pub(super) fn scene_freeze_end_impl(&mut self) {
        if vm::frame_probe_enabled() {
            vm::text_trace!(
                "[FRAME-PROBE] HOST_FREEZE_END depth={} next={}", self
                .scene_dirty_freeze, self.scene_dirty_freeze.wrapping_sub(1)
            );
        }
        self.scene_dirty_freeze = self.scene_dirty_freeze.wrapping_sub(1);
        self.push_frame_mark("freeze_end", format!("depth={}", self.scene_dirty_freeze));
    }
    pub(super) fn invalidate_page_impl(
        &mut self,
        page: u32,
        _rect: Option<(i32, i32, i32, i32)>,
    ) {
        let page = base_page_handle(page);
        if self.pages.contains_key(&page) {
            self.render_dirty = true;
        }
    }
    pub(super) fn display_sync_impl(&mut self, mode: i32) -> i32 {
        self.render_dirty = true;
        mode
    }
}
