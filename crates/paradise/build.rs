fn main() {
    let icon = format!(
        "{}/../owarusekai/assets/icon_fd.ico", std::env::var("CARGO_MANIFEST_DIR")
        .unwrap()
    );
    if std::path::Path::new(&icon).exists() {
        println!("cargo:rerun-if-changed={}", icon);
        if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
            winresource::WindowsResource::new()
                .set_icon(&icon)
                .compile()
                .expect(
                    "failed to embed game/assets/icon_fd.ico into the paradise executable",
                );
        }
    }
}
