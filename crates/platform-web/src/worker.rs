use crate::bridge::Bridge;
use crate::saves::WebSaveStore;
use engine::profile::GameProfile;
use engine::{EngineHost, Renderer};
use js_sys::SharedArrayBuffer;
use shell::GameSession;
use wasm_bindgen::prelude::*;
#[wasm_bindgen]
pub struct EngineWorker {
    session: Option<GameSession>,
    profile: &'static GameProfile,
    canvas: web_sys::OffscreenCanvas,
    vfs: Option<engine::Vfs>,
    patch: Option<engine::PatchBundle>,
    presentation: engine::PresentationConfig,
    saves: std::sync::Arc<WebSaveStore>,
    audio_ended: std::sync::Arc<std::sync::Mutex<std::collections::HashSet<u32>>>,
}
#[wasm_bindgen]
impl EngineWorker {
    #[wasm_bindgen(constructor)]
    pub fn new(
        canvas: web_sys::OffscreenCanvas,
        sab: SharedArrayBuffer,
        game: String,
        files_json: String,
        patch_files_json: String,
    ) -> Result<EngineWorker, JsValue> {
        let profile = match game.as_str() {
            "owarusekai" => &GameProfile::OWARUSEKAI,
            "ruri" => &GameProfile::RURI,
            "paradise" => &GameProfile::PARADISE,
            other => return Err(js_error(format!("unknown game profile {other:?}"))),
        };
        let files: Vec<(String, u64)> = serde_json::from_str(&files_json)
            .map_err(|error| js_error(format!("bad file listing: {error}")))?;
        let bridge = Bridge::new(sab).map_err(js_error)?;
        install_worker_services(bridge.clone(), profile.bin_name);
        let reader: std::sync::Arc<dyn formats::remote::RemoteReader> = std::sync::Arc::new(
            bridge,
        );
        let mut vfs = engine::Vfs::from_remote(std::sync::Arc::clone(&reader), files)
            .map_err(js_error)?;
        if let Some(priority) = profile.arc_priority {
            vfs = vfs.with_arc_priority(priority);
        }
        let (patch, presentation) = match engine::PatchBundle::from_remote(
            std::sync::Arc::clone(&reader),
            &serde_json::from_str::<Vec<(String, u64)>>(&patch_files_json)
                .map_err(|error| js_error(format!("bad patch listing: {error}")))?,
        ) {
            Ok(bundle) => {
                let presentation = bundle.presentation();
                web_sys::console::log_1(
                    &format!(
                        "[PATCH] mounted {:?} scale={}", bundle.id(), presentation.scale
                    )
                        .into(),
                );
                (Some(bundle), presentation)
            }
            Err(error) => {
                if !error.contains("no patch.toml") {
                    return Err(js_error(error));
                }
                web_sys::console::warn_1(
                    &format!("[PATCH] not mounted: {error}").into(),
                );
                (None, engine::PresentationConfig::default())
            }
        };
        let audio_ended = std::sync::Arc::new(
            std::sync::Mutex::new(std::collections::HashSet::new()),
        );
        crate::audio_web::install(std::sync::Arc::clone(&audio_ended));
        crate::movie_web::install();
        crate::movie_web::install_player();
        Ok(Self {
            session: None,
            profile,
            canvas,
            vfs: Some(vfs),
            patch,
            presentation,
            saves: std::sync::Arc::new(WebSaveStore::new()),
            audio_ended,
        })
    }
    pub fn audio_ended(&self, id: u32) {
        self.audio_ended.lock().unwrap().insert(id);
    }
    pub fn movie_ended(&self, duration_ms: f64) {
        crate::movie_web::notify_ended(duration_ms);
    }
    pub fn add_save_file(&self, name: String, data: Vec<u8>) {
        self.saves.add_file(name, data);
    }
    pub fn export_saves(&self) -> wasm_bindgen::JsValue {
        self.saves.export_files().into()
    }
    pub fn import_save(&self, name: String, data: Vec<u8>) {
        self.saves.import_file(&name, data);
    }
    pub fn delete_save(&self, name: String) {
        use engine::storage::SaveStore as _;
        self.saves.delete(&name);
    }
    pub fn is_save_name(&self, name: String) -> bool {
        WebSaveStore::is_save_name(&name)
    }
    pub async fn boot(&mut self) -> Result<(), JsValue> {
        let profile = self.profile;
        let vfs = self
            .vfs
            .take()
            .ok_or_else(|| js_error("boot called twice".to_string()))?;
        let patch = self.patch.take();
        let font_bytes = ["font.ttf", "font.ttc"]
            .into_iter()
            .find_map(|name| vfs.find(name));
        let patch_fonts = patch
            .as_ref()
            .map(engine::PatchBundle::font_bytes)
            .unwrap_or_default();
        let ir_scripts: Option<
            std::sync::Arc<std::collections::HashMap<String, String>>,
        > = match patch.as_ref() {
            Some(bundle) => {
                let scripts = bundle.ir_script_bytes();
                if scripts.is_empty() {
                    None
                } else {
                    Some(std::sync::Arc::new(scripts.into_iter().collect()))
                }
            }
            None => None,
        };
        let mut host = EngineHost::with_profile_and_font_bytes(
                vfs,
                font_bytes,
                patch_fonts,
                patch,
                profile,
            )
            .map_err(js_error)?;
        host.set_save_store(self.saves.clone());
        if let Some(scripts) = ir_scripts {
            host.set_ir_scripts(scripts);
        }
        let (canvas_w, canvas_h) = match (
            self.presentation.output_width,
            self.presentation.output_height,
        ) {
            (Some(width), Some(height)) => (width, height),
            _ => (profile.internal_w, profile.internal_h),
        };
        let instance = wgpu::Instance::new(
            &wgpu::InstanceDescriptor {
                backends: wgpu::Backends::BROWSER_WEBGPU,
                backend_options: wgpu::BackendOptions::default(),
                flags: wgpu::InstanceFlags::default(),
                memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            },
        );
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::OffscreenCanvas(self.canvas.clone()))
            .map_err(|error| js_error(format!("create surface: {error:?}")))?;
        let adapter = instance
            .request_adapter(
                &wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    compatible_surface: Some(&surface),
                    force_fallback_adapter: false,
                },
            )
            .await
            .map_err(|error| js_error(format!("no WebGPU adapter: {error}")))?;
        self.canvas.set_width(canvas_w);
        self.canvas.set_height(canvas_h);
        let renderer = Renderer::from_adapter_async(
                adapter,
                surface,
                winit::dpi::PhysicalSize::new(canvas_w, canvas_h),
                self.presentation.clone(),
                None,
                (profile.internal_w, profile.internal_h),
            )
            .await;
        self.session = Some(GameSession::bootstrap(host, renderer, profile, 1_000_000));
        Ok(())
    }
    pub fn on_tick(&mut self) -> Option<bool> {
        crate::movie_web::poll_tick();
        let session = self.session.as_mut()?;
        let outcome = session.on_frame();
        if outcome.present_submitted {
            session.draw();
        }
        outcome.fullscreen
    }
    pub fn key(&mut self, code: String, pressed: bool) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        if let Some(virtual_key) = crate::input::keyboard_code_to_vk(&code) {
            session
                .on_input(vm::host::Input::Key {
                    virtual_key,
                    pressed,
                });
        }
    }
    pub fn pointer_move(&mut self, x: f64, y: f64, w: u32, h: u32) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let (lx, ly) = shell::window_to_logical(
            (x, y),
            (w, h),
            self.profile.internal_w,
            self.profile.internal_h,
        );
        session
            .on_input(vm::host::Input::PointerMove {
                x: lx,
                y: ly,
            });
    }
    pub fn pointer_button(
        &mut self,
        button: u8,
        pressed: bool,
        x: f64,
        y: f64,
        w: u32,
        h: u32,
    ) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        use vm::host::PointerButton::*;
        let mapped = match button {
            0 => Some(Left),
            1 => Some(Middle),
            2 => Some(Right),
            _ => None,
        };
        let Some(button) = mapped else {
            return;
        };
        let (lx, ly) = shell::window_to_logical(
            (x, y),
            (w, h),
            self.profile.internal_w,
            self.profile.internal_h,
        );
        session
            .on_input(vm::host::Input::PointerButton {
                button,
                pressed,
                x: lx,
                y: ly,
            });
    }
    pub fn wheel(&mut self, delta_y: f64) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        if let Some(virtual_key) = crate::input::wheel_delta_to_vk(delta_y) {
            session
                .on_input(vm::host::Input::Wheel {
                    up: virtual_key == 0x21,
                });
        }
    }
    pub fn focus(&mut self, focused: bool) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        session.on_input(vm::host::Input::Focused(focused));
    }
    pub fn persist(&mut self) {
        if self.saves.has_imported_mss() {
            web_sys::console::log_1(
                &"[SAVE] unload persist skipped: imported system state not yet ".into(),
            );
            web_sys::console::log_1(&"superseded by an engine write".into());
            return;
        }
        if let Some(session) = self.session.as_mut() {
            session.persist_and_quit();
        }
    }
}
fn install_worker_services(bridge: Bridge, bin_name: &str) {
    crate::adapters::install_panic_reporting(bin_name.to_owned());
    crate::adapters::install_save_time_formatter();
    let make_sink = || -> Box<dyn Fn(&str) + Send + Sync> {
        Box::new(|line| web_sys::console::log_1(&line.into()))
    };
    engine::diag::install_log_sink(make_sink());
    vm::diag::install_log_sink(make_sink());
    engine::platform_services::install_message_box(
        Box::new(move |text| {
            bridge.message_box(text);
        }),
    );
}
fn js_error(message: String) -> JsValue {
    web_sys::console::error_1(&format!("[BOOT-ERR] {message}").into());
    JsValue::from_str(&message)
}
