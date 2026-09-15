#[cfg(target_arch = "wasm32")]
type LogSink = Box<dyn Fn(&str) + Send + Sync>;
#[cfg(target_arch = "wasm32")]
static WASM_DIAG: AtomicBool = AtomicBool::new(false);
#[cfg(target_arch = "wasm32")]
static WASM_FRAME_PROBE: AtomicBool = AtomicBool::new(false);
#[cfg(target_arch = "wasm32")]
static SINK: OnceLock<LogSink> = OnceLock::new();
#[cfg(target_arch = "wasm32")]
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
#[cfg(target_arch = "wasm32")]
pub fn install_log_sink(sink: LogSink) {
    let _ = SINK.set(sink);
}
#[cfg(target_arch = "wasm32")]
pub fn set_diag_enabled(enabled: bool) {
    WASM_DIAG.store(enabled, Ordering::Relaxed);
}
#[cfg(target_arch = "wasm32")]
pub fn set_frame_probe_enabled(enabled: bool) {
    WASM_FRAME_PROBE.store(enabled, Ordering::Relaxed);
}
pub fn diag_log_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED
        .get_or_init(|| {
            #[cfg(not(target_arch = "wasm32"))]
            { std::env::var_os("OSTB_DIAG").is_some() }
            #[cfg(target_arch = "wasm32")] { WASM_DIAG.load(Ordering::Relaxed) }
        })
}
pub fn frame_probe_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED
        .get_or_init(|| {
            #[cfg(not(target_arch = "wasm32"))]
            { std::env::var_os("OSTB_FRAME_PROBE").is_some() }
            #[cfg(target_arch = "wasm32")] { WASM_FRAME_PROBE.load(Ordering::Relaxed) }
        })
}
pub(crate) fn text_trace_setting() -> Option<std::ffi::OsString> {
    #[cfg(not(target_arch = "wasm32"))] { std::env::var_os("OSTB_TEXT_TRACE") }
    #[cfg(target_arch = "wasm32")] { None }
}
pub(crate) fn emit_log(args: std::fmt::Arguments<'_>) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::eprintln!("{}", args);
    }
    #[cfg(target_arch = "wasm32")]
    if let Some(sink) = SINK.get() {
        sink(&std::fmt::format(args));
    }
}
pub(crate) fn runtime_log_enabled(format: &str) -> bool {
    diag_log_enabled() || format.starts_with("[HOTSPOT] add ")
        || format.starts_with("[HOTSPOT] callback ")
        || format.starts_with("[TEXT] {} page=")
        || format.starts_with("[SCENE] transition")
        || format.starts_with("[SCENE] script_reset") || format.starts_with("[UNIMPL]")
        || format.starts_with("[RENDER-SKIP]")
        || format.starts_with("[SCHED] budget exhausted")
        || format.starts_with("[SAVE] restore frame")
        || format.starts_with("[SAVE] snapshot frame") || format.contains("failed")
        || format.contains("not found") || format.contains("unknown")
        || format.contains("unresolved") || format.contains("rejected")
        || format.contains("mismatch") || format.contains("skipped")
        || format.contains("uses default key") || format.contains("falling back")
}
