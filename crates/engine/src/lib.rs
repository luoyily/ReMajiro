pub use web_time::{Instant, SystemTime};
pub use diag::diag_log_enabled;
macro_rules! eprintln {
    () => {
        if crate ::diag_log_enabled() { crate ::diag::emit_log(format_args!("")); }
    };
    ($format:literal $(, $arg:expr)* $(,)?) => {
        if crate ::diag::runtime_log_enabled($format) { crate
        ::diag::emit_log(format_args!($format $(, $arg)*)); }
    };
    ($($arg:tt)*) => {
        if crate ::diag_log_enabled() { crate ::diag::emit_log(format_args!($($arg)*)); }
    };
}
pub mod audio;
pub mod audio_out;
pub mod boot;
pub mod diag;
pub mod gpu;
pub mod host;
pub mod patch;
pub mod platform_services;
pub mod profile;
pub mod render_model;
pub mod render_profile;
pub mod renderer;
pub mod storage;
pub mod text;
pub mod vfs;
pub mod video;
pub use boot::EngineBoot;
pub use gpu::GpuContext;
pub use host::EngineHost;
pub use patch::{PatchBundle, PresentationConfig};
pub use render_model::{
    PageColorMode, PageCopyMode, PageFillMode, PageOp, PagePoint, RenderFrame,
    RenderPage, RenderQuad,
};
pub use render_profile::{RenderProfileConfig, RenderProfileMode, RuntimeCpuTimings};
pub use renderer::Renderer;
pub use vfs::Vfs;
