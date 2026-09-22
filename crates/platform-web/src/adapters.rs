use crate::format::format_utc_timestamp;
pub fn install(bin_name: &str) {
    install_panic_reporting(bin_name.to_owned());
    install_services();
}
pub(crate) fn install_panic_reporting(bin_name: String) {
    std::panic::set_hook(
        Box::new(move |info| {
            web_sys::console::error_1(&format!("{bin_name} panic: {info}").into());
        }),
    );
}
fn install_services() {
    engine::platform_services::install_message_box(Box::new(message_ok));
    install_save_time_formatter();
}
pub(crate) fn install_save_time_formatter() {
    engine::platform_services::install_save_time_formatter(
        Box::new(|modified| { format_utc_timestamp(modified) }),
    );
}
fn message_ok(text: &str) {
    web_sys::console::log_1(&format!("[MSGBOX] {text}").into());
    if let Some(window) = web_sys::window() {
        window
            .alert_with_message(text)
            .unwrap_or_else(|error| {
                web_sys::console::error_1(&format!("alert failed: {error:?}").into());
            });
    }
}
