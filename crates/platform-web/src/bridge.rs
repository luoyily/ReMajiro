use formats::remote::RemoteReader;
use js_sys::{Atomics, Int32Array, Uint8Array};
use std::cell::Cell;
use wasm_bindgen::{JsCast, JsValue};
const WINDOW_BYTES: u32 = 32 * 1024 * 1024;
const WINDOW_OFFSET: u32 = 64;
const SEQ: u32 = 0;
const STATUS: u32 = 1;
const LEN: u32 = 2;
const WAIT_TIMEOUT_MS: f64 = 15_000.0;
#[derive(Clone)]
pub(crate) struct Bridge {
    sab: js_sys::SharedArrayBuffer,
    ints: Int32Array,
    post: js_sys::Function,
    pending_msgbox: Cell<i32>,
}
unsafe impl Send for Bridge {}
unsafe impl Sync for Bridge {}
impl Bridge {
    pub(crate) fn new(sab: js_sys::SharedArrayBuffer) -> Result<Self, String> {
        let scope = worker_scope()?;
        let post = js_sys::Function::from(
            js_sys::Reflect::get(&scope, &"postMessage".into())
                .map_err(|error| format!("worker postMessage unavailable: {error:?}"))?,
        );
        Ok(Self {
            ints: Int32Array::new(&sab),
            sab,
            post,
            pending_msgbox: Cell::new(0),
        })
    }
    fn post_request(
        &self,
        kind: &str,
        id: u32,
        name: &str,
        offset: u64,
        len: u32,
    ) -> Result<(), String> {
        let scope = worker_scope()?;
        let message = js_sys::Object::new();
        let set = |key: &str, value: JsValue| {
            js_sys::Reflect::set(&message, &key.into(), &value)
                .map(|_| ())
                .map_err(|error| format!("request build failed at {key}: {error:?}"))
        };
        set("t", kind.into())?;
        set("id", id.into())?;
        set("name", name.into())?;
        set("off", (offset as f64).into())?;
        set("len", len.into())?;
        self.post
            .call1(&scope, &message)
            .map(|_| ())
            .map_err(|error| format!("post to IO service failed: {error:?}"))
    }
    fn wait_done(&self, id: u32) -> Result<(bool, u32), String> {
        loop {
            let current = Atomics::load(&self.ints, SEQ)
                .map_err(|error| format!("Atomics.load failed: {error:?}"))?;
            if current == id as i32 {
                let status = Atomics::load(&self.ints, STATUS)
                    .map_err(|error| format!("Atomics.load failed: {error:?}"))?;
                let len = Atomics::load(&self.ints, LEN)
                    .map_err(|error| format!("Atomics.load failed: {error:?}"))?;
                return Ok((status == 1, len as u32));
            }
            let result = Atomics::wait_with_timeout(
                    &self.ints,
                    SEQ,
                    current,
                    WAIT_TIMEOUT_MS,
                )
                .map_err(|error| format!("Atomics.wait failed: {error:?}"))?;
            if result.as_string().as_deref() == Some("timed-out") {
                let latest = Atomics::load(&self.ints, SEQ)
                    .map_err(|error| format!("Atomics.load failed: {error:?}"))?;
                if latest == id as i32 {
                    continue;
                }
                return Err("IO bridge timed out waiting for the IO service".into());
            }
        }
    }
    fn data_window(&self, len: u32) -> Uint8Array {
        Uint8Array::new_with_byte_offset_and_length(&self.sab, WINDOW_OFFSET, len)
    }
    pub(crate) fn message_box(&self, text: &str) {
        static NEXT_ID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(
            2_000_000_000,
        );
        let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.pending_msgbox.set(id as i32);
        let forward = (|| -> Result<(), String> {
            let scope = worker_scope()?;
            let message = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&message, &"t".into(), &"msgbox".into());
            let _ = js_sys::Reflect::set(&message, &"id".into(), &id.into());
            let _ = js_sys::Reflect::set(&message, &"text".into(), &text.into());
            self.post
                .call1(&scope, &message)
                .map(|_| ())
                .map_err(|error| format!("post failed: {error:?}"))
        })();
        if let Err(error) = forward {
            web_sys::console::error_1(
                &format!("[MSGBOX] {text} (forward failed: {error})").into(),
            );
            return;
        }
        if let Err(error) = self.wait_done(id).map(|_| ()) {
            web_sys::console::error_1(
                &format!("[MSGBOX] {text} (wait failed: {error})").into(),
            );
        }
    }
}
impl RemoteReader for Bridge {
    fn read_at(&self, name: &str, offset: u64, size: u32) -> Result<Vec<u8>, String> {
        static NEXT_ID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(
            1,
        );
        let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut out = Vec::with_capacity(size as usize);
        let mut done = 0u32;
        while done < size {
            let len = (size - done).min(WINDOW_BYTES);
            self.post_request("read", id, name, offset + u64::from(done), len)?;
            let (ok, actual) = self.wait_done(id)?;
            if !ok {
                return Err(format!("read {name}+{done} failed at the IO service"));
            }
            if actual == 0 {
                return Err(format!("read {name}+{done}: empty response window"));
            }
            let chunk = self.data_window(actual).to_vec();
            out.extend_from_slice(&chunk);
            done += actual;
            if actual < len && done < size {
                return Err(
                    format!(
                        "read {name}: short chunk {actual}/{len} at offset {}", offset +
                        u64::from(done - actual)
                    ),
                );
            }
        }
        Ok(out)
    }
    fn size_of(&self, name: &str) -> Result<u64, String> {
        static NEXT_ID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(
            1_000_000_000,
        );
        let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.post_request("stat", id, name, 0, 0)?;
        let (ok, len) = self.wait_done(id)?;
        if !ok {
            return Err(format!("stat {name} failed at the IO service"));
        }
        Ok(u64::from(len))
    }
}
fn worker_scope() -> Result<web_sys::DedicatedWorkerGlobalScope, String> {
    js_sys::global()
        .dyn_into::<web_sys::DedicatedWorkerGlobalScope>()
        .map_err(|_| "not running inside a dedicated worker".to_string())
}
