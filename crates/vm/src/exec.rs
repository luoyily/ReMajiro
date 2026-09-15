use crate::cursor::Cursor;
use crate::host::{Host, HotspotEvent};
use crate::opcode::is_valid_outer_opcode;
use crate::value::{Scope, Stack, Value};
pub(crate) const MARK_SYSVAR_HASH: u32 = 0x11F9_1FD3;
pub(crate) const X_CONTROL_NAME_HASH: u32 = 0xEC77_1B85;
pub(crate) mod handlers;
#[derive(Debug)]
pub enum VmError {
    Unimplemented { opcode_or_hash: u32, detail: String, ip: usize },
    CodeOverread { ip: usize, need: usize, have: usize },
    StackUnderflow,
    InvalidJump { from: usize, target: usize, len: usize },
    UnknownEntryHash { hash: u32, from_script: usize, ip: usize },
    Exit,
    HostQuit,
    Other(String),
}
impl std::fmt::Display for VmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VmError::Unimplemented { opcode_or_hash, detail, ip } => {
                write!(
                    f, "unimplemented opcode/handler 0x{:X} ({}) at ip=0x{:X}",
                    opcode_or_hash, detail, ip
                )
            }
            VmError::CodeOverread { ip, need, have } => {
                write!(f, "code overread at ip=0x{:X} need={} have={}", ip, need, have)
            }
            VmError::StackUnderflow => write!(f, "stack underflow"),
            VmError::InvalidJump { from, target, len } => {
                write!(
                    f, "jump from 0x{:X} to 0x{:X} (code len 0x{:X})", from, target, len
                )
            }
            VmError::UnknownEntryHash { hash, from_script, ip } => {
                write!(
                    f,
                    "CALL target hash 0x{:08X} not in script {} entry table (ip=0x{:X})",
                    hash, from_script, ip
                )
            }
            VmError::Exit => write!(f, "VM exited (root frame returned)"),
            VmError::HostQuit => write!(f, "host requested quit"),
            VmError::Other(s) => write!(f, "{}", s),
        }
    }
}
impl std::error::Error for VmError {}
#[derive(Debug, Clone)]
pub struct ScriptEntry {
    pub name_hash: u32,
    pub offset: u32,
}
pub struct CallFrame {
    pub cursor: Cursor<'static>,
    pub script_idx: usize,
    pub entry_sp: usize,
    pub stack_base: usize,
    pub cleanup_mode: u32,
    pub(crate) script_marker: u32,
    pub param_count: i16,
    pub label: String,
    pub(crate) jump_target_a: Option<usize>,
    pub(crate) jump_target_b: Option<usize>,
    pub(crate) jump_target_c: Option<usize>,
    pub(crate) frame_advance_target: Option<usize>,
    pub(crate) transition_target: Option<usize>,
    pub(crate) force_return: bool,
    pub(crate) wait_deadline: WaitDeadline,
    pub(crate) wait_present_epoch: Option<u64>,
    pub(crate) text_line_pending: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WaitDeadline {
    None,
    At(i32),
    Never,
}
#[derive(Debug, Clone)]
pub(crate) struct TextReplayState {
    pub(crate) cursor: i32,
    pub(crate) first_record: bool,
    pub(crate) page_pause: bool,
    pub(crate) force_newline: bool,
    pub(crate) rendering: bool,
}
pub(crate) struct TextVmState {
    pub(crate) first_render: bool,
    pub(crate) render_state: i32,
    pub(crate) display_state: i32,
    pub(crate) skip_depth: i32,
    pub(crate) history_records: Vec<TextHistoryRecord>,
    pub(crate) char_id: i32,
    pub(crate) accumulator: Vec<u8>,
    pub(crate) page_accumulator: Vec<u8>,
    pub(crate) page_capture_enabled: bool,
    pub(crate) history_buf: Vec<u8>,
    pub(crate) pending_render_line: Vec<u8>,
    pub(crate) last_history_render: Option<(usize, usize)>,
    pub(crate) replay: Option<TextReplayState>,
}
impl Default for TextVmState {
    fn default() -> Self {
        Self {
            first_render: false,
            render_state: 0,
            display_state: 0,
            skip_depth: 0,
            history_records: vec![TextHistoryRecord::default(); 20],
            char_id: 0,
            accumulator: Vec::new(),
            page_accumulator: Vec::new(),
            page_capture_enabled: false,
            history_buf: vec![0; 40_000],
            pending_render_line: Vec::new(),
            last_history_render: None,
            replay: None,
        }
    }
}
impl TextVmState {
    pub(crate) fn clear_history(&mut self) {
        self.accumulator.clear();
        self.char_id = 0;
        self.history_records.fill(TextHistoryRecord::default());
    }
    pub(crate) fn reset_for_wait(&mut self) {
        self.clear_history();
        self.page_accumulator.clear();
        self.page_capture_enabled = true;
        self.render_state = 0;
        self.pending_render_line.clear();
        self.last_history_render = None;
    }
    pub(crate) fn reset_for_replay_wait(&mut self) {
        self.clear_history();
        self.page_accumulator.clear();
        self.page_capture_enabled = true;
        self.render_state = 0;
        self.last_history_render = None;
    }
    fn history_capture_enabled(&self) -> bool {
        self.display_state == 0 && self.skip_depth == 0
    }
    fn append_render_line(&mut self, payload: &[u8]) {
        let payload = payload
            .get(..payload.iter().position(|byte| *byte == 0).unwrap_or(payload.len()))
            .unwrap_or(&[]);
        self.pending_render_line.extend_from_slice(payload);
    }
    fn append_history_record(&mut self, payload: &[u8]) {
        if !self.history_capture_enabled() {
            return;
        }
        let payload = payload
            .get(..payload.iter().position(|byte| *byte == 0).unwrap_or(payload.len()))
            .unwrap_or(&[]);
        let total = payload.len().saturating_add(3);
        if total > self.history_buf.len() || total > usize::from(u16::MAX) {
            return;
        }
        let keep = self.history_buf.len() - total;
        self.history_buf.copy_within(total.., 0);
        self.history_buf[keep..keep + payload.len()].copy_from_slice(payload);
        self.history_buf[keep + payload.len()] = 0;
        let footer = (total as u16).to_le_bytes();
        let end = self.history_buf.len();
        self.history_buf[end - 2..].copy_from_slice(&footer);
    }
    fn record_voice(&mut self, name: &[u8]) {
        let name = name
            .get(..name.iter().position(|byte| *byte == 0).unwrap_or(name.len()))
            .unwrap_or(&[]);
        if name.is_empty() {
            return;
        }
        let mut index = usize::try_from(self.char_id).unwrap_or(0).min(19);
        if index != 0 && self.history_records[index - 1].open {
            index -= 1;
        }
        let record = &mut self.history_records[index];
        if record.text.is_empty() {
            record.text.extend_from_slice(name);
        } else {
            if !record.voice.is_empty() {
                record.voice.push(b',');
            }
            record.voice.extend_from_slice(name);
        }
    }
    fn update_inline_records(
        &mut self,
        markers: InlineRecordMarkers,
        start_y: i32,
        end_y: i32,
        line_height: i32,
    ) {
        if markers.open {
            let index = usize::try_from(self.char_id).unwrap_or(20);
            if let Some(record) = self.history_records.get_mut(index) {
                record.open = true;
                if !record.text.is_empty() {
                    record.value_a = start_y;
                    self.char_id = self.char_id.saturating_add(1).min(20);
                }
            }
        }
        if markers.close && self.char_id > 0 {
            let index = (self.char_id - 1) as usize;
            if let Some(record) = self.history_records.get_mut(index) {
                record.open = false;
                if record.value_b == 0 {
                    record.value_b = end_y
                        .saturating_add(line_height)
                        .saturating_sub(record.value_a);
                }
            }
        }
    }
}
pub struct Vm {
    pub(crate) scripts: Vec<LoadedCode>,
    pub(crate) frames: Vec<CallFrame>,
    pub stack: Stack,
    pub globals: Vec<Value>,
    pub locals: Vec<Value>,
    pub threads: Vec<Value>,
    pub(crate) global_keys: std::collections::HashMap<u32, usize>,
    pub(crate) local_keys: std::collections::HashMap<u32, usize>,
    pub(crate) thread_keys: std::collections::HashMap<u32, usize>,
    pub(crate) rng_state: u32,
    pub(crate) ip_trace: std::collections::VecDeque<(usize, usize)>,
    pub(crate) derail_dumped: bool,
    pub(crate) text: TextVmState,
    pub(crate) host_funcs: std::collections::HashMap<u32, Vec<HostFuncReg>>,
    pub(crate) suspended_ctxs: Vec<Option<ExecContext>>,
    pub(crate) current_ctx: usize,
    pub(crate) active_function_id: u32,
    pub(crate) active_parent_function_id: Option<u32>,
    pub(crate) active_state: CtxState,
    pub(crate) active_phase: CtxPhase,
    pub(crate) active_finalizer_target: Option<(usize, usize)>,
    pub(crate) next_function_id: u32,
    pub(crate) scene_transition_requested: bool,
    pub(crate) scene_cycle_preserve_audio: bool,
    pub(crate) scheduler_no_present: bool,
    pub(crate) pending_save_snapshot: Option<formats::save::SavFile>,
    pub(crate) pending_load: Option<formats::save::SavFile>,
    pub(crate) quit_requested: bool,
    pub(crate) save_description: [u8; 128],
    pub(crate) pending_hotspot_callback: Option<HotspotEvent>,
    pub(crate) pending_inner_host_bridges: Vec<(u32, Vec<Value>, bool)>,
    pub(crate) input_suppressed: bool,
    pub(crate) native_mode_bits: u8,
    pub(crate) native_mode_special: bool,
    pub(crate) native_mode_counter: u32,
    pub(crate) native_skip_mode: bool,
    pub(crate) readmarks: std::collections::HashMap<
        Vec<u8>,
        formats::save::ReadmarkRecord,
    >,
    pub(crate) native_mode_restore_auto: bool,
    pub(crate) schedule_wall_slice: Option<std::time::Duration>,
}
#[derive(Debug, Clone)]
pub(crate) struct HostFuncReg {
    pub ctx: u32,
    pub callback_hash: u32,
    pub callback_target: Option<(usize, usize)>,
    pub pinned: bool,
}
#[derive(Debug, Clone, Default)]
pub(crate) struct TextHistoryRecord {
    pub(crate) value_a: i32,
    pub(crate) value_b: i32,
    pub(crate) text: Vec<u8>,
    pub(crate) voice: Vec<u8>,
    pub(crate) open: bool,
}
pub(crate) struct LoadedCode {
    pub(crate) code: Box<[u8]>,
    pub(crate) code_crc32: u32,
    pub(crate) entries: Vec<ScriptEntry>,
    pub(crate) name: Option<Vec<u8>>,
    pub(crate) readmark_count: u32,
}
fn canonical_readmark_name(name: &[u8]) -> Vec<u8> {
    let name = name.split(|byte| *byte == 0).next().unwrap_or(name);
    let basename = name
        .rsplit(|byte| matches!(* byte, b'/' | b'\\'))
        .next()
        .unwrap_or(name);
    let mut canonical = basename.to_vec();
    if canonical.len() < 4
        || !canonical[canonical.len() - 4..].eq_ignore_ascii_case(b".MJO")
    {
        canonical.extend_from_slice(b".MJO");
    }
    uppercase_sjis_ascii(&mut canonical);
    canonical.truncate(formats::save::SCRIPT_NAME_SIZE - 1);
    canonical
}
fn legacy_readmark_name(name: &[u8]) -> Vec<u8> {
    let name = name.split(|byte| *byte == 0).next().unwrap_or(name);
    let basename = name
        .rsplit(|byte| matches!(* byte, b'/' | b'\\'))
        .next()
        .unwrap_or(name);
    let mut canonical = basename.to_vec();
    if canonical.len() < 4
        || !canonical[canonical.len() - 4..].eq_ignore_ascii_case(b".MJO")
    {
        canonical.extend_from_slice(b".MJO");
    }
    canonical.make_ascii_uppercase();
    canonical.truncate(formats::save::SCRIPT_NAME_SIZE - 1);
    canonical
}
fn uppercase_sjis_ascii(bytes: &mut [u8]) {
    let mut index = 0;
    while index < bytes.len() {
        if matches!(bytes[index], 0x81..= 0x9F | 0xE0..= 0xFC) && index + 1 < bytes.len()
        {
            index += 2;
        } else {
            bytes[index].make_ascii_uppercase();
            index += 1;
        }
    }
}
fn write_readmark_name(
    destination: &mut [u8; formats::save::SCRIPT_NAME_SIZE],
    name: &[u8],
) {
    destination.fill(0);
    let length = name.len().min(destination.len() - 1);
    destination[..length].copy_from_slice(&name[..length]);
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CtxState {
    Idle,
    Running,
    Waiting,
    Inactive,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CtxPhase {
    Initial,
    TransitionRequested,
    Cleanup,
    CleanupReturned,
    Finalizer,
    Dead,
}
pub(crate) struct ExecContext {
    pub(crate) function_id: u32,
    pub(crate) parent_function_id: Option<u32>,
    pub(crate) frames: Vec<CallFrame>,
    pub(crate) stack: Stack,
    pub(crate) threads: Vec<Value>,
    pub(crate) thread_keys: std::collections::HashMap<u32, usize>,
    pub(crate) state: CtxState,
    pub(crate) phase: CtxPhase,
    pub(crate) finalizer_target: Option<(usize, usize)>,
}
impl ExecContext {
    fn new(function_id: u32, parent_function_id: Option<u32>) -> Self {
        let mut stack = Stack::new();
        stack.push(Value::int(0));
        Self {
            function_id,
            parent_function_id,
            frames: Vec::new(),
            stack,
            threads: Vec::new(),
            thread_keys: std::collections::HashMap::new(),
            state: CtxState::Idle,
            phase: CtxPhase::Initial,
            finalizer_target: None,
        }
    }
}
fn redirect_transition_frames(frames: &mut [CallFrame]) -> bool {
    let mut has_cleanup = false;
    for frame in frames {
        if let Some(target) = frame.transition_target.take() {
            frame.cursor.ip = target;
            frame.force_return = false;
            has_cleanup = true;
        } else {
            frame.force_return = true;
        }
        frame.wait_deadline = WaitDeadline::None;
    }
    has_cleanup
}
fn install_context_finalizer(
    scripts: &[LoadedCode],
    frames: &mut Vec<CallFrame>,
    stack: &mut Stack,
    (script_idx, target): (usize, usize),
) {
    stack.push(Value::int(0));
    let entry_sp = stack.len();
    let code_ptr: *const [u8] = scripts[script_idx].code.as_ref();
    let code: &'static [u8] = unsafe { &*code_ptr };
    frames.clear();
    frames
        .push(CallFrame {
            cursor: Cursor::new(code, target),
            script_idx,
            entry_sp,
            stack_base: entry_sp,
            cleanup_mode: 0,
            script_marker: 0,
            param_count: 0,
            label: format!("ctx-finalizer@0x{target:06X}"),
            jump_target_a: None,
            jump_target_b: None,
            jump_target_c: None,
            frame_advance_target: None,
            transition_target: None,
            force_return: false,
            wait_deadline: WaitDeadline::None,
            wait_present_epoch: None,
            text_line_pending: false,
        });
}
fn process_saved_context_phase(scripts: &[LoadedCode], ctx: &mut ExecContext) -> bool {
    match ctx.phase {
        CtxPhase::TransitionRequested => {
            if redirect_transition_frames(&mut ctx.frames) {
                ctx.phase = CtxPhase::Cleanup;
                ctx.state = CtxState::Running;
                true
            } else if let Some(target) = ctx.finalizer_target.take() {
                install_context_finalizer(
                    scripts,
                    &mut ctx.frames,
                    &mut ctx.stack,
                    target,
                );
                ctx.phase = CtxPhase::Finalizer;
                ctx.state = CtxState::Running;
                true
            } else {
                false
            }
        }
        CtxPhase::CleanupReturned => {
            if let Some(target) = ctx.finalizer_target.take() {
                install_context_finalizer(
                    scripts,
                    &mut ctx.frames,
                    &mut ctx.stack,
                    target,
                );
                ctx.phase = CtxPhase::Finalizer;
                ctx.state = CtxState::Running;
                true
            } else {
                false
            }
        }
        CtxPhase::Dead => false,
        CtxPhase::Initial | CtxPhase::Cleanup | CtxPhase::Finalizer => true,
    }
}
fn two_hex_fields(a: i32, b: i32) -> Vec<u8> {
    format!("0x{:08x}0x{:08x}", a as u32, b as u32).into_bytes()
}
#[derive(Clone, Copy)]
struct InlineRecordMarkers {
    open: bool,
    close: bool,
}
fn split_name_popup(bytes: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
    const OPEN_QUOTE: [u8; 2] = [0x81, 0x75];
    let bytes = bytes
        .get(..bytes.iter().position(|byte| *byte == 0).unwrap_or(bytes.len()))
        .unwrap_or(bytes);
    let index = bytes.windows(OPEN_QUOTE.len()).position(|window| window == OPEN_QUOTE)?;
    (index != 0).then(|| (bytes[..index].to_vec(), bytes[index..].to_vec()))
}
fn inline_record_markers(bytes: &[u8]) -> InlineRecordMarkers {
    const OPEN_QUOTE: [u8; 2] = [0x81, 0x75];
    const CLOSE_QUOTE: [u8; 2] = [0x81, 0x76];
    const OPEN_PAREN: [u8; 2] = [0x81, 0x69];
    const CLOSE_PAREN: [u8; 2] = [0x81, 0x6A];
    const OPEN_DOUBLE: [u8; 2] = [0x81, 0x77];
    const CLOSE_DOUBLE: [u8; 2] = [0x81, 0x78];
    let mut transformed = bytes.to_vec();
    let mut open = false;
    let mut close = false;
    for pattern in [
        [OPEN_QUOTE.as_slice(), OPEN_PAREN.as_slice()].concat(),
        [OPEN_QUOTE.as_slice(), OPEN_DOUBLE.as_slice()].concat(),
    ] {
        while let Some(index) = find_byte_sequence(&transformed, &pattern) {
            transformed.drain(index..index + OPEN_QUOTE.len());
            open = true;
        }
    }
    for pattern in [
        [CLOSE_PAREN.as_slice(), CLOSE_QUOTE.as_slice()].concat(),
        [CLOSE_DOUBLE.as_slice(), CLOSE_QUOTE.as_slice()].concat(),
    ] {
        while let Some(index) = find_byte_sequence(&transformed, &pattern) {
            transformed.drain(index + CLOSE_PAREN.len()..index + pattern.len());
            close = true;
        }
    }
    let doubled_close = [CLOSE_QUOTE.as_slice(), CLOSE_QUOTE.as_slice()].concat();
    while let Some(index) = find_byte_sequence(&transformed, &doubled_close) {
        transformed.drain(index..index + doubled_close.len());
        open = true;
    }
    let doubled_open = [OPEN_QUOTE.as_slice(), OPEN_QUOTE.as_slice()].concat();
    while let Some(index) = find_byte_sequence(&transformed, &doubled_open) {
        transformed.drain(index..index + doubled_open.len());
        close = true;
    }
    InlineRecordMarkers {
        open: open
            || transformed.windows(OPEN_QUOTE.len()).any(|window| window == OPEN_QUOTE),
        close: close
            || transformed.windows(CLOSE_QUOTE.len()).any(|window| window == CLOSE_QUOTE),
    }
}
fn find_byte_sequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}
impl Vm {
    pub fn new() -> Self {
        let mut stack = Stack::new();
        stack.push(Value::int(0));
        let rng_state = crate::SystemTime::now()
            .duration_since(crate::SystemTime::UNIX_EPOCH)
            .map(|duration| duration.as_secs() as u32)
            .unwrap_or(1);
        Self {
            scripts: Vec::new(),
            frames: Vec::new(),
            stack,
            globals: Vec::new(),
            locals: Vec::new(),
            threads: Vec::new(),
            global_keys: std::collections::HashMap::new(),
            local_keys: std::collections::HashMap::new(),
            thread_keys: std::collections::HashMap::new(),
            rng_state,
            ip_trace: std::collections::VecDeque::new(),
            derail_dumped: false,
            text: TextVmState::default(),
            host_funcs: std::collections::HashMap::new(),
            suspended_ctxs: vec![None],
            current_ctx: 0,
            active_function_id: 0,
            active_parent_function_id: None,
            active_state: CtxState::Running,
            active_phase: CtxPhase::Initial,
            active_finalizer_target: None,
            next_function_id: 1,
            scene_transition_requested: false,
            scene_cycle_preserve_audio: false,
            scheduler_no_present: false,
            pending_save_snapshot: None,
            pending_load: None,
            quit_requested: false,
            save_description: [0; 128],
            pending_hotspot_callback: None,
            pending_inner_host_bridges: Vec::new(),
            input_suppressed: false,
            native_mode_bits: 0,
            native_mode_special: false,
            native_mode_counter: 0,
            native_skip_mode: false,
            readmarks: std::collections::HashMap::new(),
            native_mode_restore_auto: false,
            schedule_wall_slice: None,
        }
    }
    pub fn take_scene_cycle_preserve_audio(&mut self) -> bool {
        std::mem::replace(&mut self.scene_cycle_preserve_audio, false)
    }
    pub fn reset_for_outer_scene_cycle(&mut self) -> usize {
        let retired_callbacks = self.cleanup_unpinned_host_funcs();
        self.frames.clear();
        self.stack = Stack::new();
        self.stack.push(Value::int(0));
        self.locals.clear();
        self.local_keys.clear();
        self.threads.clear();
        self.thread_keys.clear();
        self.text = TextVmState::default();
        self.suspended_ctxs = vec![None];
        self.current_ctx = 0;
        self.active_function_id = 0;
        self.active_parent_function_id = None;
        self.active_state = CtxState::Running;
        self.active_phase = CtxPhase::Initial;
        self.active_finalizer_target = None;
        self.next_function_id = 1;
        self.scene_transition_requested = false;
        self.scheduler_no_present = false;
        self.pending_save_snapshot = None;
        self.pending_load = None;
        self.quit_requested = false;
        self.save_description.fill(0);
        self.pending_hotspot_callback = None;
        self.pending_inner_host_bridges.clear();
        self.input_suppressed = false;
        self.native_mode_bits = 0;
        self.native_mode_special = false;
        self.native_mode_counter = 0;
        self.native_skip_mode = false;
        self.native_mode_restore_auto = false;
        retired_callbacks
    }
    pub fn restore_system_globals<I>(&mut self, entries: I)
    where
        I: IntoIterator<Item = (u32, Value)>,
    {
        self.globals.clear();
        self.global_keys.clear();
        for (key, mut value) in entries {
            value.scope = Scope::Global;
            let slot = self.globals.len();
            self.globals.push(value);
            self.global_keys.entry(key).or_insert(slot);
        }
    }
    pub fn system_global_value(&self, key: u32) -> Option<&Value> {
        self.global_keys.get(&key).and_then(|slot| self.globals.get(*slot))
    }
    pub fn set_system_global_int(&mut self, key: u32, value: i32) {
        self.write_var(Scope::Global, key, 0, Value::int(value));
    }
    pub fn load_script(&mut self, code: Vec<u8>) -> usize {
        self.load_script_with_entries(code, Vec::new())
    }
    pub fn load_script_with_entries(
        &mut self,
        code: Vec<u8>,
        entries: Vec<ScriptEntry>,
    ) -> usize {
        self.load_script_with_entries_named(code, entries, None)
    }
    pub fn load_script_with_entries_named(
        &mut self,
        code: Vec<u8>,
        entries: Vec<ScriptEntry>,
        name: Option<Vec<u8>>,
    ) -> usize {
        self.load_script_with_entries_named_readmarks(code, entries, name, 0)
    }
    pub fn load_script_with_entries_named_readmarks(
        &mut self,
        code: Vec<u8>,
        entries: Vec<ScriptEntry>,
        name: Option<Vec<u8>>,
        readmark_count: u32,
    ) -> usize {
        let code_crc32 = formats::crypto::crc32(&code);
        self.load_script_with_entries_named_readmarks_crc(
            code,
            entries,
            name,
            readmark_count,
            code_crc32,
        )
    }
    pub fn load_resolved_script(&mut self, script: crate::host::LoadedScript) -> usize {
        let entries = script
            .entries
            .into_iter()
            .map(|(name_hash, offset)| ScriptEntry { name_hash, offset })
            .collect();
        self.load_script_with_entries_named_readmarks_crc(
            script.code,
            entries,
            Some(script.name.into_bytes()),
            script.line_count,
            script.code_crc32,
        )
    }
    pub(crate) fn load_script_with_entries_named_readmarks_crc(
        &mut self,
        code: Vec<u8>,
        entries: Vec<ScriptEntry>,
        name: Option<Vec<u8>>,
        readmark_count: u32,
        code_crc32: u32,
    ) -> usize {
        if let Some(name) = name.as_deref() {
            self.prepare_readmark_record(name, readmark_count);
        }
        let idx = self.scripts.len();
        self.scripts
            .push(LoadedCode {
                code: code.into_boxed_slice(),
                code_crc32,
                entries,
                name,
                readmark_count,
            });
        idx
    }
    pub fn script_code(&self, idx: usize) -> &[u8] {
        &self.scripts[idx].code
    }
    pub fn script_entries(&self, idx: usize) -> &[ScriptEntry] {
        &self.scripts[idx].entries
    }
    pub fn restore_readmarks(&mut self, records: Vec<formats::save::ReadmarkRecord>) {
        self.readmarks.clear();
        for mut record in records {
            let key = canonical_readmark_name(&record.name);
            write_readmark_name(&mut record.name, &key);
            self.readmarks.insert(key, record);
        }
    }
    pub(crate) fn snapshot_readmarks(&self) -> Vec<formats::save::ReadmarkRecord> {
        let mut records = self.readmarks.values().cloned().collect::<Vec<_>>();
        records.sort_unstable_by_key(|record| record.name);
        records
    }
    pub(crate) fn has_readmark_script(&self, name: &[u8]) -> bool {
        let key = canonical_readmark_name(name);
        if self.readmarks.contains_key(&key) {
            return true;
        }
        let legacy_key = legacy_readmark_name(name);
        legacy_key != key && self.readmarks.contains_key(&legacy_key)
    }
    fn prepare_readmark_record(&mut self, name: &[u8], bit_count: u32) {
        let key = canonical_readmark_name(name);
        if bit_count == 0 {
            self.readmarks.remove(&key);
            return;
        }
        let byte_count = bit_count.div_ceil(8) as usize;
        let required_size = byte_count + 16;
        let valid = self
            .readmarks
            .get(&key)
            .is_some_and(|record| {
                record.bit_count == bit_count && record.data.len() >= byte_count
            });
        if valid {
            return;
        }
        let mut record_name = [0u8; formats::save::SCRIPT_NAME_SIZE];
        write_readmark_name(&mut record_name, &key);
        self.readmarks
            .insert(
                key,
                formats::save::ReadmarkRecord {
                    name: record_name,
                    bit_count,
                    data: vec![0; required_size],
                },
            );
    }
    fn mark_current_read(&mut self, script_idx: usize, flag: u16) {
        if self.active_parent_function_id.is_some() {
            return;
        }
        let Some(script) = self.scripts.get(script_idx) else {
            return;
        };
        if script.readmark_count == 0 {
            if self.active_function_id == 0 {
                self.native_skip_mode = false;
            }
            return;
        }
        let Some(name) = script.name.as_deref() else {
            self.native_skip_mode = false;
            return;
        };
        let key = canonical_readmark_name(name);
        let Some(record) = self.readmarks.get_mut(&key) else {
            self.native_skip_mode = false;
            return;
        };
        let bit = flag as usize;
        let byte = bit / 8;
        let mask = 1u8 << (bit & 7);
        let Some(cell) = record.data.get_mut(byte) else {
            self.native_skip_mode = false;
            return;
        };
        self.native_skip_mode = *cell & mask != 0;
        *cell |= mask;
    }
    pub fn run_entry<H: Host>(
        &mut self,
        script_idx: usize,
        entry_offset: usize,
        label: impl Into<String>,
        host: &mut H,
    ) -> Result<StepResult, VmError> {
        self.push_frame(script_idx, entry_offset, label)?;
        self.run(host)
    }
    pub fn push_frame(
        &mut self,
        script_idx: usize,
        entry_offset: usize,
        label: impl Into<String>,
    ) -> Result<(), VmError> {
        let code_ptr: *const [u8] = self.scripts[script_idx].code.as_ref();
        let code: &'static [u8] = unsafe { &*code_ptr };
        if entry_offset > code.len() {
            return Err(
                VmError::Other(
                    format!(
                        "entry offset 0x{:X} beyond code len 0x{:X}", entry_offset, code
                        .len()
                    ),
                ),
            );
        }
        let entry_sp = self.stack.len();
        self.frames
            .push(CallFrame {
                cursor: Cursor::new(code, entry_offset),
                script_idx,
                entry_sp,
                stack_base: entry_sp,
                cleanup_mode: 0,
                script_marker: 0,
                param_count: 0,
                label: label.into(),
                jump_target_a: None,
                jump_target_b: None,
                jump_target_c: None,
                frame_advance_target: None,
                transition_target: None,
                force_return: false,
                wait_deadline: WaitDeadline::None,
                wait_present_epoch: None,
                text_line_pending: false,
            });
        Ok(())
    }
    pub fn run<H: Host>(&mut self, host: &mut H) -> Result<StepResult, VmError> {
        loop {
            match self.step(host)? {
                StepResult::Continue => continue,
                other => return Ok(other),
            }
        }
    }
    pub fn resume_with_input<H: Host>(
        &mut self,
        input: crate::host::Input,
        host: &mut H,
    ) {
        let focus_lost = matches!(& input, crate ::host::Input::Focused(false));
        let pointer_interrupt = matches!(
            & input, crate ::host::Input::PointerButton { button : crate
            ::host::PointerButton::Left | crate ::host::PointerButton::Middle, pressed :
            true, .. }
        );
        let key_edge = match &input {
            crate::host::Input::Key { virtual_key, pressed } => {
                Some((*virtual_key, *pressed))
            }
            _ => None,
        };
        if pointer_interrupt && !self.native_mode_special {
            self.native_mode_bits = 0;
            self.native_mode_restore_auto = false;
            host.set_native_mode_flags(0, false);
        }
        host.deliver_input(input);
        if focus_lost {
            self.native_mode_bits = 0;
            self.native_mode_special = false;
            self.native_mode_restore_auto = false;
            host.set_native_mode_flags(0, false);
        }
        if let Some((virtual_key, pressed)) = key_edge {
            let input_active = !self.input_suppressed
                || host.input_suppression_bypassed();
            let next_bits = match (virtual_key, pressed) {
                (0x11, true) if input_active => {
                    self.native_mode_restore_auto |= self.native_mode_bits & 0b100 != 0;
                    if host.config_load(b"CTRLKEYDEF", 0) != 0
                        && host.config_load(b"FastModeFlg_AR", 0) != 0
                    {
                        Some(0b010)
                    } else {
                        Some(0b001)
                    }
                }
                (0x10, true) if input_active && !host.key_capslock_on() => {
                    self.native_mode_restore_auto |= self.native_mode_bits & 0b100 != 0;
                    Some(0b010)
                }
                (0x14, true) => {
                    self.native_mode_restore_auto = false;
                    Some(0)
                }
                (0x10, false) | (0x11, false) => {
                    let bits = if self.native_mode_restore_auto { 0b100 } else { 0 };
                    self.native_mode_restore_auto = false;
                    Some(bits)
                }
                _ => None,
            };
            if let Some(bits) = next_bits {
                self.native_mode_bits = bits;
                self.native_mode_special = false;
                host.set_native_mode_flags(bits, false);
            }
        }
    }
    pub(crate) fn native_effect_skip_active<H: Host>(&mut self, host: &mut H) -> bool {
        self.native_mode_pending_exact(host)
    }
    pub(crate) fn native_mode_pending_exact<H: Host>(&mut self, host: &mut H) -> bool {
        if self.native_mode_counter != 0 {
            return false;
        }
        if self.native_mode_bits & 0b001 != 0 || self.native_mode_special {
            return true;
        }
        let input_active = !self.input_suppressed || host.input_suppression_bypassed();
        let caps = input_active && host.key_capslock_on();
        let caps_mode = host.config_load(b"CAPSMODE", 0) == 1;
        let message_window_status = !caps_mode && caps;
        let system_caps_status = caps_mode && caps;
        if !message_window_status && self.native_mode_bits & 0b010 == 0 {
            return (system_caps_status || self.native_mode_bits & 0b100 != 0)
                && host.input_dispatch_mode() != 0;
        }
        if !self.native_skip_mode {
            self.native_mode_bits &= !0b010;
            host.set_native_mode_flags(self.native_mode_bits, self.native_mode_special);
            return false;
        }
        if !message_window_status || !host.input_is_held_or_recent(131) {
            return true;
        }
        for index in [131, 130, 136] {
            host.input_consume_recent_press(index);
        }
        self.native_mode_bits &= !0b010;
        host.set_native_mode_flags(self.native_mode_bits, self.native_mode_special);
        host.set_capslock_state(false);
        host.request_present();
        false
    }
    pub fn native_fast_forward_active(&self) -> bool {
        self.native_mode_counter == 0
            && (self.native_mode_bits & 0b001 != 0 || self.native_mode_special)
    }
    fn apply_frame_advance_target(&mut self) {
        for frame in self.frames.iter_mut().rev() {
            let Some(target) = frame.frame_advance_target else {
                continue;
            };
            if target <= frame.cursor.ip {
                continue;
            }
            frame.cursor.ip = target;
            frame.frame_advance_target = None;
            frame.wait_deadline = WaitDeadline::None;
            break;
        }
    }
    pub fn step<H: Host>(&mut self, host: &mut H) -> Result<StepResult, VmError> {
        if self.frames.last().is_some_and(|frame| frame.force_return) {
            return self.exec_ret();
        }
        let frame = match self.frames.last_mut() {
            Some(f) => f,
            None => return Ok(StepResult::Exited),
        };
        let start_ip = frame.cursor.ip;
        let script_idx = frame.script_idx;
        let context_id = self.active_function_id;
        self.ip_trace.push_back((script_idx, start_ip));
        if self.ip_trace.len() > 48 {
            self.ip_trace.pop_front();
        }
        let opcode = frame.cursor.read_u16()?;
        let mut apply_frame_advance = false;
        let result = match opcode {
            0x800 => {
                let v = frame.cursor.read_i32()?;
                self.stack.push(Value::int(v));
                Ok(StepResult::Continue)
            }
            0x803 => {
                let v = frame.cursor.read_f32()?;
                self.stack.push(Value::float(v));
                Ok(StepResult::Continue)
            }
            0x801 => {
                let len = frame.cursor.read_u16()? as usize;
                let bytes = frame.cursor.read_bytes(len)?;
                self.stack.push(Value::string(bytes.to_vec()));
                Ok(StepResult::Continue)
            }
            0x82F => {
                self.stack.pop().ok_or(VmError::StackUnderflow)?;
                Ok(StepResult::Continue)
            }
            0x82C => {
                let off = frame.cursor.read_i32()?;
                let target = (frame.cursor.ip as isize + off as isize) as usize;
                let len = frame.cursor.code.len();
                if target > len {
                    return Err(VmError::InvalidJump {
                        from: start_ip,
                        target,
                        len,
                    });
                }
                frame.cursor.ip = target;
                Ok(StepResult::Continue)
            }
            0x82D | 0x82E => {
                let off = frame.cursor.read_i32()?;
                let v = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let cond = v.truthy();
                let cond = if opcode == 0x82D { cond } else { !cond };
                if cond {
                    let target = (frame.cursor.ip as isize + off as isize) as usize;
                    let len = frame.cursor.code.len();
                    if target > len {
                        return Err(VmError::InvalidJump {
                            from: start_ip,
                            target,
                            len,
                        });
                    }
                    frame.cursor.ip = target;
                }
                Ok(StepResult::Continue)
            }
            0x80F | 0x810 => {
                let cleanup_mode: u32 = if opcode == 0x80F { 0 } else { 1 };
                let hash = frame.cursor.read_u32()?;
                let cached_offset = frame.cursor.read_u32()?;
                let param_count = frame.cursor.read_i16()?;
                let caller_script = frame.script_idx;
                let caller_return_ip = frame.cursor.ip;
                let caller_ip_at_instr = start_ip;
                let caller_sp = self.stack.len();
                if hash == 0xE9E9_9F12 {
                    crate::text_trace!(
                        "[VM] BIRTHDAY_DIALOG_CALL ctx=0x{context_id:08X} script={caller_script} ip=0x{start_ip:X}"
                    );
                }
                let (target_script, target_offset) = if cached_offset != 0
                    && (cached_offset as usize) < self.script_code(caller_script).len()
                {
                    (caller_script, cached_offset as usize)
                } else {
                    self.resolve_entry_global(hash)
                        .ok_or({
                            VmError::UnknownEntryHash {
                                hash,
                                from_script: caller_script,
                                ip: caller_ip_at_instr,
                            }
                        })?
                };
                self.exec_call(
                    target_script,
                    caller_return_ip,
                    target_offset,
                    caller_sp,
                    cleanup_mode,
                    param_count,
                )?;
                Ok(StepResult::Continue)
            }
            0x82B => self.exec_ret(),
            0x834 | 0x835 => {
                let hash = frame.cursor.read_u32()?;
                let count = frame.cursor.read_i16()? as usize;
                let result = self
                    .dispatch_inner(hash, count, opcode == 0x835, host, start_ip)?;
                Ok(result)
            }
            0x830 => {
                let off = frame.cursor.read_i32()?;
                let next_ip = frame.cursor.ip;
                let code_len = frame.cursor.code.len();
                let marked = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                self.write_var(
                    crate::value::Scope::Thread,
                    crate::exec::MARK_SYSVAR_HASH,
                    0,
                    marked,
                );
                let target = (next_ip as isize + off as isize) as usize;
                if target > code_len {
                    return Err(VmError::InvalidJump {
                        from: start_ip,
                        target,
                        len: code_len,
                    });
                }
                self.frames.last_mut().expect("frame exists").cursor.ip = target;
                Ok(StepResult::Continue)
            }
            0x831..=0x833 => {
                let off = frame.cursor.read_i32()?;
                let next_ip = frame.cursor.ip;
                let code_len = frame.cursor.code.len();
                let sysvar = self
                    .read_var(
                        crate::value::Scope::Thread,
                        crate::exec::MARK_SYSVAR_HASH,
                        0,
                    )
                    .unwrap_or_else(Value::null);
                let top = match self.stack.peek() {
                    Some(v) => v.clone(),
                    None => return Err(VmError::StackUnderflow),
                };
                use crate::exec::handlers::helpers::{values_compare, values_not_equal};
                let cond = match opcode {
                    0x831 => values_not_equal(&top, &sysvar),
                    0x832 => values_compare(&top, &sysvar) <= 0,
                    0x833 => values_compare(&top, &sysvar) >= 0,
                    _ => unreachable!(),
                };
                if cond {
                    let target = (next_ip as isize + off as isize) as usize;
                    if target > code_len {
                        return Err(VmError::InvalidJump {
                            from: start_ip,
                            target,
                            len: code_len,
                        });
                    }
                    self.frames.last_mut().expect("frame exists").cursor.ip = target;
                }
                Ok(StepResult::Continue)
            }
            op if (0x100..=0x188).contains(&op) => self.exec_stack_binop(op),
            op if (0x190..=0x1A9).contains(&op) => self.exec_stack_unop(op),
            op if (0x1B0..=0x200).contains(&op) || (0x210..=0x260).contains(&op) => {
                let tag = frame.cursor.read_u16()?;
                let imm = frame.cursor.read_u32()?;
                let idx = frame.cursor.read_u16()? as usize;
                let pop = op >= 0x210;
                self.exec_imm_arith(op, tag, imm, idx, pop)
            }
            op if (0x270..=0x300).contains(&op) => {
                let tag = frame.cursor.read_u16()?;
                let imm = frame.cursor.read_u32()?;
                let idx = frame.cursor.read_u16()? as usize;
                self.exec_ext_store(op, tag, imm, idx)
            }
            0x836 => {
                let count = frame.cursor.read_u16()? as usize;
                let type_bytes = frame.cursor.read_bytes(count)?.to_vec();
                self.exec_sys_02(count, &type_bytes)?;
                Ok(StepResult::Continue)
            }
            0x837 => {
                let tag = frame.cursor.read_u16()?;
                let imm = frame.cursor.read_u32()?;
                let idx = frame.cursor.read_u16()? as usize;
                self.exec_sys_array_atomic(tag, imm, idx)
            }
            0x838 | 0x839 => {
                let off = frame.cursor.read_i32()?;
                self.exec_sys_cmp_sysvar(opcode, off, start_ip)
            }
            0x83A => {
                let flag = frame.cursor.read_u16()?;
                frame.script_marker = u32::from(flag);
                let script_idx = frame.script_idx;
                self.mark_current_read(script_idx, flag);
                host.sys_skip_logic(flag);
                Ok(StepResult::Continue)
            }
            0x83B..=0x83D => {
                let off = frame.cursor.read_i32()?;
                let target = (frame.cursor.ip as isize + off as isize) as usize;
                let (pending_bit, target_slot) = match opcode {
                    0x83B => (0b001, &mut frame.jump_target_a),
                    0x83C => (0b100, &mut frame.jump_target_c),
                    0x83D => (0b010, &mut frame.jump_target_b),
                    _ => unreachable!(),
                };
                *target_slot = Some(target);
                let pending = host.scheduler_button_latches(context_id) & 0b111;
                crate::text_trace!(
                    "[VM] INPUT_TARGET ctx=0x{context_id:08X} script={} ip=0x{start_ip:X} opcode=0x{opcode:03X} target=0x{target:X} pending=0b{pending:03b}",
                    frame.script_idx
                );
                if pending & pending_bit != 0 {
                    frame.cursor.ip = target;
                    *target_slot = None;
                    frame.wait_deadline = WaitDeadline::None;
                    host.consume_scheduler_button_latches(context_id, pending_bit);
                    crate::text_trace!(
                        "[VM] INPUT_TARGET_IMMEDIATE ctx=0x{context_id:08X} consumed=0b{pending_bit:03b} -> 0x{target:X}"
                    );
                }
                Ok(StepResult::Continue)
            }
            0x83E => {
                let top = self.stack.peek_mut().ok_or(VmError::StackUnderflow)?;
                if top.type_tag == crate::value::TAG_FLOAT {
                    let f = f32::from_bits(top.bits);
                    top.bits = (f as i32) as u32;
                }
                top.type_tag = crate::value::TAG_INT;
                top.data = None;
                Ok(StepResult::Continue)
            }
            0x83F => {
                let top = self.stack.peek_mut().ok_or(VmError::StackUnderflow)?;
                if top.type_tag == crate::value::TAG_INT {
                    let i = top.bits as i32;
                    top.bits = (i as f32).to_bits();
                }
                top.type_tag = crate::value::TAG_FLOAT;
                top.data = None;
                Ok(StepResult::Continue)
            }
            0x840 => {
                let stride = frame.cursor.read_u16()? as usize;
                let source_bytes = frame.cursor.read_bytes(stride)?.to_vec();
                let (decoded, _, _) = encoding_rs::SHIFT_JIS.decode(&source_bytes);
                crate::text_trace!(
                    "[VM] TEXT_LINE ctx=0x{context_id:08X} script={} ip=0x{start_ip:X} text={decoded:?}",
                    frame.script_idx
                );
                frame.text_line_pending = true;
                self.text.append_render_line(&source_bytes);
                Ok(StepResult::Continue)
            }
            0x841 => {
                crate::text_trace!(
                    "[VM] TEXT_RENDER ctx=0x{context_id:08X} script={} ip=0x{start_ip:X} first_render={} render_state={} pending_bytes={}",
                    frame.script_idx, self.text.first_render, self.text.render_state,
                    self.text.pending_render_line.len()
                );
                let history_key = (frame.script_idx, start_ip);
                if !self.text.pending_render_line.is_empty()
                    && self.text.last_history_render != Some(history_key)
                    && self.text.history_capture_enabled()
                {
                    let history = self.text.pending_render_line.clone();
                    self.text.append_history_record(&history);
                    self.text.last_history_render = Some(history_key);
                }
                if !self.text.first_render {
                    self.frames.last_mut().expect("frame exists").cursor.ip = start_ip;
                    self.invoke_host_func(
                        0xF7A4_C8D8,
                        &[Value::int(0), Value::int(0)],
                        false,
                    )?;
                    self.text.first_render = true;
                    return Ok(StepResult::Continue);
                }
                if let Some((name, remainder)) = split_name_popup(
                    &self.text.pending_render_line,
                ) {
                    self.text.pending_render_line = remainder;
                    self.frames.last_mut().expect("frame exists").cursor.ip = start_ip;
                    self.invoke_host_func(0x44A4_FF72, &[Value::string(name)], false)?;
                    return Ok(StepResult::Continue);
                }
                if !self.text.pending_render_line.is_empty() {
                    let bytes = std::mem::take(&mut self.text.pending_render_line);
                    self.text.last_history_render = None;
                    let script = &self.scripts[script_idx];
                    let site = crate::host::TextSite {
                        script_name: script.name.clone().unwrap_or_default(),
                        render_offset: start_ip,
                        code_crc32: script.code_crc32,
                    };
                    self.capture_text_accumulator(&bytes);
                    let start_y = host.get_text_pos_y();
                    let markers = inline_record_markers(&bytes);
                    host.text_line(&site, &bytes);
                    self.text
                        .update_inline_records(
                            markers,
                            start_y,
                            host.get_text_pos_y(),
                            host.text_line_height(),
                        );
                }
                let render_info = host.text_render();
                crate::text_trace!("[VM] TEXT_RENDER_RESULT {render_info:?}");
                if let Some(mut info) = render_info {
                    info.render_state = self.text.render_state;
                    if info.action == crate::host::TextRenderAction::PageOverflow {
                        self.frames.last_mut().expect("frame exists").cursor.ip = start_ip;
                        self.invoke_host_func(0x93B3_8A0B, &[], false)?;
                        return Ok(StepResult::Continue);
                    }
                    if info.action == crate::host::TextRenderAction::Continue {
                        self.frames.last_mut().expect("frame exists").cursor.ip = start_ip;
                    }
                    self.invoke_host_func(
                        0x47BB_540C,
                        &[
                            Value::int(info.render_state),
                            Value::int(info.rect[3]),
                            Value::int(info.rect[2]),
                            Value::int(info.rect[1]),
                            Value::int(info.rect[0]),
                            Value::int(info.page as i32),
                        ],
                        false,
                    )?;
                }
                Ok(StepResult::Continue)
            }
            0x842 => {
                let stride = frame.cursor.read_u16()? as usize;
                let fmt = frame.cursor.read_bytes(stride)?.to_vec();
                self.exec_sys_0e(host, &fmt)?;
                Ok(StepResult::Continue)
            }
            0x843 => {
                let off = frame.cursor.read_i32()?;
                let target = (frame.cursor.ip as isize + off as isize) as usize;
                frame.jump_target_a = Some(target);
                frame.jump_target_b = Some(target);
                frame.jump_target_c = Some(target);
                let pending = host.scheduler_button_latches(context_id) & 0b111;
                if pending & 0b001 != 0 {
                    frame.cursor.ip = target;
                    frame.jump_target_a = None;
                    frame.wait_deadline = WaitDeadline::None;
                    host.consume_scheduler_button_latches(context_id, 0b001);
                }
                if pending & 0b100 != 0 {
                    frame.cursor.ip = target;
                    frame.jump_target_c = None;
                    frame.wait_deadline = WaitDeadline::None;
                    host.consume_scheduler_button_latches(context_id, 0b100);
                }
                if pending & 0b010 != 0 {
                    frame.cursor.ip = target;
                    frame.jump_target_b = None;
                    frame.wait_deadline = WaitDeadline::None;
                    host.consume_scheduler_button_latches(context_id, 0b010);
                }
                Ok(StepResult::Continue)
            }
            0x844 => {
                frame.jump_target_a = None;
                frame.jump_target_b = None;
                frame.jump_target_c = None;
                Ok(StepResult::Continue)
            }
            0x845 => {
                let off = frame.cursor.read_i32()?;
                let target = (frame.cursor.ip as isize + off as isize) as usize;
                frame.frame_advance_target = Some(target);
                Ok(StepResult::Continue)
            }
            0x846 => {
                apply_frame_advance = true;
                Ok(StepResult::Continue)
            }
            0x847 => {
                let off = frame.cursor.read_i32()?;
                let target = (frame.cursor.ip as isize + off as isize) as usize;
                let len = frame.cursor.code.len();
                if target > len {
                    return Err(VmError::InvalidJump {
                        from: start_ip,
                        target,
                        len,
                    });
                }
                frame.transition_target = Some(target);
                frame.wait_deadline = WaitDeadline::None;
                Ok(StepResult::Continue)
            }
            0x850 => {
                let count = frame.cursor.read_u16()? as usize;
                let index = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let mut idx = index.as_int().unwrap_or(0);
                if idx < 0 || idx as usize >= count {
                    idx = (count as i32) - 1;
                }
                let off_pos = frame.cursor.ip + 4 * (idx as usize);
                let off = frame.cursor.read_i32_at(off_pos)?;
                frame.cursor.ip = off_pos + 4;
                let target = (frame.cursor.ip as isize + off as isize) as usize;
                let len = frame.cursor.code.len();
                if target > len {
                    return Err(VmError::InvalidJump {
                        from: start_ip,
                        target,
                        len,
                    });
                }
                frame.cursor.ip = target;
                Ok(StepResult::Continue)
            }
            0x802 => {
                let tag = frame.cursor.read_u16()?;
                let imm = frame.cursor.read_u32()?;
                let idx = frame.cursor.read_u16()? as usize;
                let outcome = self.exec_push_op(tag, imm, idx);
                if script_idx == 6 && (0xB537..0xB65D).contains(&start_ip) {
                    eprintln!(
                        "[PAUSE-DIAG] ip=0x{start_ip:X} var=0x{imm:08X} tag=0x{tag:04X} idx={} -> {:?}",
                        idx as i16, self.stack.peek()
                    );
                }
                outcome
            }
            0x829 => {
                let count = frame.cursor.read_u16()? as usize;
                let bytes = frame.cursor.read_bytes(count)?.to_vec();
                self.exec_push_bytes(&bytes)
            }
            _ => {
                let name = if is_valid_outer_opcode(opcode) {
                    "valid-unknown"
                } else {
                    "invalid"
                };
                if !self.derail_dumped {
                    self.derail_dumped = true;
                    self.dump_derail_context(script_idx, start_ip, opcode);
                }
                Err(VmError::Unimplemented {
                    opcode_or_hash: opcode as u32,
                    detail: format!("opcode {} ({})", opcode, name),
                    ip: start_ip,
                })
            }
        };
        if result.is_ok() && apply_frame_advance {
            self.apply_frame_advance_target();
        }
        if result.is_ok() && self.scene_transition_requested {
            self.apply_scene_transition();
        }
        result
    }
    fn dump_derail_context(&self, script_idx: usize, start_ip: usize, opcode: u16) {
        use std::fmt::Write as _;
        let mut text = String::new();
        let _ = writeln!(
            text,
            "[VM-DERAIL] first invalid opcode 0x{opcode:04X} at script={script_idx} ip=0x{start_ip:X}"
        );
        let _ = write!(text, "[VM-DERAIL] frames root→current:");
        for (depth, frame) in self.frames.iter().enumerate() {
            let name = self
                .scripts
                .get(frame.script_idx)
                .and_then(|script| script.name.as_ref())
                .map(|name| String::from_utf8_lossy(name).into_owned())
                .unwrap_or_default();
            let _ = write!(
                text, " | #{} s{}({}) {} ip=0x{:X}", depth, frame.script_idx, name, frame
                .label, frame.cursor.ip
            );
        }
        crate::diag::emit_log(format_args!("{text}"));
        let mut trace = String::from("[VM-DERAIL] recent ips:");
        for (script, ip) in &self.ip_trace {
            let _ = write!(trace, " s{script}:0x{ip:X}");
        }
        crate::diag::emit_log(format_args!("{trace}"));
    }
    fn exec_sys_0e<H: Host>(&mut self, host: &mut H, fmt: &[u8]) -> Result<(), VmError> {
        use crate::text::{ControlCode, ControlCodeKind};
        let Some(&code) = fmt.first() else {
            host.sys_log_format(fmt);
            return Ok(());
        };
        crate::text_trace!(
            "[VM] SYS_0E ctx=0x{:08X} ip={:?} code={} first_render={} render_state={}",
            self.active_function_id, self.current_ip(), char::from(code), self.text
            .first_render, self.text.render_state
        );
        let pop = |vm: &mut Self| vm.stack.pop().ok_or(VmError::StackUnderflow);
        let pop_int = |vm: &mut Self| -> Result<i32, VmError> {
            let value = pop(vm)?;
            Ok(
                value
                    .as_int()
                    .or_else(|| value.as_float().map(|v| v as i32))
                    .unwrap_or(0),
            )
        };
        match code {
            b'c' => {
                let background = pop_int(self)?;
                let foreground = pop_int(self)? as u32;
                self.text
                    .append_history_record(
                        format!("\\c0x{foreground:08x}0x{:08x}", background as u32)
                            .as_bytes(),
                    );
                host.text_set_colors(foreground, background);
            }
            b'f' => {
                let flags = pop_int(self)?;
                let face = pop(self)?.as_str_bytes().unwrap_or(&[]).to_vec();
                let line_height = pop_int(self)?;
                let width = pop_int(self)?;
                let size = pop_int(self)?;
                let mut history = format!(
                    "\\f0x{:08x}0x{:08x}0x{:08x}0x{:08x}", size as u32, width as u32,
                    line_height as u32, flags as u32
                )
                    .into_bytes();
                history
                    .extend_from_slice(
                        face.split(|byte| *byte == 0).next().unwrap_or(&face),
                    );
                self.text.append_history_record(&history);
                host.text_set_font(size, width, line_height, flags, &face);
            }
            b'g' => {
                let mut fields = [0i32; 6];
                for field in fields.iter_mut().rev() {
                    *field = pop_int(self)?;
                }
                let mut payload = Vec::with_capacity(60);
                for field in fields {
                    payload
                        .extend_from_slice(format!("0x{:08x}", field as u32).as_bytes());
                }
                let mut history = Vec::with_capacity(payload.len() + 2);
                history.extend_from_slice(b"\\g");
                history.extend_from_slice(&payload);
                self.text.append_history_record(&history);
                host.text_control(
                    &ControlCode {
                        kind: ControlCodeKind::Graphics,
                        payload,
                    },
                );
                let forwarded = fields
                    .into_iter()
                    .rev()
                    .filter(|field| *field != -99)
                    .map(Value::int)
                    .collect::<Vec<_>>();
                self.invoke_host_func(0xD3CB_0EE5, &forwarded, false)?;
            }
            b'l' => {
                let y = pop_int(self)?;
                let x = pop_int(self)?;
                self.text
                    .append_history_record(
                        format!("\\l0x{:08x}0x{:08x}", x as u32, y as u32).as_bytes(),
                    );
                host.text_set_position(x, y);
            }
            b'o' => {
                let y = pop_int(self)?;
                let x = pop_int(self)?;
                let payload = two_hex_fields(x, y);
                let mut history = Vec::with_capacity(payload.len() + 2);
                history.extend_from_slice(b"\\o");
                history.extend_from_slice(&payload);
                self.text.append_history_record(&history);
                host.text_control(
                    &ControlCode {
                        kind: ControlCodeKind::OffsetSave,
                        payload,
                    },
                );
            }
            b's' | b't' => {
                let value = pop_int(self)?;
                let payload = format!("0x{:08x}", value as u32).into_bytes();
                let mut history = Vec::with_capacity(payload.len() + 2);
                history.push(b'\\');
                history.push(code);
                history.extend_from_slice(&payload);
                self.text.append_history_record(&history);
                host.text_control(
                    &ControlCode {
                        kind: if code == b's' {
                            ControlCodeKind::Speed
                        } else {
                            ControlCodeKind::Delay
                        },
                        payload,
                    },
                );
            }
            b'x' => {
                let name = pop(self)?.as_str_bytes().unwrap_or(&[]).to_vec();
                let mut history = Vec::with_capacity(name.len() + 2);
                history.extend_from_slice(b"\\x");
                history
                    .extend_from_slice(
                        name.split(|byte| *byte == 0).next().unwrap_or(&name),
                    );
                self.text.append_history_record(&history);
                host.text_control(
                    &ControlCode {
                        kind: ControlCodeKind::ExecScript,
                        payload: name.clone(),
                    },
                );
                let scheduled = self
                    .invoke_host_func(
                        X_CONTROL_NAME_HASH,
                        &[Value::string(name.clone())],
                        false,
                    )?;
                crate::text_trace!(
                    "[VM] SYS_0E_X_BRIDGE arg={:?} hash=0x{X_CONTROL_NAME_HASH:08X} scheduled={scheduled}",
                    String::from_utf8_lossy(name.split(| byte | * byte == 0).next()
                    .unwrap_or(& name))
                );
            }
            b'd' => {
                let value = pop(self)?;
                let bytes = if let Some(s) = value.as_str_bytes() {
                    s.to_vec()
                } else if let Some(f) = value.as_float() {
                    format!("{f}").into_bytes()
                } else {
                    value.as_int().unwrap_or(0).to_string().into_bytes()
                };
                self.text.append_render_line(&bytes);
                if let Some(frame) = self.frames.last_mut() {
                    frame.text_line_pending = true;
                }
            }
            other => {
                let Some(kind) = ControlCodeKind::from_byte(other) else {
                    host.sys_log_format(fmt);
                    return Ok(());
                };
                let segment = fmt.split(|byte| *byte == 0).next().unwrap_or(fmt);
                let mut history = Vec::with_capacity(segment.len() + 1);
                history.push(b'\\');
                history.extend_from_slice(segment);
                self.text.append_history_record(&history);
                host.text_control(
                    &ControlCode {
                        kind,
                        payload: segment.get(1..).unwrap_or(&[]).to_vec(),
                    },
                );
                if matches!(
                    kind, ControlCodeKind::PagePause | ControlCodeKind::PageClear |
                    ControlCodeKind::Wait
                ) {
                    self.text.render_state = 0;
                }
                let bridge_hash = match kind {
                    ControlCodeKind::Newline | ControlCodeKind::NewlineRelative => {
                        Some(0x04AE_36BD)
                    }
                    ControlCodeKind::PagePause => Some(0x20CE_505D),
                    ControlCodeKind::PageClear => Some(0x7EE6_7053),
                    ControlCodeKind::Wait => Some(0x93B3_8A0B),
                    _ => None,
                };
                if let Some(name_hash) = bridge_hash {
                    let scheduled = self.invoke_host_func(name_hash, &[], false)?;
                    crate::text_trace!(
                        "[VM] SYS_0E_BRIDGE code={} hash=0x{name_hash:08X} scheduled={scheduled}",
                        char::from(code)
                    );
                }
                if kind == ControlCodeKind::Wait {
                    self.text.reset_for_wait();
                    if let Some(frame) = self
                        .frames
                        .iter_mut()
                        .rev()
                        .find(|frame| frame.text_line_pending)
                    {
                        frame.text_line_pending = false;
                    }
                } else if matches!(
                    kind, ControlCodeKind::Newline | ControlCodeKind::NewlineRelative
                ) {
                    if kind == ControlCodeKind::Newline {
                        self.text.accumulator.clear();
                    }
                    self.text.page_capture_enabled = false;
                }
            }
        }
        Ok(())
    }
    fn capture_text_accumulator(&mut self, bytes: &[u8]) {
        use crate::text::{tokenize, ControlCodeKind, TextToken};
        if !self.text.history_capture_enabled() {
            return;
        }
        for token in tokenize(bytes) {
            match token {
                TextToken::Text(bytes) => {
                    self.text.accumulator.extend_from_slice(&bytes);
                    if self.text.page_capture_enabled {
                        self.text.page_accumulator.extend_from_slice(&bytes);
                    }
                }
                TextToken::Control(control) => {
                    match control.kind {
                        ControlCodeKind::Wait => self.text.reset_for_wait(),
                        ControlCodeKind::Newline => {
                            self.text.accumulator.clear();
                            self.text.page_capture_enabled = false;
                        }
                        ControlCodeKind::NewlineRelative => {
                            self.text.page_capture_enabled = false;
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    pub fn frame_depth(&self) -> usize {
        self.frames.len()
    }
    pub fn current_ip(&self) -> Option<usize> {
        self.frames.last().map(|f| f.cursor.ip)
    }
    pub fn current_code(&self) -> Option<&[u8]> {
        self.frames.last().map(|f| f.cursor.code)
    }
    pub fn current_script_idx(&self) -> Option<usize> {
        self.frames.last().map(|f| f.script_idx)
    }
    pub fn current_label(&self) -> Option<&str> {
        self.frames.last().map(|f| f.label.as_str())
    }
    pub fn frame_probe_summary(&self) -> String {
        fn frame_summary(frames: &[CallFrame]) -> String {
            let Some(frame) = frames.last() else {
                return "-".to_owned();
            };
            format!(
                "s{}:0x{:X}:w{:?}", frame.script_idx, frame.cursor.ip, frame
                .wait_deadline
            )
        }
        let mut contexts = Vec::new();
        for idx in (0..self.suspended_ctxs.len()).rev() {
            if idx == self.current_ctx {
                contexts
                    .push(
                        format!(
                            "{idx}*=0x{:08X}:{:?}:{:?}:{}", self.active_function_id, self
                            .active_state, self.active_phase, frame_summary(& self
                            .frames)
                        ),
                    );
            } else if let Some(ctx) = self.suspended_ctxs[idx].as_ref() {
                contexts
                    .push(
                        format!(
                            "{idx}=0x{:08X}:{:?}:{:?}:{}", ctx.function_id, ctx.state,
                            ctx.phase, frame_summary(& ctx.frames)
                        ),
                    );
            }
        }
        format!(
            "current_slot={} no_present={} contexts=[{}]", self.current_ctx, self
            .scheduler_no_present, contexts.join(",")
        )
    }
    pub(crate) fn stack_scope_abs(&self, idx: usize) -> usize {
        let base = match self.frames.last() {
            Some(f) => f.stack_base as isize,
            None => 0,
        };
        let signed = (idx as u16) as i16 as isize;
        let raw = base.wrapping_add(signed);
        if raw < 0 { 0 } else { raw as usize }
    }
    pub(crate) fn request_scene_transition(&mut self) {
        self.scene_transition_requested = true;
    }
    fn apply_scene_transition(&mut self) {
        if self.active_phase == CtxPhase::Initial {
            self.active_phase = CtxPhase::TransitionRequested;
        }
        for ctx in self.suspended_ctxs.iter_mut().flatten() {
            if ctx.phase == CtxPhase::Initial {
                ctx.phase = CtxPhase::TransitionRequested;
            }
        }
        let mut freed_ids = Vec::new();
        for slot in &mut self.suspended_ctxs {
            let Some(mut ctx) = slot.take() else {
                continue;
            };
            if !process_saved_context_phase(&self.scripts, &mut ctx) {
                freed_ids.push(ctx.function_id);
            } else {
                *slot = Some(ctx);
            }
        }
        self.process_active_context_phase();
        if self.active_state == CtxState::Inactive {
            freed_ids.push(self.active_function_id);
        }
        if !freed_ids.is_empty() {
            self.host_funcs
                .retain(|_, regs| {
                    regs.retain(|reg| reg.pinned || !freed_ids.contains(&reg.ctx));
                    !regs.is_empty()
                });
        }
        self.scene_transition_requested = false;
    }
    pub(crate) fn process_active_context_phase(&mut self) {
        match self.active_phase {
            CtxPhase::TransitionRequested => {
                if redirect_transition_frames(&mut self.frames) {
                    self.active_phase = CtxPhase::Cleanup;
                    self.active_state = CtxState::Running;
                } else if let Some(target) = self.active_finalizer_target.take() {
                    install_context_finalizer(
                        &self.scripts,
                        &mut self.frames,
                        &mut self.stack,
                        target,
                    );
                    self.active_phase = CtxPhase::Finalizer;
                    self.active_state = CtxState::Running;
                } else {
                    self.release_active_context();
                }
            }
            CtxPhase::CleanupReturned => {
                if let Some(target) = self.active_finalizer_target.take() {
                    install_context_finalizer(
                        &self.scripts,
                        &mut self.frames,
                        &mut self.stack,
                        target,
                    );
                    self.active_phase = CtxPhase::Finalizer;
                    self.active_state = CtxState::Running;
                } else {
                    self.release_active_context();
                }
            }
            CtxPhase::Dead => self.release_active_context(),
            CtxPhase::Initial | CtxPhase::Cleanup | CtxPhase::Finalizer => {}
        }
    }
    fn release_active_context(&mut self) {
        let function_id = self.active_function_id;
        self.frames.clear();
        self.stack = Stack::new();
        self.stack.push(Value::int(0));
        self.threads.clear();
        self.thread_keys.clear();
        self.active_finalizer_target = None;
        self.active_phase = CtxPhase::Dead;
        self.active_state = CtxState::Inactive;
        self.host_funcs
            .retain(|_, regs| {
                regs.retain(|reg| reg.pinned || reg.ctx != function_id);
                !regs.is_empty()
            });
    }
    pub(crate) fn set_context_finalizer(&mut self, target: Option<(usize, usize)>) {
        self.active_finalizer_target = target;
    }
    pub(crate) fn propagate_parent_transitions(&mut self) {
        let mut initial_context_ids = std::collections::HashSet::with_capacity(
            self.suspended_ctxs.len() + 1,
        );
        if self.active_state != CtxState::Inactive
            && self.active_phase == CtxPhase::Initial
        {
            initial_context_ids.insert(self.active_function_id);
        }
        initial_context_ids
            .extend(
                self
                    .suspended_ctxs
                    .iter()
                    .flatten()
                    .filter_map(|ctx| {
                        (ctx.state != CtxState::Inactive
                            && ctx.phase == CtxPhase::Initial)
                            .then_some(ctx.function_id)
                    }),
            );
        let mut changed = false;
        if self.active_phase == CtxPhase::Initial {
            if let Some(parent) = self.active_parent_function_id {
                if !initial_context_ids.contains(&parent) {
                    self.active_parent_function_id = None;
                    self.active_phase = CtxPhase::TransitionRequested;
                    changed = true;
                }
            }
        }
        for ctx in self.suspended_ctxs.iter_mut().flatten() {
            if ctx.phase == CtxPhase::Initial
                && ctx
                    .parent_function_id
                    .is_some_and(|parent| !initial_context_ids.contains(&parent))
            {
                ctx.parent_function_id = None;
                ctx.phase = CtxPhase::TransitionRequested;
                changed = true;
            }
        }
        if !changed {
            return;
        }
        let mut freed_ids = Vec::new();
        for slot in &mut self.suspended_ctxs {
            let needs_phase_work = slot
                .as_ref()
                .is_some_and(|ctx| {
                    matches!(
                        ctx.phase, CtxPhase::TransitionRequested |
                        CtxPhase::CleanupReturned | CtxPhase::Dead
                    )
                });
            if !needs_phase_work {
                continue;
            }
            let Some(mut ctx) = slot.take() else {
                continue;
            };
            if !process_saved_context_phase(&self.scripts, &mut ctx) {
                freed_ids.push(ctx.function_id);
            } else {
                *slot = Some(ctx);
            }
        }
        self.process_active_context_phase();
        if self.active_state == CtxState::Inactive {
            freed_ids.push(self.active_function_id);
        }
        if !freed_ids.is_empty() {
            self.host_funcs
                .retain(|_, regs| {
                    regs.retain(|reg| reg.pinned || !freed_ids.contains(&reg.ctx));
                    !regs.is_empty()
                });
        }
    }
    pub(crate) fn process_pending_context_phases(&mut self) {
        let mut freed_ids = Vec::new();
        for slot in &mut self.suspended_ctxs {
            let needs_phase_work = slot
                .as_ref()
                .is_some_and(|ctx| {
                    matches!(
                        ctx.phase, CtxPhase::TransitionRequested |
                        CtxPhase::CleanupReturned | CtxPhase::Dead
                    )
                });
            if !needs_phase_work {
                continue;
            }
            let Some(mut ctx) = slot.take() else {
                continue;
            };
            if !process_saved_context_phase(&self.scripts, &mut ctx) {
                freed_ids.push(ctx.function_id);
            } else {
                *slot = Some(ctx);
            }
        }
        self.process_active_context_phase();
        if self.active_state == CtxState::Inactive {
            freed_ids.push(self.active_function_id);
        }
        if !freed_ids.is_empty() {
            self.host_funcs
                .retain(|_, regs| {
                    regs.retain(|reg| reg.pinned || !freed_ids.contains(&reg.ctx));
                    !regs.is_empty()
                });
        }
    }
    pub(crate) fn request_context_termination(&mut self, function_id: u32) {
        if self.active_function_id == function_id
            && self.active_state != CtxState::Inactive
        {
            self.active_phase = if self.active_phase == CtxPhase::Initial {
                CtxPhase::TransitionRequested
            } else {
                CtxPhase::Dead
            };
            return;
        }
        for ctx in self.suspended_ctxs.iter_mut().flatten() {
            if ctx.function_id == function_id && ctx.state != CtxState::Inactive
                && ctx.phase == CtxPhase::Initial
            {
                ctx.phase = CtxPhase::TransitionRequested;
                return;
            }
        }
    }
    pub(crate) fn reset_active_context_to_entry(
        &mut self,
        script_idx: usize,
        entry_offset: usize,
        forwarded_args: Vec<Value>,
        label: impl Into<String>,
    ) -> Result<(), VmError> {
        if entry_offset > self.scripts[script_idx].code.len() {
            return Err(
                VmError::Other(
                    format!(
                        "reset target offset 0x{entry_offset:X} beyond code len 0x{:X}",
                        self.scripts[script_idx].code.len()
                    ),
                ),
            );
        }
        let function_id = self.active_function_id;
        self.host_funcs
            .retain(|_, regs| {
                regs.retain(|reg| reg.pinned || reg.ctx != function_id);
                !regs.is_empty()
            });
        self.frames.clear();
        self.stack = Stack::new();
        self.stack.push(Value::int(0));
        self.threads.clear();
        self.thread_keys.clear();
        self.active_state = CtxState::Running;
        self.active_phase = CtxPhase::Initial;
        self.active_finalizer_target = None;
        let param_count = forwarded_args.len() as i16;
        for arg in forwarded_args {
            self.stack.push(arg);
        }
        self.stack.push(Value::int(param_count as i32));
        let entry_sp = self.stack.len();
        let code_ptr: *const [u8] = self.scripts[script_idx].code.as_ref();
        let code: &'static [u8] = unsafe { &*code_ptr };
        self.frames
            .push(CallFrame {
                cursor: Cursor::new(code, entry_offset),
                script_idx,
                entry_sp,
                stack_base: entry_sp,
                cleanup_mode: 0,
                script_marker: 0,
                param_count,
                label: label.into(),
                jump_target_a: None,
                jump_target_b: None,
                jump_target_c: None,
                frame_advance_target: None,
                transition_target: None,
                force_return: false,
                wait_deadline: WaitDeadline::None,
                wait_present_epoch: None,
                text_line_pending: false,
            });
        Ok(())
    }
    pub fn next_wake_delay_ms(&self, now_ms: i32) -> Option<u64> {
        fn top_deadline(frames: &[CallFrame]) -> Option<i32> {
            match frames.last()?.wait_deadline {
                WaitDeadline::At(deadline) => Some(deadline),
                WaitDeadline::None | WaitDeadline::Never => None,
            }
        }
        let active_runnable = !self.frames.is_empty()
            && matches!(self.active_state, CtxState::Idle | CtxState::Running);
        let suspended_runnable = self
            .suspended_ctxs
            .iter()
            .flatten()
            .any(|ctx| {
                !ctx.frames.is_empty()
                    && matches!(ctx.state, CtxState::Idle | CtxState::Running)
            });
        if active_runnable || suspended_runnable {
            return Some(0);
        }
        let active = (self.active_state == CtxState::Waiting)
            .then(|| top_deadline(&self.frames))
            .flatten();
        self.suspended_ctxs
            .iter()
            .flatten()
            .filter(|ctx| ctx.state == CtxState::Waiting)
            .filter_map(|ctx| top_deadline(&ctx.frames))
            .chain(active)
            .map(|deadline| {
                if now_ms < deadline
                    && now_ms.wrapping_sub(deadline).wrapping_abs() <= 0x0293_2E00
                {
                    deadline.wrapping_sub(now_ms) as u32 as u64
                } else {
                    0
                }
            })
            .min()
    }
    pub(crate) fn create_ctx(&mut self, function_id: u32) -> usize {
        self.create_ctx_with_parent(function_id, None)
    }
    fn create_ctx_with_parent(
        &mut self,
        function_id: u32,
        parent_function_id: Option<u32>,
    ) -> usize {
        let idx = self.suspended_ctxs.len();
        self.suspended_ctxs
            .push(Some(ExecContext::new(function_id, parent_function_id)));
        idx
    }
    pub(crate) fn create_ctx_auto(
        &mut self,
        parent_function_id: Option<u32>,
    ) -> (u32, usize) {
        let next_id = self.next_function_id;
        self.next_function_id += 1;
        let idx = self.create_ctx_with_parent(next_id, parent_function_id);
        (next_id, idx)
    }
    pub(crate) fn find_ctx_by_function_id(&self, function_id: u32) -> Option<usize> {
        if function_id == self.active_function_id
            && self.active_state != CtxState::Inactive
        {
            return Some(self.current_ctx);
        }
        for (idx, ctx_opt) in self.suspended_ctxs.iter().enumerate() {
            if let Some(ctx) = ctx_opt {
                if ctx.function_id == function_id && ctx.state != CtxState::Inactive {
                    return Some(idx);
                }
            }
        }
        None
    }
    pub fn current_function_id(&self) -> u32 {
        self.active_function_id
    }
    pub(crate) fn switch_ctx(&mut self, idx: usize) {
        if idx == self.current_ctx || idx >= self.suspended_ctxs.len() {
            return;
        }
        let Some(next) = self.suspended_ctxs[idx].take() else {
            return;
        };
        let old_frames = std::mem::take(&mut self.frames);
        let old_stack = std::mem::take(&mut self.stack);
        let old_threads = std::mem::take(&mut self.threads);
        let old_thread_keys = std::mem::take(&mut self.thread_keys);
        if self.active_state == CtxState::Inactive {
            self.suspended_ctxs[self.current_ctx] = None;
        } else {
            self.suspended_ctxs[self.current_ctx] = Some(ExecContext {
                function_id: self.active_function_id,
                parent_function_id: self.active_parent_function_id,
                frames: old_frames,
                stack: old_stack,
                threads: old_threads,
                thread_keys: old_thread_keys,
                state: self.active_state,
                phase: self.active_phase,
                finalizer_target: self.active_finalizer_target,
            });
        }
        self.active_function_id = next.function_id;
        self.active_parent_function_id = next.parent_function_id;
        self.active_state = next.state;
        self.active_phase = next.phase;
        self.active_finalizer_target = next.finalizer_target;
        self.frames = next.frames;
        self.stack = next.stack;
        self.threads = next.threads;
        self.thread_keys = next.thread_keys;
        self.current_ctx = idx;
    }
    pub(crate) fn is_root_ctx(&self) -> bool {
        self.current_ctx == 0
    }
    pub fn skip_current_instruction(&mut self) -> Option<u16> {
        let frame = self.frames.last_mut()?;
        let code = frame.cursor.code;
        let ip = frame.cursor.ip;
        if ip + 2 > code.len() {
            return None;
        }
        let op = u16::from_le_bytes([code[ip], code[ip + 1]]);
        let size = crate::opcode::instruction_size(code, ip).unwrap_or(2);
        frame.cursor.ip = (frame.cursor.ip + size).min(code.len());
        Some(op)
    }
}
impl Default for Vm {
    fn default() -> Self {
        Self::new()
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepResult {
    Continue,
    Returned,
    Waiting,
    YieldNoPresent,
    Exited,
}

