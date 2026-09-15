use std::time::SystemTime;
use winit::window::WindowAttributes;
pub fn install() {
    #[cfg(windows)]
    {
        engine::platform_services::install_message_box(Box::new(message_box_ok));
        engine::platform_services::install_save_time_formatter(
            Box::new(format_last_write_time),
        );
    }
}
#[cfg(windows)]
fn message_box_ok(text: &str) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetActiveWindow;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, SetForegroundWindow, MB_OK,
    };
    let mut text_wide = text.encode_utf16().collect::<Vec<_>>();
    text_wide.push(0);
    let mut caption = engine::platform_services::app_name()
        .encode_utf16()
        .collect::<Vec<_>>();
    caption.push(0);
    unsafe {
        let owner = GetActiveWindow();
        if !owner.is_null() {
            SetForegroundWindow(owner);
        }
        MessageBoxW(owner, text_wide.as_ptr(), caption.as_ptr(), MB_OK);
    }
}
#[cfg(windows)]
fn format_last_write_time(modified: SystemTime) -> Option<String> {
    use windows_sys::Win32::Foundation::{FILETIME, SYSTEMTIME};
    use windows_sys::Win32::Storage::FileSystem::FileTimeToLocalFileTime;
    use windows_sys::Win32::System::Time::FileTimeToSystemTime;
    let since_epoch = modified.duration_since(std::time::UNIX_EPOCH).ok()?;
    let ticks = (since_epoch.as_nanos() / 100) as u64 + 116_444_736_000_000_000;
    let utc = FILETIME {
        dwLowDateTime: ticks as u32,
        dwHighDateTime: (ticks >> 32) as u32,
    };
    let mut local = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let mut system = SYSTEMTIME {
        wYear: 0,
        wMonth: 0,
        wDayOfWeek: 0,
        wDay: 0,
        wHour: 0,
        wMinute: 0,
        wSecond: 0,
        wMilliseconds: 0,
    };
    unsafe {
        if FileTimeToLocalFileTime(&utc, &mut local) == 0
            || FileTimeToSystemTime(&local, &mut system) == 0
        {
            return None;
        }
    }
    Some(
        format!(
            "{:02}/{:02}/{:02} {:02}:{:02}:{:02}", system.wYear % 100, system.wMonth,
            system.wDay, system.wHour, system.wMinute, system.wSecond
        ),
    )
}
#[cfg(windows)]
pub fn with_application_icon(attributes: WindowAttributes) -> WindowAttributes {
    use winit::platform::windows::{IconExtWindows, WindowAttributesExtWindows};
    let icon = winit::window::Icon::from_resource(1, None)
        .expect("load embedded application icon resource 1");
    attributes.with_window_icon(Some(icon.clone())).with_taskbar_icon(Some(icon))
}
#[cfg(not(windows))]
pub fn with_application_icon(attributes: WindowAttributes) -> WindowAttributes {
    attributes
}
pub fn install_panic_reporting() {
    #[cfg(all(windows, not(debug_assertions)))]
    std::panic::set_hook(
        Box::new(|info| {
            report_startup_error(
                &format!("The game terminated unexpectedly:\n\n{info}"),
            );
        }),
    );
}
pub fn report_startup_error(message: &str) {
    append_error_log(message);
    #[cfg(all(windows, not(debug_assertions)))]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            MessageBoxW, MB_ICONERROR, MB_OK,
        };
        let text = message.encode_utf16().chain(std::iter::once(0)).collect::<Vec<_>>();
        let title = format!("{} startup error", engine::platform_services::app_name())
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                text.as_ptr(),
                title.as_ptr(),
                MB_OK | MB_ICONERROR,
            );
        }
    }
    #[cfg(any(not(windows), debug_assertions))]
    eprintln!("{message}");
}
pub fn report_cli_output(message: &str) {
    #[cfg(all(windows, not(debug_assertions)))]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_OK};
        let text = message.encode_utf16().chain(std::iter::once(0)).collect::<Vec<_>>();
        let title = engine::platform_services::app_name()
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        unsafe {
            MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), MB_OK);
        }
    }
    #[cfg(any(not(windows), debug_assertions))]
    print!("{message}");
}
fn append_error_log(message: &str) {
    if let Ok(executable) = std::env::current_exe() {
        if let Some(directory) = executable.parent() {
            let log_path = directory
                .join(format!("{}-error.log", engine::platform_services::app_name()));
            if let Ok(mut log) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(log_path)
            {
                use std::io::Write;
                let _ = writeln!(log, "{message}\n");
            }
        }
    }
}
