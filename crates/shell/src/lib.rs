use engine::profile::GameProfile;
use engine::{EngineBoot, EngineHost, Renderer, RuntimeCpuTimings};
use formats::crypto::crc32;
use vm::host::Host;
use vm::{SchedSignal, Vm};
macro_rules! eprintln {
    ($($arg:tt)*) => {
        engine::diag::emit_log(format_args!($($arg)*))
    };
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootPhase {
    Init,
    HotReset,
    Game,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    Poll,
    Wait,
    WaitUntilMicros(u64),
}
#[derive(Debug)]
pub struct FrameOutcome {
    pub present_submitted: bool,
    pub exit: bool,
    pub flow: Flow,
    pub cursor_warp: Option<(i32, i32)>,
    pub cursor_visible: Option<bool>,
    pub fullscreen: Option<bool>,
}
pub struct GameSession {
    vm: Box<Vm>,
    host: Box<EngineHost>,
    renderer: Box<Renderer>,
    profile: &'static GameProfile,
    step_budget: usize,
    boot_phase: BootPhase,
    start_script_idx: usize,
    start_main_offset: usize,
    diag_ticks: u64,
    post_load_refresh_pending: bool,
    loop_diag: LoopDiagnostics,
}
impl GameSession {
    pub fn bootstrap(
        mut host: EngineHost,
        renderer: Renderer,
        profile: &'static GameProfile,
        step_budget: usize,
    ) -> Self {
        let mut vm = Vm::new();
        EngineBoot::init(&mut vm, Some(host.save_store().as_ref()));
        if sync_title_extra_unlock(&mut vm, profile) {
            eprintln!("[BOOT] restored persistent title Extra unlock state");
        }
        for preload in profile.preload_scripts {
            let loaded = host
                .load_script(preload)
                .unwrap_or_else(|| {
                    panic!("could not load {preload}.mjo from loose files or ARC")
                });
            let idx = vm.load_resolved_script(loaded);
            eprintln!("[BOOT] preloaded {} → script_idx={}", preload, idx);
        }
        let start = host
            .load_script(profile.entry_script)
            .unwrap_or_else(|| {
                panic!(
                    "could not load {}.mjo from loose files or ARC", profile.entry_script
                )
            });
        let init_hash = crc32(profile.entry_label.as_bytes());
        let init_offset = start
            .entries
            .iter()
            .find(|(name_hash, _)| *name_hash == init_hash)
            .map(|(_, offset)| *offset as usize)
            .unwrap_or(0);
        let start_main_offset = start.main_offset as usize;
        let start_idx = vm.load_resolved_script(start);
        eprintln!(
            "[BOOT] {}.mjo {} at offset 0x{:X}", profile.entry_script, profile
            .entry_label, init_offset
        );
        vm.push_frame(start_idx, init_offset, profile.entry_label)
            .expect("push start frame");
        vm.set_schedule_wall_slice(Some(std::time::Duration::from_millis(8)));
        eprintln!("[BOOT] start.mjo main entry at offset 0x{:X}", start_main_offset);
        Self {
            vm: Box::new(vm),
            host: Box::new(host),
            renderer: Box::new(renderer),
            profile,
            step_budget,
            boot_phase: BootPhase::Init,
            start_script_idx: start_idx,
            start_main_offset,
            diag_ticks: 0,
            post_load_refresh_pending: false,
            loop_diag: LoopDiagnostics::new(),
        }
    }
    pub fn on_input(&mut self, input: vm::host::Input) {
        self.vm.resume_with_input(input, self.host.as_mut());
    }
    pub fn pointer_client_position(&self) -> (i32, i32) {
        self.host.pointer_client_position()
    }
    pub fn on_frame(&mut self) -> FrameOutcome {
        let budget = self.step_budget;
        let phase = self.boot_phase;
        let start_idx = self.start_script_idx;
        let main_off = self.start_main_offset;
        let mut next_flow = Flow::Wait;
        let mut publish_frame = true;
        let mut consume_scheduler_input_boundary = false;
        let mut runtime_cpu = RuntimeCpuTimings::default();
        let post_load_refresh_due = self.post_load_refresh_pending;
        self.loop_diag.about_to_wait = self.loop_diag.about_to_wait.wrapping_add(1);
        const MAX_NO_PRESENT_DRAIN_PASSES: usize = 4096;
        let mut no_present_drain_passes = 0usize;
        let mut no_present_input_quiescent = false;
        let schedule_signal = loop {
            let schedule_started = engine::Instant::now();
            let signal = self.vm.schedule(self.host.as_mut(), budget);
            let schedule_us = schedule_started.elapsed().as_micros();
            runtime_cpu.schedule_us = runtime_cpu.schedule_us.wrapping_add(schedule_us);
            self.loop_diag.schedule_us = self
                .loop_diag
                .schedule_us
                .wrapping_add(schedule_us);
            if vm::frame_probe_enabled() {
                vm::text_trace!(
                    "[FRAME-PROBE] TICK tick={} drain={} signal={signal:?} vm={} host={}",
                    self.loop_diag.about_to_wait, no_present_drain_passes, self.vm
                    .frame_probe_summary(), self.host.frame_probe_summary()
                );
            }
            if !matches!(& signal, Ok(SchedSignal::WaitingNoPresent)) {
                break signal;
            }
            self.loop_diag.signals[1] = self.loop_diag.signals[1].wrapping_add(1);
            no_present_drain_passes += 1;
            if no_present_drain_passes >= MAX_NO_PRESENT_DRAIN_PASSES {
                if engine::diag_log_enabled() {
                    eprintln!(
                        "[SCHED] no-present drain guard reached after {} passes",
                        no_present_drain_passes
                    );
                }
                break signal;
            }
            if !no_present_input_quiescent {
                no_present_input_quiescent = !self
                    .vm
                    .consume_scheduler_input_boundary(self.host.as_mut());
            }
        };
        match schedule_signal {
            Ok(SchedSignal::Waiting) => {
                self.loop_diag.signals[0] = self.loop_diag.signals[0].wrapping_add(1);
                consume_scheduler_input_boundary = true;
                self.diag_ticks = self.diag_ticks.wrapping_add(1);
                if engine::diag_log_enabled() && self.diag_ticks.is_multiple_of(300) {
                    eprintln!(
                        "[SCHED] phase={:?} ctx=0x{:08X} script={:?} ip={:?} depth={} label={:?}",
                        phase, self.vm.current_function_id(), self.vm
                        .current_script_idx(), self.vm.current_ip(), self.vm
                        .frame_depth(), self.vm.current_label()
                    );
                }
                next_flow = match self.vm.next_wake_delay_ms(self.host.get_timestamp()) {
                    Some(delay_ms) => {
                        Flow::WaitUntilMicros(delay_ms.saturating_mul(1_000))
                    }
                    None => Flow::Wait,
                };
            }
            Ok(SchedSignal::WaitingNoPresent) => {
                consume_scheduler_input_boundary = true;
                publish_frame = false;
                next_flow = Flow::Poll;
            }
            Ok(SchedSignal::LoadRequested) => {
                self.loop_diag.signals[5] = self.loop_diag.signals[5].wrapping_add(1);
                publish_frame = false;
                match self.vm.apply_pending_save_restore(self.host.as_mut()) {
                    Ok(true) => {
                        eprintln!("[SAVE] restore boundary complete");
                        self.post_load_refresh_pending = true;
                        next_flow = Flow::Poll;
                    }
                    Ok(false) => {
                        eprintln!("[SAVE] restore boundary had no pending slot");
                        next_flow = Flow::Poll;
                    }
                    Err(error) => {
                        eprintln!("[SAVE] restore failed: {}", error);
                        next_flow = Flow::Wait;
                    }
                }
            }
            Ok(SchedSignal::QuitRequested) => {
                self.loop_diag.signals[6] = self.loop_diag.signals[6].wrapping_add(1);
                let _ = self.host.take_exit_requested();
                let persisted = self.vm.persist_system_state(self.host.as_mut());
                if !persisted {
                    eprintln!("[SAVE] failed to persist system state on scripted exit");
                }
                return FrameOutcome {
                    present_submitted: false,
                    exit: true,
                    flow: Flow::Wait,
                    cursor_warp: None,
                    cursor_visible: None,
                    fullscreen: None,
                };
            }
            Ok(SchedSignal::BudgetExhausted) => {
                self.loop_diag.signals[2] = self.loop_diag.signals[2].wrapping_add(1);
                publish_frame = false;
                eprintln!(
                    "[SCHED] budget exhausted (safety valve) — this indicates a missed wait boundary"
                );
                next_flow = Flow::Poll;
            }
            Ok(SchedSignal::TimeSliced) => {
                self.loop_diag.signals[7] = self.loop_diag.signals[7].wrapping_add(1);
                consume_scheduler_input_boundary = true;
                next_flow = Flow::Poll;
            }
            Ok(SchedSignal::RootExited) => {
                self.loop_diag.signals[3] = self.loop_diag.signals[3].wrapping_add(1);
                eprintln!("[VM] root frame exited (phase={:?})", phase);
                if phase == BootPhase::Init {
                    self.host.pin_scene_resources();
                    self.vm.pin_host_funcs();
                    match self
                        .vm
                        .start_scene(start_idx, main_off, "$main@GLOBAL")
                        .and_then(|()| invoke_hot_reset(
                            self.vm.as_mut(),
                            self.profile,
                            0,
                            0,
                        ))
                    {
                        Ok(n) => {
                            self.boot_phase = BootPhase::Game;
                            eprintln!(
                                "[BOOT] init complete → main entry pushed at 0x{:X}; HOT_RESET scheduled {} callbacks",
                                main_off, n
                            );
                        }
                        Err(e) => {
                            eprintln!(
                                "[BOOT] main push / HOT_RESET bridge failed: {}", e
                            )
                        }
                    }
                    next_flow = Flow::Poll;
                } else if phase == BootPhase::HotReset {
                    eprintln!(
                        "[BOOT] HOT_RESET root callbacks complete; draining spawned contexts"
                    );
                    next_flow = Flow::Poll;
                } else {
                    vm::text_trace!(
                        "[SCENE-CYCLE] RootExited in Game phase -> begin_outer_scene_cycle"
                    );
                    match begin_outer_scene_cycle(
                        self.vm.as_mut(),
                        self.host.as_mut(),
                        self.profile,
                        start_idx,
                        main_off,
                    ) {
                        Ok((retired, scheduled)) => {
                            self.boot_phase = BootPhase::Game;
                            eprintln!(
                                "[BOOT] root scene cycle -> HOT_RESET scheduled {} callbacks; retired {} transient callbacks; main entry re-pushed at 0x{:X}",
                                scheduled, retired, main_off
                            );
                            next_flow = Flow::Poll;
                        }
                        Err(e) => {
                            eprintln!("[BOOT] outer scene-cycle reset failed: {}", e);
                            next_flow = Flow::Wait;
                        }
                    }
                }
            }
            Ok(SchedSignal::Terminated) => {
                self.loop_diag.signals[4] = self.loop_diag.signals[4].wrapping_add(1);
                if phase == BootPhase::HotReset {
                    eprintln!(
                        "[BOOT] HOT_RESET complete → starting start.mjo main sequence"
                    );
                    match self.vm.start_scene(start_idx, main_off, "$main@GLOBAL") {
                        Ok(()) => {
                            self.boot_phase = BootPhase::Game;
                            vm::text_trace!(
                                "[SCENE-CYCLE] start.mjo $main re-pushed at offset 0x{:X}",
                                main_off
                            );
                            eprintln!(
                                "[BOOT] main entry pushed at offset 0x{:X}", main_off
                            );
                        }
                        Err(e) => eprintln!("[BOOT] failed to push main entry: {}", e),
                    }
                    next_flow = Flow::Poll;
                } else if phase == BootPhase::Game {
                    vm::text_trace!(
                        "[SCENE-CYCLE] Terminated in Game phase -> begin_outer_scene_cycle"
                    );
                    match begin_outer_scene_cycle(
                        self.vm.as_mut(),
                        self.host.as_mut(),
                        self.profile,
                        start_idx,
                        main_off,
                    ) {
                        Ok((retired, scheduled)) => {
                            self.boot_phase = BootPhase::Game;
                            eprintln!(
                                "[BOOT] all contexts retired -> HOT_RESET scheduled {} callbacks; retired {} transient callbacks; main entry re-pushed at 0x{:X}",
                                scheduled, retired, main_off
                            );
                            next_flow = Flow::Poll;
                        }
                        Err(e) => {
                            eprintln!("[BOOT] outer scene-cycle reset failed: {}", e);
                            next_flow = Flow::Wait;
                        }
                    }
                } else {
                    eprintln!("[VM] all contexts terminated (phase={:?})", phase);
                }
            }
            Err(e) => {
                eprintln!("[VM] error: {}", e);
                publish_frame = false;
            }
        }
        if post_load_refresh_due {
            self.host.save_refresh_after_restore();
            publish_frame = true;
        }
        let mut outcome = FrameOutcome {
            present_submitted: false,
            exit: false,
            flow: next_flow,
            cursor_warp: None,
            cursor_visible: None,
            fullscreen: None,
        };
        if publish_frame {
            let host_frame_started = engine::Instant::now();
            let frame = self.host.take_render_frame();
            runtime_cpu.host_frame_us = host_frame_started.elapsed().as_micros();
            self.loop_diag.host_frame_us = self
                .loop_diag
                .host_frame_us
                .wrapping_add(runtime_cpu.host_frame_us);
            if let Some(frame) = frame {
                if vm::frame_probe_enabled() {
                    let fullscreen = frame
                        .quads
                        .iter()
                        .enumerate()
                        .filter(|(_, quad)| quad.width >= 1000.0 && quad.height >= 560.0)
                        .map(|(index, quad)| {
                            format!(
                                "{index}:p{}:a{:.3}:xy({:.0},{:.0}):wh({:.0},{:.0}):src({:.0},{:.0},{:.0},{:.0}):m{}",
                                quad.page, quad.alpha, quad.x, quad.y, quad.width, quad
                                .height, quad.source_x, quad.source_y, quad.source_width,
                                quad.source_height, quad.draw_mode
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("|");
                    let dirty_pages = frame
                        .pages
                        .iter()
                        .map(|page| {
                            if page.width >= 1000 && page.height >= 560 {
                                let mut min_alpha = u8::MAX;
                                let mut max_alpha = u8::MIN;
                                let mut visible = 0usize;
                                let mut partial = 0usize;
                                let mut black = 0usize;
                                let mut dark_bands = [0usize; 4];
                                let mut rgb_hash = 0xcbf29ce484222325u64;
                                for (pixel_index, pixel) in page
                                    .pixels
                                    .chunks_exact(4)
                                    .enumerate()
                                {
                                    let alpha = pixel[3];
                                    min_alpha = min_alpha.min(alpha);
                                    max_alpha = max_alpha.max(alpha);
                                    visible += usize::from(alpha != 0);
                                    partial += usize::from(alpha != 0 && alpha != 255);
                                    black
                                        += usize::from(
                                            pixel[0] == 0 && pixel[1] == 0 && pixel[2] == 0,
                                        );
                                    let dark = u16::from(pixel[0]) + u16::from(pixel[1])
                                        + u16::from(pixel[2]) < 48;
                                    if dark {
                                        let row = pixel_index / page.width as usize;
                                        let band = (row * dark_bands.len() / page.height as usize)
                                            .min(dark_bands.len() - 1);
                                        dark_bands[band] += 1;
                                    }
                                    for channel in &pixel[..3] {
                                        rgb_hash ^= u64::from(*channel);
                                        rgb_hash = rgb_hash.wrapping_mul(0x100000001b3);
                                    }
                                }
                                format!(
                                    "{}@{}:a{}-{}:visible{}:partial{}:black{}:dark{:?}:rgb{:016x}",
                                    page.handle, page.revision, min_alpha, max_alpha, visible,
                                    partial, black, dark_bands, rgb_hash
                                )
                            } else {
                                format!("{}@{}", page.handle, page.revision)
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(",");
                    vm::text_trace!(
                        "[FRAME-PROBE] SUBMIT tick={} display={:?} epoch={} quads={} fullscreen=[{}] dirty_pages=[{}] released={:?}",
                        self.loop_diag.about_to_wait, frame.display_page, frame
                        .display_epoch, frame.quads.len(), fullscreen, dirty_pages, frame
                        .released_pages
                    );
                }
                self.loop_diag.submits = self.loop_diag.submits.wrapping_add(1);
                let submit_started = engine::Instant::now();
                self.renderer.record_runtime_cpu(runtime_cpu);
                self.renderer.submit_frame(frame);
                if post_load_refresh_due {
                    self.post_load_refresh_pending = false;
                    eprintln!("[SAVE] post-restore display refresh submitted");
                }
                self.loop_diag.submit_us = self
                    .loop_diag
                    .submit_us
                    .wrapping_add(submit_started.elapsed().as_micros());
                outcome.present_submitted = true;
            }
        }
        if consume_scheduler_input_boundary
            && self.vm.consume_scheduler_input_boundary(self.host.as_mut())
        {
            outcome.flow = Flow::Poll;
        }
        outcome.cursor_warp = self.host.take_cursor_warp();
        outcome.cursor_visible = self.host.take_cursor_visibility();
        if let Some(fullscreen) = self.host.take_fullscreen_request() {
            outcome.fullscreen = Some(fullscreen);
            outcome.flow = Flow::Poll;
        }
        if self.host.has_active_animations() {
            outcome.flow = match outcome.flow {
                Flow::Poll => Flow::Poll,
                Flow::WaitUntilMicros(deadline_us) if deadline_us < 16_667 => {
                    Flow::WaitUntilMicros(deadline_us)
                }
                _ => Flow::WaitUntilMicros(16_667),
            };
        }
        if self.vm.native_fast_forward_active() {
            outcome.flow = Flow::Poll;
        }
        self.loop_diag.report_if_due();
        outcome
    }
    pub fn draw(&mut self) {
        self.loop_diag.redraws = self.loop_diag.redraws.wrapping_add(1);
        let draw_started = engine::Instant::now();
        self.renderer.draw_frame();
        self.loop_diag.draw_us = self
            .loop_diag
            .draw_us
            .wrapping_add(draw_started.elapsed().as_micros());
    }
    pub fn resize(&mut self, width: u32, height: u32) {
        self.renderer.resize(winit::dpi::PhysicalSize::new(width, height));
    }
    pub fn persist_and_quit(&mut self) -> bool {
        self.vm.persist_system_state(self.host.as_mut())
    }
}
fn sync_title_extra_unlock(vm: &mut Vm, profile: &GameProfile) -> bool {
    let Some(flags) = profile.gameplay_flags else {
        return false;
    };
    let already_shown = vm
        .system_global_value(flags.title_extra_shown_flag)
        .and_then(vm::value::Value::as_int)
        .unwrap_or(0) != 0;
    let unlocked = flags
        .ending_unlock_flags
        .iter()
        .any(|key| {
            vm.system_global_value(*key).and_then(vm::value::Value::as_int).unwrap_or(0)
                != 0
        });
    if already_shown || !unlocked {
        return false;
    }
    vm.set_system_global_int(flags.title_extra_shown_flag, 1);
    true
}
fn invoke_hot_reset(
    vm: &mut Vm,
    profile: &GameProfile,
    first_render: i32,
    loaded: i32,
) -> Result<usize, vm::VmError> {
    let args = [
        vm::value::Value::int(first_render),
        vm::value::Value::string(Vec::new()),
        vm::value::Value::int(loaded),
    ];
    vm.invoke_host_func(profile.hot_reset_hash, &args, false)
}
fn begin_outer_scene_cycle(
    vm: &mut Vm,
    host: &mut EngineHost,
    profile: &GameProfile,
    start_idx: usize,
    main_off: usize,
) -> Result<(usize, usize), vm::VmError> {
    let preserve_audio = vm.take_scene_cycle_preserve_audio();
    if preserve_audio {
        host.reset_for_outer_scene_cycle_preserve_audio();
    } else {
        host.reset_for_outer_scene_cycle();
    }
    let retired_callbacks = vm.reset_for_outer_scene_cycle();
    if sync_title_extra_unlock(vm, profile) {
        eprintln!("[BOOT] synchronized persistent title Extra unlock state");
    }
    vm.start_scene(start_idx, main_off, "$main@GLOBAL")?;
    let scheduled_callbacks = invoke_hot_reset(vm, profile, 0, 0)?;
    Ok((retired_callbacks, scheduled_callbacks))
}
struct LoopDiagnostics {
    started: engine::Instant,
    about_to_wait: u64,
    redraws: u64,
    submits: u64,
    signals: [u64; 8],
    schedule_us: u128,
    host_frame_us: u128,
    submit_us: u128,
    draw_us: u128,
}
impl LoopDiagnostics {
    fn new() -> Self {
        Self {
            started: engine::Instant::now(),
            about_to_wait: 0,
            redraws: 0,
            submits: 0,
            signals: [0; 8],
            schedule_us: 0,
            host_frame_us: 0,
            submit_us: 0,
            draw_us: 0,
        }
    }
    fn report_if_due(&mut self) {
        let elapsed = self.started.elapsed();
        if elapsed < std::time::Duration::from_secs(1) {
            return;
        }
        if engine::diag::env_knob_bool("OSTB_LOOP_DIAG", false) {
            eprintln!(
                "[LOOP-PERF] ms={} about={} redraw={} submit={} waiting={} no_present={} budget={} root={} terminated={} load={} quit={} schedule_us={} host_us={} submit_us={} draw_us={}",
                elapsed.as_millis(), self.about_to_wait, self.redraws, self.submits, self
                .signals[0], self.signals[1], self.signals[2], self.signals[3], self
                .signals[4], self.signals[5], self.signals[6], self.schedule_us, self
                .host_frame_us, self.submit_us, self.draw_us,
            );
        }
        *self = Self::new();
    }
}
pub fn validate_window_size(
    width: u32,
    height: u32,
    internal_w: u32,
    internal_h: u32,
) -> Result<(), String> {
    if width == 0 || height == 0 {
        return Err(format!("window dimensions must be non-zero, got {width}x{height}"));
    }
    if u64::from(width) * u64::from(internal_h)
        != u64::from(height) * u64::from(internal_w)
    {
        return Err(
            format!(
                "{width}x{height} is not proportional to the {internal_w}x{internal_h} logical resolution"
            ),
        );
    }
    Ok(())
}
pub fn window_to_logical(
    position: (f64, f64),
    size: (u32, u32),
    internal_w: u32,
    internal_h: u32,
) -> (i32, i32) {
    let width = size.0.max(1) as f64;
    let height = size.1.max(1) as f64;
    let x = (position.0 * f64::from(internal_w) / width).floor() as i32;
    let y = (position.1 * f64::from(internal_h) / height).floor() as i32;
    (x.clamp(0, internal_w as i32 - 1), y.clamp(0, internal_h as i32 - 1))
}
pub fn logical_to_window(
    position: (i32, i32),
    size: (u32, u32),
    internal_w: u32,
    internal_h: u32,
) -> (f64, f64) {
    let width = f64::from(size.0.max(1));
    let height = f64::from(size.1.max(1));
    (
        f64::from(position.0) * width / f64::from(internal_w),
        f64::from(position.1) * height / f64::from(internal_h),
    )
}

