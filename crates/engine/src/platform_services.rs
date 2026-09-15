use crate::SystemTime;
use std::sync::OnceLock;
type MessageBoxFn = Box<dyn Fn(&str) + Send + Sync>;
type SaveTimeFormatFn = Box<dyn Fn(SystemTime) -> Option<String> + Send + Sync>;
static MESSAGE_BOX: OnceLock<MessageBoxFn> = OnceLock::new();
static SAVE_TIME_FORMAT: OnceLock<SaveTimeFormatFn> = OnceLock::new();
static APP_NAME: OnceLock<String> = OnceLock::new();
pub fn install_message_box(implementation: MessageBoxFn) {
    let _ = MESSAGE_BOX.set(implementation);
}
pub fn install_save_time_formatter(implementation: SaveTimeFormatFn) {
    let _ = SAVE_TIME_FORMAT.set(implementation);
}
pub fn install_app_name(name: impl Into<String>) {
    let _ = APP_NAME.set(name.into());
}
pub fn app_name() -> &'static str {
    APP_NAME.get().map(String::as_str).unwrap_or("owarusekai")
}
pub fn message_ok(text: &str) {
    match MESSAGE_BOX.get() {
        Some(show) => show(text),
        None => eprintln!("[SYS] message_ok {text:?}"),
    }
}
pub fn format_save_last_write_time(modified: SystemTime) -> Option<String> {
    let format = SAVE_TIME_FORMAT.get()?;
    format(modified)
}
