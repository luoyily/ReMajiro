use engine::video::{DecodedFrame, DecoderEvent, MovieAudioBuffer, RemoteMoviePlayer};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Sender, SyncSender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use wasm_bindgen::prelude::*;
struct ActiveSession {
    cancel: Arc<AtomicBool>,
    event_tx: Sender<DecoderEvent>,
    _frame_tx: SyncSender<DecodedFrame>,
}
static POST: OnceLock<(js_sys::Function, JsValue)> = OnceLock::new();
static ACTIVE: Mutex<Option<ActiveSession>> = Mutex::new(None);
pub(crate) fn install() {
    let global = js_sys::global();
    let post = js_sys::Function::from(
        js_sys::Reflect::get(&global, &"postMessage".into()).expect("worker postMessage"),
    );
    let _ = POST.set((post, global.into()));
}
pub(crate) fn install_player() {
    engine::video::install_remote_movie_player(Arc::new(RemoteMovie));
}
fn post(kind: &str, build: impl FnOnce(&js_sys::Object)) {
    let Some((post, post_this)) = POST.get() else {
        return;
    };
    let message = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&message, &"t".into(), &kind.into());
    build(&message);
    let _ = post.call1(post_this, &message);
}
struct RemoteMovie;
impl RemoteMoviePlayer for RemoteMovie {
    fn play(
        &self,
        name: &str,
        cancel: Arc<AtomicBool>,
        _audio_buffer: Arc<MovieAudioBuffer>,
        event_tx: Sender<DecoderEvent>,
        frame_tx: SyncSender<DecodedFrame>,
    ) {
        *ACTIVE.lock().unwrap() = Some(ActiveSession {
            cancel,
            event_tx: event_tx.clone(),
            _frame_tx: frame_tx,
        });
        let _ = event_tx
            .send(DecoderEvent::Ready {
                width: 0,
                height: 0,
                audio: None,
            });
        post(
            "movie-play",
            |message| {
                let _ = js_sys::Reflect::set(message, &"name".into(), &name.into());
            },
        );
    }
}
fn finish_session(session: ActiveSession, end_pts: Duration) {
    drop(session._frame_tx);
    let _ = session.event_tx.send(DecoderEvent::Finished { end_pts });
}
pub(crate) fn notify_ended(duration_ms: f64) {
    eprintln!("[VIDEO] page movie-ended ms={duration_ms}");
    let session = ACTIVE.lock().unwrap().take();
    if let Some(session) = session {
        finish_session(session, Duration::ZERO);
    }
}
pub(crate) fn poll_tick() {
    let mut active = ACTIVE.lock().unwrap();
    let cancelled = active
        .as_ref()
        .is_some_and(|session| session.cancel.load(Ordering::Acquire));
    if cancelled {
        let session = active.take().expect("session checked above");
        drop(active);
        eprintln!("[VIDEO] engine teardown relayed -> movie-stop");
        post("movie-stop", |_| {});
        finish_session(session, Duration::ZERO);
    }
}
