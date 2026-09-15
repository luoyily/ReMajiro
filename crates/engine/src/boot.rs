use crate::storage::SaveStore;
use std::rc::Rc;
use formats::save::{read_mss, read_readmarks, ComplexPayload, SerValue};
use vm::value::{Scope, Value, ValueData, TAG_INT_ARRAY, TAG_STRING_ARRAY};
use vm::Vm;
const POOL_CAPACITY: usize = 1024;
pub struct EngineBoot;
impl EngineBoot {
    pub fn init(vm: &mut Vm, storage: Option<&dyn SaveStore>) {
        if let Some(storage) = storage {
            match storage.read("majiro_system.mss") {
                Ok(Some(bytes)) => {
                    match read_mss(&bytes) {
                        Ok(system) => {
                            let count = system.values.len();
                            vm.restore_system_globals(
                                system
                                    .keys
                                    .into_iter()
                                    .zip(system.values.into_iter().map(value_from_save)),
                            );
                            eprintln!(
                                "[BOOT] loaded {} system globals from majiro_system.mss",
                                count
                            );
                        }
                        Err(error) => {
                            eprintln!(
                                "[BOOT] rejected system save majiro_system.mss: {} (using first-launch state)",
                                error
                            );
                        }
                    }
                }
                Ok(None) => {
                    eprintln!("[BOOT] system save not found; using first-launch state");
                }
                Err(error) => {
                    eprintln!(
                        "[BOOT] failed to read system save majiro_system.mss: {} (using first-launch state)",
                        error
                    );
                }
            }
            match storage.read("majiro_readmark.mss") {
                Ok(Some(bytes)) => {
                    match read_readmarks(&bytes) {
                        Ok(records) => {
                            let count = records.len();
                            vm.restore_readmarks(records);
                            eprintln!(
                                "[BOOT] loaded {} script readmark records from majiro_readmark.mss",
                                count
                            );
                        }
                        Err(error) => {
                            eprintln!(
                                "[BOOT] rejected readmark save majiro_readmark.mss: {}",
                                error
                            )
                        }
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    eprintln!(
                        "[BOOT] failed to read readmark save majiro_readmark.mss: {}",
                        error
                    )
                }
            }
        }
        vm.globals.reserve(POOL_CAPACITY);
        vm.locals.reserve(POOL_CAPACITY.saturating_sub(vm.locals.capacity()));
        vm.threads.reserve(POOL_CAPACITY.saturating_sub(vm.threads.capacity()));
        eprintln!(
            "[BOOT] variable pools ready: globals={} values/{} capacity, locals={}, threads={}",
            vm.globals.len(), vm.globals.capacity(), vm.locals.capacity(), vm.threads
            .capacity(),
        );
    }
}
fn value_from_save(value: SerValue) -> Value {
    match value {
        SerValue::Int(value) => Value::int(value),
        SerValue::Float(value) => Value::float(value),
        SerValue::String(value) => Value::string(value.as_ref().to_vec()),
        SerValue::Array(value) => {
            Value {
                scope: Scope::Global,
                type_tag: TAG_INT_ARRAY,
                bits: 0,
                data: Some(
                    Rc::new(ValueData::IntArray {
                        ndim: dimension_count(value.dims),
                        dims: value.dims,
                        cells: value.cells.iter().copied().map(Value::int).collect(),
                    }),
                ),
            }
        }
        SerValue::Complex(value) => complex_from_save(&value),
    }
}
fn complex_from_save(value: &ComplexPayload) -> Value {
    let cells = value
        .entries
        .iter()
        .map(|entry| match entry {
            Some(bytes) => Value::string(bytes.as_ref().to_vec()),
            None => Value::string(vec![0]),
        })
        .collect();
    Value {
        scope: Scope::Global,
        type_tag: TAG_STRING_ARRAY,
        bits: 0,
        data: Some(
            Rc::new(ValueData::StringArray {
                dims: value.dims,
                cells,
            }),
        ),
    }
}
fn dimension_count(dims: [u32; 3]) -> u8 {
    if dims[2] > 1 { 3 } else if dims[1] > 1 { 2 } else { 1 }
}

