pub mod format;
pub mod input;
#[cfg(target_arch = "wasm32")]
mod adapters;
#[cfg(target_arch = "wasm32")]
pub mod audio_web;
#[cfg(target_arch = "wasm32")]
pub mod bridge;
#[cfg(target_arch = "wasm32")]
mod movie_web;
#[cfg(target_arch = "wasm32")]
pub(crate) mod saves;
#[cfg(target_arch = "wasm32")]
pub mod worker;
#[cfg(not(target_arch = "wasm32"))]
pub fn install() {}
#[cfg(target_arch = "wasm32")]
pub fn install() {
    adapters::install();
}
