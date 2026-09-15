use engine::audio_out::{AttachedClip, AudioOutBackend, AudioStream, ClipPlay, PcmStream};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use wasm_bindgen::prelude::*;
static NEXT_STREAM_ID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(
    1,
);
pub(crate) fn install(ended: Arc<Mutex<HashSet<u32>>>) {
    let backend = Arc::new(WebAudioOut::new(ended));
    engine::audio_out::install_audio_out(backend);
}
fn now_secs() -> f64 {
    js_sys::Date::now() / 1000.0
}
pub(crate) struct WebAudioOut {
    post: js_sys::Function,
    post_this: wasm_bindgen::JsValue,
    ended: Arc<Mutex<HashSet<u32>>>,
}
unsafe impl Send for WebAudioOut {}
unsafe impl Sync for WebAudioOut {}
impl WebAudioOut {
    pub(crate) fn new(ended: Arc<Mutex<HashSet<u32>>>) -> Self {
        let global = js_sys::global();
        let post = js_sys::Function::from(
            js_sys::Reflect::get(&global, &"postMessage".into())
                .expect("worker postMessage"),
        );
        Self {
            post,
            post_this: global.into(),
            ended,
        }
    }
    fn send(&self, build: impl FnOnce(&js_sys::Object)) {
        let message = js_sys::Object::new();
        build(&message);
        let _ = self.post.call1(&self.post_this, &message);
    }
}
impl AudioOutBackend for WebAudioOut {
    fn default_device_id(&self) -> Option<String> {
        None
    }
    fn open_default_output(&self) -> Result<(), String> {
        Ok(())
    }
    fn play_clip(&self, play: ClipPlay) -> Result<AttachedClip, String> {
        let (sample_rate, _channels) = probe_header_sample_rate(&play.data)
            .ok_or("audio header: unsupported format")?;
        let id = NEXT_STREAM_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.ended.lock().unwrap().remove(&id);
        let tail = play
            .loop_tail
            .as_ref()
            .map(|bytes| js_sys::Uint8Array::from(bytes.as_slice()).into());
        let gain = play.gain.clamp(0.0, 1.0);
        self.send(|message| {
            let set = |key: &str, value: JsValue| {
                let _ = js_sys::Reflect::set(message, &key.into(), &value);
            };
            set("t", "aplay".into());
            set("id", id.into());
            set("data", js_sys::Uint8Array::from(play.data.as_slice()).into());
            set("tail", tail.unwrap_or(JsValue::NULL));
            set("looped", play.looped.into());
            set("gain", gain.into());
            set("pan", play.pan.into());
        });
        Ok(AttachedClip {
            stream: Box::new(WebClipStream {
                post: self.post.clone(),
                post_this: self.post_this.clone(),
                id,
                ended: Arc::clone(&self.ended),
                base_offset: std::cell::Cell::new(0.0),
                started_at: std::cell::Cell::new(Some(now_secs())),
                speed: std::cell::Cell::new(1.0),
                paused: std::cell::Cell::new(false),
                stopped: std::cell::Cell::new(false),
            }),
            sample_rate,
        })
    }
    fn play_pcm(
        &self,
        _stream: Box<dyn PcmStream>,
        _gain: f32,
    ) -> Result<Box<dyn AudioStream>, String> {
        Err("movie PCM audio is not wired on the web yet (R7)".into())
    }
}
fn probe_header_sample_rate(data: &[u8]) -> Option<(u32, u16)> {
    if data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WAVE" {
        let mut pos = 12usize;
        while pos + 8 <= data.len() {
            let id = &data[pos..pos + 4];
            let size = u32::from_le_bytes(data[pos + 4..pos + 8].try_into().ok()?)
                as usize;
            if id == b"fmt " && pos + 16 <= data.len() {
                let channels = u16::from_le_bytes(
                    data[pos + 8..pos + 10].try_into().ok()?,
                );
                let rate = u32::from_le_bytes(data[pos + 12..pos + 16].try_into().ok()?);
                return Some((rate, channels));
            }
            pos += 8 + size + (size & 1);
        }
        return None;
    }
    if data.len() >= 64 && &data[0..4] == b"OggS" {
        if let Some(pos) = data.windows(7).position(|window| window == b"\x01vorbis") {
            let channels = *data.get(pos + 11)?;
            let rate = u32::from_le_bytes(
                data.get(pos + 12..pos + 16)?.try_into().ok()?,
            );
            return Some((rate, u16::from(channels)));
        }
    }
    None
}
pub(crate) struct WebClipStream {
    post: js_sys::Function,
    post_this: wasm_bindgen::JsValue,
    id: u32,
    ended: Arc<Mutex<HashSet<u32>>>,
    base_offset: std::cell::Cell<f64>,
    started_at: std::cell::Cell<Option<f64>>,
    speed: std::cell::Cell<f64>,
    paused: std::cell::Cell<bool>,
    stopped: std::cell::Cell<bool>,
}
unsafe impl Send for WebClipStream {}
impl WebClipStream {
    fn send(&self, kind: &str, extra: impl FnOnce(&js_sys::Object)) {
        let message = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&message, &"t".into(), &kind.into());
        let _ = js_sys::Reflect::set(&message, &"id".into(), &self.id.into());
        extra(&message);
        let _ = self.post.call1(&self.post_this, &message);
    }
    fn material_position(&self) -> Duration {
        let Some(started_at) = self.started_at.get() else {
            return Duration::from_secs_f64(self.base_offset.get());
        };
        let elapsed = (now_secs() - started_at).max(0.0) * self.speed.get();
        Duration::from_secs_f64(self.base_offset.get() + elapsed)
    }
}
impl AudioStream for WebClipStream {
    fn set_volume(&self, gain: f32) {
        let gain = gain.clamp(0.0, 1.0);
        self.send(
            "avol",
            |message| {
                let _ = js_sys::Reflect::set(message, &"v".into(), &gain.into());
            },
        );
    }
    fn set_speed(&self, ratio: f32) {
        let now = self.material_position().as_secs_f64();
        self.base_offset.set(now);
        if self.started_at.get().is_some() {
            self.started_at.set(Some(now_secs()));
        }
        self.speed.set(ratio as f64);
        self.send(
            "aspd",
            |message| {
                let _ = js_sys::Reflect::set(
                    message,
                    &"r".into(),
                    &(ratio as f64).into(),
                );
            },
        );
    }
    fn set_pan(&self, pan: i32) {
        self.send(
            "apan",
            |message| {
                let _ = js_sys::Reflect::set(message, &"pan".into(), &pan.into());
            },
        );
    }
    fn set_paused(&self, paused: bool) {
        if self.paused.get() == paused {
            return;
        }
        if paused {
            let position = self.material_position().as_secs_f64();
            self.base_offset.set(position);
            self.started_at.set(None);
            self.paused.set(true);
        } else {
            self.paused.set(false);
            self.started_at.set(Some(now_secs()));
        }
        self.send(
            "apause",
            |message| {
                let _ = js_sys::Reflect::set(message, &"p".into(), &paused.into());
            },
        );
    }
    fn stop(&self) {
        self.stopped.set(true);
        self.ended.lock().unwrap().insert(self.id);
        self.send("astop", |_| {});
    }
    fn is_drained(&self) -> bool {
        self.stopped.get() || self.ended.lock().unwrap().contains(&self.id)
    }
    fn is_paused(&self) -> bool {
        self.paused.get()
    }
    fn position(&self) -> Duration {
        self.material_position()
    }
    fn seek(&self, position: Duration) -> Result<(), String> {
        self.base_offset.set(position.as_secs_f64());
        if self.started_at.get().is_some() {
            self.started_at.set(Some(now_secs()));
        }
        self.send(
            "aseek",
            |message| {
                let _ = js_sys::Reflect::set(
                    message,
                    &"ms".into(),
                    &(position.as_secs_f64() * 1000.0).into(),
                );
            },
        );
        Ok(())
    }
}
