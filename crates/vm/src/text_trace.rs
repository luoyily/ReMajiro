use std::ffi::OsStr;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
static TRACE_FILE: OnceLock<Option<Mutex<File>>> = OnceLock::new();
static TRACE_SEQUENCE: AtomicU64 = AtomicU64::new(1);
fn trace_path() -> Option<PathBuf> {
    let setting = crate::diag::text_trace_setting();
    let executable = (setting.as_deref() == Some(OsStr::new("1")))
        .then(std::env::current_exe)
        .and_then(Result::ok);
    resolve_trace_path(setting.as_deref(), executable.as_deref())
}
fn resolve_trace_path(
    setting: Option<&OsStr>,
    executable: Option<&Path>,
) -> Option<PathBuf> {
    match setting {
        Some(value) if value == "1" => {
            executable
                .and_then(Path::parent)
                .map(|parent| parent.join("ostb-text-trace.log"))
                .or_else(|| Some(PathBuf::from("ostb-text-trace.log")))
        }
        Some(value) if !value.is_empty() && value != "0" => Some(value.into()),
        _ => None,
    }
}
fn trace_file() -> Option<&'static Mutex<File>> {
    TRACE_FILE
        .get_or_init(|| {
            let path = trace_path()?;
            let mut file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(path)
                .ok()?;
            let _ = writeln!(
                file, "# OSTB text/input trace pid={} unix_ms={}", std::process::id(),
                crate ::SystemTime::now().duration_since(crate ::SystemTime::UNIX_EPOCH)
                .map(| value | value.as_millis()).unwrap_or(0)
            );
            Some(Mutex::new(file))
        })
        .as_ref()
}
pub fn record(args: std::fmt::Arguments<'_>) {
    let Some(file) = trace_file() else {
        return;
    };
    let sequence = TRACE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let Ok(mut file) = file.lock() else {
        return;
    };
    let _ = writeln!(file, "{sequence:06} {args}");
    let _ = file.flush();
}
#[macro_export]
macro_rules! text_trace {
    ($($arg:tt)*) => {
        $crate::text_trace::record(format_args!($($arg)*))
    };
}

