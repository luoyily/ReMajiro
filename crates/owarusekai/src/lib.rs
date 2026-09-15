#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
use std::path::{Path, PathBuf};
use std::sync::Arc;
use clap::{Parser, ValueEnum};
use engine::{
    EngineHost, PatchBundle, RenderProfileConfig, RenderProfileMode, Renderer, Vfs,
};
use shell::{Flow, GameSession};
use winit::application::ApplicationHandler;
use winit::event::{MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Fullscreen, Window, WindowAttributes, WindowId};
pub fn launch(profile: &'static engine::profile::GameProfile) {
    platform_desktop::install();
    engine::platform_services::install_app_name(profile.bin_name);
    platform_desktop::shell::install_panic_reporting();
    let cli = match CliArgs::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            let exit_code = error.exit_code();
            if error.use_stderr() {
                platform_desktop::shell::report_startup_error(&error.to_string());
            } else {
                platform_desktop::shell::report_cli_output(&error.to_string());
            }
            std::process::exit(exit_code);
        }
    };
    let args = match cli.resolve(profile) {
        Ok(args) => args,
        Err(error) => {
            platform_desktop::shell::report_startup_error(&error);
            std::process::exit(1);
        }
    };
    if let Err(error) = run(args, profile) {
        platform_desktop::shell::report_startup_error(&error.to_string());
        std::process::exit(1);
    }
}
#[derive(Parser, Debug)]
#[command(name = "game", about = "Majiro engine port – VM-driven rendering")]
struct CliArgs {
    #[arg(long)]
    asset_root: Option<PathBuf>,
    #[arg(long)]
    arc_dir: Option<PathBuf>,
    #[arg(long)]
    save_dir: Option<PathBuf>,
    #[arg(long)]
    font: Option<PathBuf>,
    #[arg(long)]
    patch: Option<PathBuf>,
    #[arg(long)]
    ir_dir: Option<PathBuf>,
    #[arg(long, value_enum)]
    render_profile: Option<RenderProfileArg>,
    #[arg(long)]
    render_profile_output: Option<PathBuf>,
    #[arg(long, default_value = "1000000")]
    step_budget: usize,
    #[arg(long)]
    width: Option<u32>,
    #[arg(long)]
    height: Option<u32>,
}
#[derive(Debug)]
struct Args {
    asset_root: PathBuf,
    arc_dir: PathBuf,
    save_dir: PathBuf,
    font: Option<PathBuf>,
    patch: Option<PathBuf>,
    ir_dir: Option<PathBuf>,
    render_profile: Option<RenderProfileArg>,
    render_profile_output: PathBuf,
    step_budget: usize,
    width: u32,
    height: u32,
}
impl CliArgs {
    fn resolve(self, profile: &engine::profile::GameProfile) -> Result<Args, String> {
        let width = self.width.unwrap_or(profile.internal_w);
        let height = self.height.unwrap_or(profile.internal_h);
        shell::validate_window_size(
            width,
            height,
            profile.internal_w,
            profile.internal_h,
        )?;
        let executable = std::env::current_exe()
            .map_err(|error| {
                format!("failed to locate the running executable: {error}")
            })?;
        let executable_dir = executable
            .parent()
            .ok_or_else(|| {
                format!(
                    "running executable has no parent directory: {}", executable
                    .display()
                )
            })?
            .to_path_buf();
        let patch = self
            .patch
            .or_else(|| {
                let automatic = executable_dir.join("patch");
                automatic.join("patch.toml").is_file().then_some(automatic)
            });
        Ok(Args {
            asset_root: self.asset_root.unwrap_or_else(|| executable_dir.clone()),
            arc_dir: self.arc_dir.unwrap_or_else(|| executable_dir.clone()),
            save_dir: self.save_dir.unwrap_or_else(|| executable_dir.join("savedata")),
            font: self.font,
            patch,
            ir_dir: self.ir_dir,
            render_profile: self.render_profile,
            render_profile_output: self
                .render_profile_output
                .unwrap_or_else(|| {
                    executable_dir.join("logs").join("render_profile.jsonl")
                }),
            step_budget: self.step_budget,
            width,
            height,
        })
    }
}
#[derive(Clone, Copy, Debug, ValueEnum)]
enum RenderProfileArg {
    Summary,
    Deep,
}
impl From<RenderProfileArg> for RenderProfileMode {
    fn from(value: RenderProfileArg) -> Self {
        match value {
            RenderProfileArg::Summary => Self::Summary,
            RenderProfileArg::Deep => Self::Deep,
        }
    }
}
enum State {
    Pending { args: Args, win_size: winit::dpi::PhysicalSize<u32> },
    Running { window: Arc<Window>, session: GameSession },
    Destroyed,
}
struct App {
    state: State,
    profile: &'static engine::profile::GameProfile,
}
impl App {
    fn new(args: Args, profile: &'static engine::profile::GameProfile) -> Self {
        let win_size = winit::dpi::PhysicalSize::new(args.width, args.height);
        Self {
            state: State::Pending { args, win_size },
            profile,
        }
    }
}
impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let profile = self.profile;
        let (args, mut win_size) = match std::mem::replace(
            &mut self.state,
            State::Destroyed,
        ) {
            State::Pending { args, win_size } => (args, win_size),
            _ => return,
        };
        let patch = args
            .patch
            .as_ref()
            .map(|root| {
                PatchBundle::load(root)
                    .unwrap_or_else(|error| {
                        panic!("patch initialization failed: {error}")
                    })
            });
        let presentation = patch
            .as_ref()
            .map(PatchBundle::presentation)
            .unwrap_or_default();
        if args.width == profile.internal_w && args.height == profile.internal_h {
            if let (Some(width), Some(height)) = (
                presentation.output_width,
                presentation.output_height,
            ) {
                win_size = winit::dpi::PhysicalSize::new(width, height);
            }
        }
        shell::validate_window_size(
                win_size.width,
                win_size.height,
                profile.internal_w,
                profile.internal_h,
            )
            .unwrap_or_else(|error| panic!("invalid initial window size: {error}"));
        if let Some(patch) = patch.as_ref() {
            eprintln!(
                "[PATCH] mounted {:?} locale={:?} scale={} images={} ir={} files={} root={}",
                patch.id(), patch.locale(), presentation.scale, patch.image_count(),
                patch.ir_script_count(), patch.file_count(), patch.root().display()
            );
        }
        let effective_ir_dir = args
            .ir_dir
            .clone()
            .or_else(|| {
                patch.as_ref().and_then(|patch| patch.ir_root().map(Path::to_path_buf))
            });
        let window_attributes = with_application_icon(
            Window::default_attributes()
                .with_title(profile.title)
                .with_inner_size(win_size)
                .with_resizable(false),
        );
        let window = Arc::new(
            event_loop.create_window(window_attributes).expect("create window"),
        );
        let instance = wgpu::Instance::new(
            &wgpu::InstanceDescriptor {
                backends: wgpu::Backends::all(),
                backend_options: wgpu::BackendOptions::default(),
                flags: wgpu::InstanceFlags::default(),
                memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            },
        );
        let surface = instance.create_surface(window.clone()).expect("create surface");
        let render_profile = args
            .render_profile
            .map(|mode| RenderProfileConfig::new(
                mode.into(),
                args.render_profile_output.clone(),
            ));
        let mut renderer = Renderer::new_with_presentation_and_profile(
            &instance,
            surface,
            window.inner_size(),
            presentation,
            render_profile,
            (profile.internal_w, profile.internal_h),
        );
        let mut vfs = Vfs::new(&args.asset_root, &args.arc_dir)
            .with_base_path(args.asset_root.clone());
        if let Some(priority) = profile.arc_priority {
            vfs = vfs.with_arc_priority(priority);
        }
        eprintln!(
            "[PATHS] assets={} arcs={} saves={} patch={} ir={}", args.asset_root
            .display(), args.arc_dir.display(), args.save_dir.display(), args.patch
            .as_deref().map(Path::display).map(| path | path.to_string())
            .unwrap_or_else(|| "<none>".to_owned()), effective_ir_dir.as_deref()
            .map(Path::display).map(| path | path.to_string()).unwrap_or_else(|| "<none>"
            .to_owned())
        );
        let mut host = EngineHost::with_profile(
                vfs,
                args.font.as_deref(),
                patch,
                profile,
            )
            .unwrap_or_else(|error| panic!("text font initialization failed: {error}"));
        if args.render_profile.is_some() {
            host.enable_render_profiling();
        }
        host.set_save_dir(args.save_dir.clone());
        if let Some(ir_dir) = effective_ir_dir {
            host.set_ir_dir(ir_dir);
        }
        if let Some(fullscreen) = host.take_fullscreen_request() {
            apply_fullscreen_request(&window, fullscreen);
        }
        if let Some(frame) = host.take_render_frame() {
            renderer.submit_frame(frame);
        }
        let session = GameSession::bootstrap(host, renderer, profile, args.step_budget);
        window.request_redraw();
        self.state = State::Running { window, session };
        event_loop.set_control_flow(ControlFlow::Poll);
    }
    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let State::Running { window, session, .. } = &mut self.state else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => {
                if !session.persist_and_quit() {
                    eprintln!("[SAVE] failed to persist system state on window close");
                }
                event_loop.exit();
            }
            WindowEvent::Resized(physical) => {
                session.resize(physical.width, physical.height);
                window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                session.draw();
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let (x, y) = session.pointer_client_position();
                let button = match button {
                    MouseButton::Left => Some(vm::host::PointerButton::Left),
                    MouseButton::Right => Some(vm::host::PointerButton::Right),
                    MouseButton::Middle => Some(vm::host::PointerButton::Middle),
                    _ => None,
                };
                let Some(button) = button else {
                    return;
                };
                let pressed = state == winit::event::ElementState::Pressed;
                if engine::diag_log_enabled() {
                    eprintln!(
                        "[INPUT-DIAG] {:?} {} at logical=({x},{y})", button, if pressed {
                        "pressed" } else { "released" }
                    );
                }
                session
                    .on_input(vm::host::Input::PointerButton {
                        button,
                        pressed,
                        x,
                        y,
                    });
                event_loop.set_control_flow(ControlFlow::Poll);
            }
            WindowEvent::CursorMoved { position, .. } => {
                let (x, y) = shell::window_to_logical(
                    (position.x, position.y),
                    (window.inner_size().width, window.inner_size().height),
                    self.profile.internal_w,
                    self.profile.internal_h,
                );
                if engine::diag_log_enabled() {
                    eprintln!(
                        "[INPUT-DIAG] cursor physical=({:.1},{:.1}) logical=({x},{y})",
                        position.x, position.y
                    );
                }
                session
                    .on_input(vm::host::Input::PointerMove {
                        x,
                        y,
                    });
                event_loop.set_control_flow(ControlFlow::Poll);
            }
            WindowEvent::CursorEntered { .. } => {
                session.on_input(vm::host::Input::PointerInside(true));
            }
            WindowEvent::CursorLeft { .. } => {
                session.on_input(vm::host::Input::PointerInside(false));
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if let Some(virtual_key) = platform_desktop::input::mouse_wheel_to_vk(
                    &delta,
                ) {
                    if engine::diag_log_enabled() {
                        eprintln!("[INPUT-DIAG] wheel -> vk=0x{virtual_key:02X}");
                    }
                    session
                        .on_input(vm::host::Input::Wheel {
                            up: virtual_key == 0x21,
                        });
                    event_loop.set_control_flow(ControlFlow::Poll);
                }
            }
            WindowEvent::Focused(focused) => {
                session.on_input(vm::host::Input::Focused(focused));
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let Some(virtual_key) = platform_desktop::input::physical_key_to_vk(
                    event.physical_key,
                ) {
                    let pressed = event.state == winit::event::ElementState::Pressed;
                    if engine::diag_log_enabled() {
                        eprintln!(
                            "[INPUT-DIAG] vk=0x{virtual_key:02X} {} repeat={}", if
                            pressed { "pressed" } else { "released" }, event.repeat
                        );
                    }
                    session
                        .on_input(vm::host::Input::Key {
                            virtual_key,
                            pressed,
                        });
                    event_loop.set_control_flow(ControlFlow::Poll);
                }
            }
            _ => {}
        }
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let State::Running { window, session, .. } = &mut self.state else {
            return;
        };
        let outcome = session.on_frame();
        if outcome.exit {
            event_loop.exit();
            return;
        }
        if outcome.present_submitted {
            window.request_redraw();
        }
        if let Some((x, y)) = outcome.cursor_warp {
            let size = window.inner_size();
            let (fx, fy) = shell::logical_to_window(
                (x, y),
                (size.width, size.height),
                self.profile.internal_w,
                self.profile.internal_h,
            );
            let _ = window
                .set_cursor_position(winit::dpi::PhysicalPosition::new(fx, fy));
        }
        if let Some(visible) = outcome.cursor_visible {
            window.set_cursor_visible(visible);
        }
        if let Some(fullscreen) = outcome.fullscreen {
            apply_fullscreen_request(window, fullscreen);
        }
        event_loop
            .set_control_flow(
                match outcome.flow {
                    Flow::Poll => ControlFlow::Poll,
                    Flow::Wait => ControlFlow::Wait,
                    Flow::WaitUntilMicros(micros) => {
                        ControlFlow::WaitUntil(
                            std::time::Instant::now()
                                + std::time::Duration::from_micros(micros),
                        )
                    }
                },
            );
    }
    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.state = State::Destroyed;
    }
}
fn apply_fullscreen_request(window: &Window, fullscreen: bool) {
    if window.fullscreen().is_some() == fullscreen {
        return;
    }
    let mode = fullscreen.then_some(Fullscreen::Borderless(None));
    window.set_fullscreen(mode);
    window.request_redraw();
    eprintln!(
        "[WINDOW] switched to {} mode", if fullscreen { "fullscreen" } else { "windowed"
        }
    );
}
fn with_application_icon(attributes: WindowAttributes) -> WindowAttributes {
    platform_desktop::shell::with_application_icon(attributes)
}
fn run(
    args: Args,
    profile: &'static engine::profile::GameProfile,
) -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    let mut app = App::new(args, profile);
    let event_loop = EventLoop::new()?;
    event_loop.run_app(&mut app)?;
    Ok(())
}
