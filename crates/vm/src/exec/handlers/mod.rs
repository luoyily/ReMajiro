mod control;
pub(super) mod helpers;
mod immediate;
mod inner;
mod inner_audio;
mod inner_ctx;
mod inner_file;
mod inner_gameplay;
mod inner_host;
mod inner_init;
mod inner_input;
mod inner_render;
mod inner_save;
mod inner_scene;
mod inner_scene2;
mod inner_strmath;
mod inner_text;
mod stack;
mod store;
mod system;
pub(crate) const INNER_ROUTE_GROUPS: &[&[u32]] = &[
    inner_host::HASHES,
    inner_scene::HASHES,
    inner_ctx::HASHES,
    inner_render::HASHES,
    inner_text::HASHES,
    inner_audio::HASHES,
    inner_scene2::HASHES,
    inner_save::HASHES,
    inner_input::HASHES,
    inner_strmath::HASHES,
    inner_file::HASHES,
    inner_gameplay::HASHES,
    inner_init::HASHES,
];
