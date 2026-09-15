#[cfg(target_arch = "wasm32")]
type LogSink = Box<dyn Fn(&str) + Send + Sync>;
#[cfg(target_arch = "wasm32")]
static WASM_DIAG: AtomicBool = AtomicBool::new(false);
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
pub fn diag_log_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED
        .get_or_init(|| {
            #[cfg(not(target_arch = "wasm32"))]
            { std::env::var_os("OSTB_DIAG").is_some() }
            #[cfg(target_arch = "wasm32")] { WASM_DIAG.load(Ordering::Relaxed) }
        })
}
pub fn emit_log(args: std::fmt::Arguments<'_>) {
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
        || format.starts_with("[IMAGE-CLASS] ") || format.starts_with("[TEXT] {} page=")
        || format.starts_with("[SCENE] transition")
        || format.starts_with("[SCENE] script_reset") || format.starts_with("[UNIMPL]")
        || format.starts_with("[RENDER-SKIP]") || format.starts_with("[COMPOSITE-")
        || format.starts_with("[BOOT] ") || format.starts_with("[SAVE] ")
        || format.starts_with("[VIDEO] ") || format.starts_with("[CENSUS]")
        || format.starts_with("[SCHED] budget exhausted")
        || format.starts_with("[IR] resolved ") || format.contains("failed")
        || format.contains("not found") || format.contains("unknown")
        || format.contains("unresolved") || format.contains("rejected")
        || format.contains("mismatch") || format.contains("skipped")
        || format.contains("uses default key") || format.contains("falling back")
}
pub fn env_knob_u64(name: &str, default: u64) -> u64 {
    #[cfg(not(target_arch = "wasm32"))]
    { std::env::var(name).ok().and_then(|value| value.parse().ok()).unwrap_or(default) }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = name;
        default
    }
}
pub fn env_knob_bool(name: &str, default: bool) -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::env::var(name)
            .ok()
            .and_then(|value| match value.to_ascii_lowercase().as_str() {
                "1" | "true" | "yes" | "on" => Some(true),
                "0" | "false" | "no" | "off" => Some(false),
                _ => None,
            })
            .unwrap_or(default)
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = name;
        default
    }
}
pub fn env_knob_path(name: &str) -> Option<std::path::PathBuf> {
    #[cfg(not(target_arch = "wasm32"))]
    { std::env::var(name).ok().map(std::path::PathBuf::from) }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = name;
        None
    }
}
