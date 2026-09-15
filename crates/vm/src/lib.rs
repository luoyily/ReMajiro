pub use web_time::SystemTime;
pub mod diag;
pub use diag::{diag_log_enabled, frame_probe_enabled};
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
pub mod cursor;
pub mod exec;
pub mod host;
pub mod ir;
pub mod opcode;
mod save;
pub mod scheduler;
pub mod text;
pub mod text_trace;
pub mod value;
pub use cursor::Cursor;
pub use exec::{CallFrame, ScriptEntry, StepResult, Vm, VmError};
pub use host::{
    DisplayTextSite, FileTraceHost, Host, Input, LoadedScript, TextSite, TraceHost,
};
pub use opcode::{
    instruction_size, is_valid_outer_opcode, lookup_inner_opcode, opcode_size,
    outer_opcode_name, INNER_VM_TABLE,
};
pub use scheduler::SchedSignal;
pub use text::{format_tokens, tokenize, ControlCode, ControlCodeKind, TextToken};
pub use value::{Scope, Stack, Value, ValueData};
pub(crate) fn formats_crc32(bytes: &[u8]) -> u32 {
    let cstr = bytes.split(|&b| b == 0).next().unwrap_or(bytes);
    formats::crypto::crc32(cstr)
}

