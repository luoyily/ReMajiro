use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use encoding_rs::SHIFT_JIS;
use formats::save::{
    ArrayPayload, ComplexPayload, MssFile, SavFile, SavHeader, ScriptStateEntry, SerValue,
};
use crate::cursor::Cursor;
use crate::exec::{CallFrame, CtxPhase, CtxState, ScriptEntry, Vm, VmError, WaitDeadline};
use crate::host::Host;
use crate::value::{
    Scope, Value, ValueData, TAG_FIXED_INT_ARRAY, TAG_FLOAT, TAG_INT, TAG_INT_ARRAY,
    TAG_STRING, TAG_STRING_ARRAY,
};
const INNER_INSTRUCTION_SIZE: usize = 8;
type RootSaveContext<'a> = (
    &'a [CallFrame],
    &'a [Value],
    &'a [Value],
    &'a HashMap<u32, usize>,
);
struct SnapshotBridge {
    objects: HashMap<usize, SerValue>,
}
impl SnapshotBridge {
    fn new() -> Self {
        Self { objects: HashMap::new() }
    }
    fn pool(&mut self, values: &[Value]) -> Vec<SerValue> {
        values.iter().map(|value| self.value(value)).collect()
    }
    fn value(&mut self, value: &Value) -> SerValue {
        match value.type_tag {
            TAG_INT => SerValue::Int(value.bits as i32),
            TAG_FLOAT => SerValue::Float(f32::from_bits(value.bits)),
            TAG_STRING | TAG_INT_ARRAY | TAG_FIXED_INT_ARRAY | TAG_STRING_ARRAY => {
                let Some(data) = value.data.as_ref() else {
                    return match value.type_tag {
                        TAG_STRING_ARRAY => {
                            SerValue::Complex(
                                Arc::new(ComplexPayload {
                                    dims: [1, 1, 1],
                                    entries: vec![None],
                                }),
                            )
                        }
                        TAG_INT_ARRAY | TAG_FIXED_INT_ARRAY => {
                            SerValue::Array(
                                Arc::new(ArrayPayload {
                                    dims: [1, 1, 1],
                                    cells: vec![0],
                                }),
                            )
                        }
                        _ => SerValue::String(Arc::from([])),
                    };
                };
                let key = Rc::as_ptr(data) as *const () as usize;
                if let Some(cached) = self.objects.get(&key) {
                    return cached.clone();
                }
                let serialized = match data.as_ref() {
                    ValueData::Str(bytes) => {
                        SerValue::String(Arc::from(bytes.as_slice()))
                    }
                    ValueData::IntArray { dims, cells, .. } => {
                        SerValue::Array(
                            Arc::new(ArrayPayload {
                                dims: normalized_dims(*dims),
                                cells: cells
                                    .iter()
                                    .map(|cell| cell.as_int().unwrap_or(cell.bits as i32))
                                    .collect(),
                            }),
                        )
                    }
                    ValueData::StringArray { dims, cells } => {
                        SerValue::Complex(
                            Arc::new(ComplexPayload {
                                dims: normalized_dims(*dims),
                                entries: cells
                                    .iter()
                                    .map(|cell| cell.as_str_bytes().map(Arc::<[u8]>::from))
                                    .collect(),
                            }),
                        )
                    }
                };
                self.objects.insert(key, serialized.clone());
                serialized
            }
            _ => SerValue::Int(value.bits as i32),
        }
    }
}
struct RestoreBridge {
    objects: HashMap<(u8, usize), Rc<ValueData>>,
}
impl RestoreBridge {
    fn new() -> Self {
        Self { objects: HashMap::new() }
    }
    fn pool(&mut self, values: &[SerValue], scope: Scope) -> Vec<Value> {
        values.iter().map(|value| self.value(value, scope)).collect()
    }
    fn value(&mut self, value: &SerValue, scope: Scope) -> Value {
        match value {
            SerValue::Int(value) => scoped(Value::int(*value), scope),
            SerValue::Float(value) => scoped(Value::float(*value), scope),
            SerValue::String(bytes) => {
                let key = (TAG_STRING as u8, arc_slice_key(bytes));
                let data = self
                    .objects
                    .entry(key)
                    .or_insert_with(|| Rc::new(ValueData::Str(bytes.as_ref().to_vec())))
                    .clone();
                Value {
                    scope,
                    type_tag: TAG_STRING,
                    bits: 0,
                    data: Some(data),
                }
            }
            SerValue::Array(array) => {
                let key = (TAG_INT_ARRAY as u8, Arc::as_ptr(array) as usize);
                let data = self
                    .objects
                    .entry(key)
                    .or_insert_with(|| {
                        Rc::new(ValueData::IntArray {
                            ndim: dimension_count(array.dims),
                            dims: array.dims,
                            cells: array.cells.iter().copied().map(Value::int).collect(),
                        })
                    })
                    .clone();
                Value {
                    scope,
                    type_tag: TAG_INT_ARRAY,
                    bits: 0,
                    data: Some(data),
                }
            }
            SerValue::Complex(complex) => {
                let key = (TAG_STRING_ARRAY as u8, Arc::as_ptr(complex) as usize);
                let cells = complex
                    .entries
                    .iter()
                    .map(|entry| match entry {
                        Some(bytes) => {
                            self.value(&SerValue::String(bytes.clone()), Scope::Stack)
                        }
                        None => Value::string(vec![0]),
                    })
                    .collect();
                let data = self
                    .objects
                    .entry(key)
                    .or_insert_with(|| {
                        Rc::new(ValueData::StringArray {
                            dims: complex.dims,
                            cells,
                        })
                    })
                    .clone();
                Value {
                    scope,
                    type_tag: TAG_STRING_ARRAY,
                    bits: 0,
                    data: Some(data),
                }
            }
        }
    }
}
impl Vm {
    pub(crate) fn capture_save_snapshot(&mut self) {
        let Some((frames, stack_values, thread_values, thread_keys)) = self
            .root_save_context() else {
            eprintln!("[SAVE] snapshot skipped: root context is unavailable");
            return;
        };
        if frames.is_empty() {
            eprintln!("[SAVE] snapshot skipped: root context has no call frames");
            return;
        }
        let mut bridge = SnapshotBridge::new();
        let globals = bridge.pool(&self.locals);
        let thread_globals = bridge.pool(thread_values);
        let locals = bridge.pool(stack_values);
        let global_slot_heads = slot_heads(&self.local_keys, self.locals.len());
        let thread_slot_heads = slot_heads(thread_keys, thread_values.len());
        let local_slot_heads = vec![0; stack_values.len()];
        let mut script_state: Vec<ScriptStateEntry> = frames
            .iter()
            .enumerate()
            .map(|(index, frame)| {
                self.snapshot_frame(
                    frame,
                    frames.get(index + 1),
                    index + 1 == frames.len(),
                    stack_values.len(),
                )
            })
            .collect();
        if let Some(top) = script_state.last_mut() {
            top.ip_offset = top.ip_offset.saturating_sub(INNER_INSTRUCTION_SIZE as u32);
        }
        for (index, entry) in script_state.iter().enumerate() {
            eprintln!(
                "[SAVE] snapshot frame[{}] {} ip=0x{:X}", index, saved_script_name(&
                entry.name), entry.ip_offset
            );
        }
        let header = SavHeader {
            description: self.save_description,
            text_first_render: u32::from(self.text.first_render),
            text_state_blob: self.text.history_buf.clone(),
            ..SavHeader::default()
        };
        self.pending_save_snapshot = Some(SavFile {
            header,
            thumbnail: None,
            script_state,
            global_slot_heads,
            thread_slot_heads,
            local_slot_heads,
            globals,
            thread_globals,
            locals,
        });
    }
    pub(crate) fn set_save_description(&mut self, description: &[u8]) {
        self.save_description.fill(0);
        let description = description
            .split(|byte| *byte == 0)
            .next()
            .unwrap_or(description);
        let len = description.len().min(self.save_description.len() - 1);
        self.save_description[..len].copy_from_slice(&description[..len]);
    }
    pub(crate) fn snapshot_system_file(&self) -> MssFile {
        let mut keys = vec![0; self.globals.len()];
        for (&key, &slot) in &self.global_keys {
            if let Some(destination) = keys.get_mut(slot) {
                *destination = key;
            }
        }
        let mut bridge = SnapshotBridge::new();
        MssFile {
            keys,
            values: bridge.pool(&self.globals),
            header_tail: Vec::new(),
        }
    }
    pub fn persist_system_state<H: Host>(&self, host: &mut H) -> bool {
        let system = self.snapshot_system_file();
        let readmarks = self.snapshot_readmarks();
        host.save_write_system(&system) && host.save_write_readmarks(&readmarks)
    }
    pub(crate) fn queue_save_restore(&mut self, save: SavFile) {
        self.pending_load = Some(save);
    }
    pub(crate) fn free_save_snapshot(&mut self) {
        self.pending_save_snapshot = None;
    }
    pub fn apply_pending_save_restore<H: Host>(
        &mut self,
        host: &mut H,
    ) -> Result<bool, VmError> {
        let Some(save) = self.pending_load.take() else {
            return Ok(false);
        };
        let hot_reset_text_state = save.header.text_first_render as i32;
        let saved_audio_state = save.header.audio_blob.clone();
        host.save_prepare_restore();
        self.native_mode_bits = 0;
        self.native_mode_special = false;
        self.native_mode_counter = 0;
        self.native_skip_mode = false;
        self.input_suppressed = false;
        host.set_native_mode_flags(0, false);
        let retired_callbacks = self.cleanup_unpinned_host_funcs();
        if retired_callbacks != 0 {
            eprintln!(
                "[SAVE] restore boundary retired {} transient host callbacks",
                retired_callbacks
            );
        }
        if let Some(system) = host.save_read_system() {
            self.restore_system_snapshot(&system);
        }
        self.restore_save(save, host)?;
        host.save_restore_audio_state(&saved_audio_state);
        host.save_set_dialog_available(true);
        host.save_mark_ready();
        let args = [
            Value::int(hot_reset_text_state),
            Value::string(Vec::new()),
            Value::int(1),
        ];
        let callbacks = self.invoke_host_func(0x3857_9896, &args, false)?;
        eprintln!("[SAVE] HOT_RESET loaded=1 scheduled {} callbacks", callbacks);
        Ok(true)
    }
    fn restore_system_snapshot(&mut self, system: &MssFile) {
        let mut bridge = RestoreBridge::new();
        let values = bridge.pool(&system.values, Scope::Global);
        self.restore_system_globals(system.keys.iter().copied().zip(values));
    }
    fn restore_save<H: Host>(
        &mut self,
        save: SavFile,
        host: &mut H,
    ) -> Result<(), VmError> {
        if save.script_state.is_empty() {
            return Err(
                VmError::Other(
                    "save restore rejected: no script-state entries".to_string(),
                ),
            );
        }
        for (index, entry) in save.script_state.iter().enumerate() {
            eprintln!(
                "[SAVE] restore frame[{}] {} ip=0x{:X}", index, saved_script_name(& entry
                .name), entry.ip_offset
            );
        }
        let restored_local_keys = restored_slot_keys(
            &save.global_slot_heads,
            save.globals.len(),
        );
        let restored_thread_keys = restored_slot_keys(
            &save.thread_slot_heads,
            save.thread_globals.len(),
        );
        let mut bridge = RestoreBridge::new();
        let restored_locals = bridge.pool(&save.globals, Scope::Local);
        let restored_threads = bridge.pool(&save.thread_globals, Scope::Thread);
        let restored_stack = bridge.pool(&save.locals, Scope::Stack);
        let mut frames: Vec<CallFrame> = Vec::with_capacity(save.script_state.len());
        for (index, entry) in save.script_state.iter().enumerate() {
            let script_idx = self.resolve_saved_script(&entry.name, host)?;
            let code_ptr: *const [u8] = self.scripts[script_idx].code.as_ref();
            let code: &'static [u8] = unsafe { &*code_ptr };
            let ip = entry.ip_offset as usize;
            if ip > code.len() {
                return Err(
                    VmError::Other(
                        format!(
                            "save restore rejected: frame {} IP 0x{:X} exceeds script length 0x{:X}",
                            index, ip, code.len()
                        ),
                    ),
                );
            }
            let target = |offset: u32| -> Result<Option<usize>, VmError> {
                if offset == 0 {
                    return Ok(None);
                }
                let offset = offset as usize;
                if offset > code.len() {
                    return Err(
                        VmError::Other(
                            format!(
                                "save restore rejected: frame {} target 0x{:X} exceeds script length 0x{:X}",
                                index, offset, code.len()
                            ),
                        ),
                    );
                }
                Ok(Some(offset))
            };
            let stack_base = entry.refs[0] as usize;
            let (entry_sp, cleanup_mode, param_count) = if index == 0 {
                (stack_base, 0, 0)
            } else {
                let parent = &save.script_state[index - 1];
                let parent_code = frames
                    .last()
                    .expect("non-root save frame has a restored parent")
                    .cursor
                    .code;
                let (entry_sp, param_count) = restored_call_boundary(
                    index,
                    entry,
                    parent,
                    parent_code,
                    &restored_stack,
                )?;
                (entry_sp, parent.refs[2], param_count)
            };
            frames
                .push(CallFrame {
                    cursor: Cursor::new(code, ip),
                    script_idx,
                    entry_sp,
                    stack_base,
                    cleanup_mode,
                    script_marker: entry.frame_count,
                    param_count,
                    label: format!(
                        "save-restore:{}@0x{ip:06X}", saved_script_name(& entry.name)
                    ),
                    jump_target_a: target(entry.extra_offsets[0])?,
                    jump_target_b: target(entry.extra_offsets[1])?,
                    jump_target_c: target(entry.extra_offsets[2])?,
                    frame_advance_target: target(entry.extra_offsets[3])?,
                    transition_target: target(entry.extra_offsets[4])?,
                    force_return: false,
                    wait_deadline: WaitDeadline::None,
                    wait_present_epoch: None,
                    text_line_pending: false,
                });
        }
        let saved_top = save
            .script_state
            .last()
            .map(|entry| entry.refs[1] as usize)
            .unwrap_or(0);
        if saved_top != restored_stack.len() {
            return Err(
                VmError::Other(
                    format!(
                        "save restore rejected: top-frame SP {} does not match Stack pool length {}",
                        saved_top, restored_stack.len()
                    ),
                ),
            );
        }
        if frames.iter().any(|frame| frame.stack_base > restored_stack.len()) {
            return Err(
                VmError::Other(
                    "save restore rejected: frame Stack base exceeds Stack pool"
                        .to_string(),
                ),
            );
        }
        self.frames = frames;
        self.stack.replace(restored_stack);
        self.locals = restored_locals;
        self.local_keys = restored_local_keys;
        self.threads = restored_threads;
        self.thread_keys = restored_thread_keys;
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
        self.pending_hotspot_callback = None;
        self.pending_inner_host_bridges.clear();
        self.save_description = save.header.description;
        self.text.first_render = save.header.text_first_render != 0;
        self.text.history_buf = save.header.text_state_blob;
        self.text.history_buf.resize(40_000, 0);
        self.text.history_buf.truncate(40_000);
        self.text.clear_history();
        self.text.page_accumulator.clear();
        self.text.page_capture_enabled = false;
        self.text.display_state = 0;
        self.text.skip_depth = 0;
        self.text.pending_render_line.clear();
        self.text.last_history_render = None;
        self.pending_save_snapshot = None;
        eprintln!(
            "[SAVE] restored {} frames, {} local vars, {} thread vars, {} stack values",
            self.frames.len(), self.locals.len(), self.threads.len(), self.stack.len()
        );
        Ok(())
    }
    fn root_save_context(&self) -> Option<RootSaveContext<'_>> {
        if self.active_function_id == 0 {
            return Some((
                &self.frames,
                self.stack.as_slice(),
                &self.threads,
                &self.thread_keys,
            ));
        }
        self.suspended_ctxs
            .iter()
            .flatten()
            .find_map(|context| {
                (context.function_id == 0)
                    .then_some((
                        context.frames.as_slice(),
                        context.stack.as_slice(),
                        context.threads.as_slice(),
                        &context.thread_keys,
                    ))
            })
    }
    fn snapshot_frame(
        &self,
        frame: &CallFrame,
        next_frame: Option<&CallFrame>,
        is_top: bool,
        stack_len: usize,
    ) -> ScriptStateEntry {
        let mut name = [0; 128];
        let script_name = self.scripts[frame.script_idx].name.as_deref().unwrap_or(b"");
        let disk_name = script_name_for_save(script_name);
        let len = disk_name.len().min(name.len() - 1);
        name[..len].copy_from_slice(&disk_name[..len]);
        ScriptStateEntry {
            name,
            ip_offset: frame.cursor.ip as u32,
            refs: [
                frame.stack_base as u32,
                if is_top {
                    stack_len as u32
                } else if let Some(next) = next_frame {
                    next.entry_sp.saturating_sub(1 + next.param_count.max(0) as usize)
                        as u32
                } else {
                    frame.entry_sp as u32
                },
                next_frame.map(|next| next.cleanup_mode).unwrap_or(0),
            ],
            frame_count: frame.script_marker,
            extra_offsets: [
                frame.jump_target_a.unwrap_or(0) as u32,
                frame.jump_target_b.unwrap_or(0) as u32,
                frame.jump_target_c.unwrap_or(0) as u32,
                frame.frame_advance_target.unwrap_or(0) as u32,
                frame.transition_target.unwrap_or(0) as u32,
            ],
        }
    }
    fn resolve_saved_script<H: Host>(
        &mut self,
        raw_name: &[u8; 128],
        host: &mut H,
    ) -> Result<usize, VmError> {
        let requested = normalized_saved_script_name(raw_name);
        if let Some(index) = self
            .scripts
            .iter()
            .position(|script| {
                script
                    .name
                    .as_deref()
                    .is_some_and(|loaded_name| {
                        normalized_runtime_script_name(loaded_name) == requested
                    })
            })
        {
            return Ok(index);
        }
        let loaded = host
            .load_script(&requested)
            .ok_or_else(|| {
                VmError::Other(
                    format!(
                        "save restore could not load script {:?}",
                        saved_script_name(raw_name)
                    ),
                )
            })?;
        let entries = loaded
            .entries
            .into_iter()
            .map(|(name_hash, offset)| ScriptEntry { name_hash, offset })
            .collect();
        Ok(
            self
                .load_script_with_entries_named_readmarks_crc(
                    loaded.code,
                    entries,
                    Some(loaded.name.into_bytes()),
                    loaded.line_count,
                    loaded.code_crc32,
                ),
        )
    }
}
fn scoped(mut value: Value, scope: Scope) -> Value {
    value.scope = scope;
    value
}
fn slot_heads(keys: &HashMap<u32, usize>, len: usize) -> Vec<u32> {
    let mut heads = vec![0; len];
    for (&key, &slot) in keys {
        if let Some(head) = heads.get_mut(slot) {
            *head = key;
        }
    }
    heads
}
fn restored_slot_keys(heads: &[u32], value_count: usize) -> HashMap<u32, usize> {
    heads
        .iter()
        .copied()
        .take(value_count)
        .enumerate()
        .filter_map(|(slot, key)| (key != 0).then_some((key, slot)))
        .collect()
}
fn normalized_dims(mut dims: [u32; 3]) -> [u32; 3] {
    for dim in &mut dims {
        *dim = (*dim).max(1);
    }
    dims
}
fn dimension_count(dims: [u32; 3]) -> u8 {
    if dims[2] > 1 { 3 } else if dims[1] > 1 { 2 } else { 1 }
}
fn arc_slice_key(value: &Arc<[u8]>) -> usize {
    Arc::as_ptr(value) as *const u8 as usize
}
fn bare_script_name(bytes: &[u8]) -> &[u8] {
    let bytes = bytes.split(|byte| *byte == 0).next().unwrap_or(bytes);
    let bytes = bytes
        .rsplit(|byte| matches!(* byte, b'/' | b'\\'))
        .next()
        .unwrap_or(bytes);
    bytes
        .get(bytes.len().saturating_sub(4)..)
        .filter(|suffix| suffix.eq_ignore_ascii_case(b".mjo"))
        .map_or(bytes, |_| &bytes[..bytes.len() - 4])
}
fn normalized_saved_script_name(bytes: &[u8]) -> String {
    SHIFT_JIS.decode(bare_script_name(bytes)).0.to_lowercase()
}
fn normalized_runtime_script_name(bytes: &[u8]) -> String {
    let bytes = bare_script_name(bytes);
    match std::str::from_utf8(bytes) {
        Ok(name) => name.to_lowercase(),
        Err(_) => SHIFT_JIS.decode(bytes).0.to_lowercase(),
    }
}
fn script_name_for_save(bytes: &[u8]) -> Vec<u8> {
    let mut encoded = match std::str::from_utf8(bytes) {
        Ok(name) => SHIFT_JIS.encode(name).0.into_owned(),
        Err(_) => bytes.to_vec(),
    };
    let has_extension = encoded
        .get(encoded.len().saturating_sub(4)..)
        .is_some_and(|suffix| suffix.eq_ignore_ascii_case(b".mjo"));
    if !has_extension {
        encoded.extend_from_slice(b".MJO");
    }
    encoded
}
fn saved_script_name(bytes: &[u8]) -> String {
    let bytes = bytes.split(|byte| *byte == 0).next().unwrap_or(bytes);
    SHIFT_JIS.decode(bytes).0.into_owned()
}
fn restored_call_boundary(
    frame_index: usize,
    entry: &ScriptStateEntry,
    parent: &ScriptStateEntry,
    parent_code: &[u8],
    stack: &[Value],
) -> Result<(usize, i16), VmError> {
    let parent_sp = parent.refs[1] as usize;
    let stack_base = entry.refs[0] as usize;
    if let Some(boundary) = validated_call_boundary(parent_sp, stack_base, stack) {
        return Ok(boundary);
    }
    if let Some(param_count) = saved_parent_call_param_count(parent, parent_code) {
        let entry_sp = parent_sp
            .checked_add(param_count as usize + 1)
            .ok_or_else(|| {
                VmError::Other(
                    format!(
                        "save restore rejected: frame {} CALL boundary overflows",
                        frame_index
                    ),
                )
            })?;
        return validated_call_boundary(parent_sp, entry_sp, stack)
            .ok_or_else(|| {
                VmError::Other(
                    format!(
                        "save restore rejected: frame {} CALL bytecode and marker disagree",
                        frame_index
                    ),
                )
            });
    }
    let max_param_count = stack
        .len()
        .saturating_sub(parent_sp.saturating_add(1))
        .min(i16::MAX as usize);
    let mut matches = (0..=max_param_count)
        .filter_map(|param_count| {
            let marker = stack.get(parent_sp.checked_add(param_count)?)?;
            (marker.as_int() == Some(param_count as i32))
                .then_some((parent_sp + param_count + 1, param_count as i16))
        });
    let Some(boundary) = matches.next() else {
        return Err(
            VmError::Other(
                format!(
                    "save restore rejected: frame {} has no valid CALL marker after parent SP {}",
                    frame_index, parent_sp
                ),
            ),
        );
    };
    if matches.next().is_some() {
        return Err(
            VmError::Other(
                format!(
                    "save restore rejected: frame {} has ambiguous CALL markers after parent SP {}",
                    frame_index, parent_sp
                ),
            ),
        );
    }
    Ok(boundary)
}
fn saved_parent_call_param_count(parent: &ScriptStateEntry, code: &[u8]) -> Option<i16> {
    const CALL_INSTRUCTION_SIZE: usize = 12;
    let return_ip = parent.ip_offset as usize;
    let call_ip = return_ip.checked_sub(CALL_INSTRUCTION_SIZE)?;
    let instruction = code.get(call_ip..return_ip)?;
    let opcode = u16::from_le_bytes(instruction[0..2].try_into().ok()?);
    if !matches!(opcode, 0x80F | 0x810) {
        return None;
    }
    let param_count = i16::from_le_bytes(instruction[10..12].try_into().ok()?);
    (param_count >= 0).then_some(param_count)
}
fn validated_call_boundary(
    parent_sp: usize,
    entry_sp: usize,
    stack: &[Value],
) -> Option<(usize, i16)> {
    let marker_index = entry_sp.checked_sub(1)?;
    let raw_param_count = stack.get(marker_index)?.as_int()?;
    let param_count = i16::try_from(raw_param_count).ok()?;
    if param_count < 0 {
        return None;
    }
    (parent_sp.checked_add(param_count as usize + 1)? == entry_sp)
        .then_some((entry_sp, param_count))
}

