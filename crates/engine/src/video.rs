use crate::audio::AudioBackend;
use crate::audio_out::{AudioStream, PcmStream};
use crate::render_model::{RenderMovieFrame, RenderMoviePixels};
use crate::vfs::Vfs;
use crate::Instant;
use encoding_rs::SHIFT_JIS;
use std::collections::VecDeque;
use std::num::{NonZeroU16, NonZeroU32};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};
#[cfg(not(target_arch = "wasm32"))]
use std::thread;
use std::time::Duration;
const UNITY_VOLUME: i32 = 1000;
const VIDEO_CHANNEL_CAPACITY: usize = 4;
pub const VIDEO_REORDER_DEPTH: usize = 4;
const HOST_FRAME_CAPACITY: usize = 4;
const AUDIO_PREBUFFER: Duration = Duration::from_millis(120);
const VIDEO_DUE_TOLERANCE: Duration = Duration::from_millis(5);
const MAX_AUDIO_SECONDS: usize = 3;
pub trait MovieDecodeBackend: Send + Sync {
    fn decode(&self, args: MovieDecodeArgs<'_>) -> Result<(), String>;
}
pub struct MovieDecodeArgs<'a> {
    pub path: &'a Path,
    pub cancel: &'a AtomicBool,
    pub audio_buffer: &'a MovieAudioBuffer,
    pub event_tx: &'a mpsc::Sender<DecoderEvent>,
    pub frame_tx: &'a mpsc::SyncSender<DecodedFrame>,
}
static MOVIE_DECODE_BACKEND: std::sync::OnceLock<
    std::sync::Arc<dyn MovieDecodeBackend>,
> = std::sync::OnceLock::new();
pub fn install_movie_decode_backend(backend: std::sync::Arc<dyn MovieDecodeBackend>) {
    let _ = MOVIE_DECODE_BACKEND.set(backend);
}
#[cfg(target_arch = "wasm32")]
pub trait RemoteMoviePlayer: Send + Sync {
    fn play(
        &self,
        name: &str,
        cancel: std::sync::Arc<AtomicBool>,
        audio_buffer: std::sync::Arc<MovieAudioBuffer>,
        event_tx: mpsc::Sender<DecoderEvent>,
        frame_tx: mpsc::SyncSender<DecodedFrame>,
    );
}
#[cfg(target_arch = "wasm32")]
static REMOTE_MOVIE_PLAYER: std::sync::OnceLock<std::sync::Arc<dyn RemoteMoviePlayer>> = std::sync::OnceLock::new();
#[cfg(target_arch = "wasm32")]
pub fn install_remote_movie_player(player: std::sync::Arc<dyn RemoteMoviePlayer>) {
    let _ = REMOTE_MOVIE_PLAYER.set(player);
}
#[cfg(not(target_arch = "wasm32"))]
struct UnavailableMovieDecoder;
#[cfg(not(target_arch = "wasm32"))]
impl MovieDecodeBackend for UnavailableMovieDecoder {
    fn decode(&self, _args: MovieDecodeArgs<'_>) -> Result<(), String> {
        Err("movie decoding is currently available only on Windows".into())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudioFormat {
    pub channels: u16,
    pub sample_rate: u32,
}
impl AudioFormat {
    fn samples_for(&self, duration: Duration) -> usize {
        (duration.as_secs_f64() * self.sample_rate as f64 * self.channels as f64)
            as usize
    }
}
#[derive(Debug)]
pub struct DecodedFrame {
    pub pts: Duration,
    pub end_pts: Duration,
    pub width: u32,
    pub height: u32,
    pub pixels: RenderMoviePixels,
}
#[derive(Debug)]
pub enum DecoderEvent {
    Ready { width: u32, height: u32, audio: Option<AudioFormat> },
    Finished { end_pts: Duration },
    Failed(String),
}
#[derive(Default)]
struct AudioQueueState {
    samples: VecDeque<f32>,
    finished: bool,
}
pub struct MovieAudioBuffer {
    state: Mutex<AudioQueueState>,
    space_available: Condvar,
    max_samples: usize,
    underflows: AtomicU64,
    discarded: AtomicBool,
}
impl Default for MovieAudioBuffer {
    fn default() -> Self {
        Self::new()
    }
}
impl MovieAudioBuffer {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(AudioQueueState::default()),
            space_available: Condvar::new(),
            max_samples: 192_000 * 8 * MAX_AUDIO_SECONDS,
            underflows: AtomicU64::new(0),
            discarded: AtomicBool::new(false),
        }
    }
    pub fn push(&self, samples: &[f32], cancel: &AtomicBool) -> bool {
        if self.discarded.load(Ordering::Acquire) {
            return true;
        }
        let mut offset = 0;
        while offset < samples.len() {
            if cancel.load(Ordering::Acquire) {
                return false;
            }
            if self.discarded.load(Ordering::Acquire) {
                return true;
            }
            let mut state = self.state.lock().unwrap();
            while state.samples.len() >= self.max_samples && !state.finished
                && !cancel.load(Ordering::Acquire)
            {
                state = self
                    .space_available
                    .wait_timeout(state, Duration::from_millis(10))
                    .unwrap()
                    .0;
            }
            if state.finished || cancel.load(Ordering::Acquire) {
                return false;
            }
            let writable = (self.max_samples - state.samples.len())
                .min(samples.len() - offset);
            state.samples.extend(samples[offset..offset + writable].iter().copied());
            offset += writable;
        }
        true
    }
    pub fn discard(&self) {
        self.discarded.store(true, Ordering::Release);
        self.state.lock().unwrap().samples.clear();
        self.space_available.notify_all();
    }
    pub fn finish(&self) {
        let mut state = self.state.lock().unwrap();
        state.finished = true;
        self.space_available.notify_all();
    }
    fn available_samples(&self) -> usize {
        self.state.lock().unwrap().samples.len()
    }
    fn is_drained(&self) -> bool {
        let state = self.state.lock().unwrap();
        state.finished && state.samples.is_empty()
    }
}
pub(super) struct MovieAudioSource {
    shared: Arc<MovieAudioBuffer>,
    channels: NonZeroU16,
    sample_rate: NonZeroU32,
}
impl MovieAudioSource {
    fn new(shared: Arc<MovieAudioBuffer>, format: AudioFormat) -> Option<Self> {
        Some(Self {
            shared,
            channels: NonZeroU16::new(format.channels)?,
            sample_rate: NonZeroU32::new(format.sample_rate)?,
        })
    }
}
impl PcmStream for MovieAudioSource {
    fn channels(&self) -> u16 {
        self.channels.get()
    }
    fn sample_rate(&self) -> u32 {
        self.sample_rate.get()
    }
    fn next_sample(&mut self) -> Option<f32> {
        let mut state = self.shared.state.lock().unwrap();
        if let Some(sample) = state.samples.pop_front() {
            self.shared.space_available.notify_one();
            return Some(sample);
        }
        if state.finished {
            return None;
        }
        drop(state);
        self.shared.underflows.fetch_add(1, Ordering::Relaxed);
        Some(0.0)
    }
}
struct MovieSession {
    cancel: Arc<AtomicBool>,
    event_rx: mpsc::Receiver<DecoderEvent>,
    frame_rx: mpsc::Receiver<DecodedFrame>,
    frame_channel_closed: bool,
    frames: VecDeque<DecodedFrame>,
    audio_buffer: Arc<MovieAudioBuffer>,
    audio_format: Option<AudioFormat>,
    audio_stream: Option<Box<dyn AudioStream>>,
    audio_started: bool,
    audio_output_generation: u64,
    audio_clock_offset: Duration,
    fallback_clock: Option<Instant>,
    current_frame: Option<RenderMovieFrame>,
    target: MovieTarget,
    decoder_ready: bool,
    decoder_finished: bool,
    failed: bool,
    latest_end_pts: Duration,
}
impl MovieSession {
    fn stop(mut self) {
        self.cancel.store(true, Ordering::Release);
        self.audio_buffer.finish();
        if let Some(stream) = self.audio_stream.take() {
            stream.stop();
        }
    }
    fn playback_position(&self) -> Duration {
        if let Some(stream) = &self.audio_stream {
            self.audio_clock_offset.saturating_add(stream.position())
        } else {
            self.fallback_clock.map(|started| started.elapsed()).unwrap_or_default()
        }
    }
}
#[derive(Clone, Copy)]
pub struct MovieTarget {
    pub visible: bool,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}
pub struct MoviePlayer {
    volume: i32,
    next_revision: u64,
    session: Option<MovieSession>,
}
impl Default for MoviePlayer {
    fn default() -> Self {
        Self::new()
    }
}
impl MoviePlayer {
    pub fn new() -> Self {
        Self {
            volume: UNITY_VOLUME,
            next_revision: 1,
            session: None,
        }
    }
    pub fn set_volume(&mut self, value: i32) {
        self.volume = value.clamp(0, UNITY_VOLUME);
        if let Some(stream) = self
            .session
            .as_ref()
            .and_then(|session| session.audio_stream.as_ref())
        {
            stream.set_volume(self.volume as f32 / UNITY_VOLUME as f32);
        }
    }
    pub fn play(&mut self, vfs: &Vfs, raw_name: &[u8], target: MovieTarget) -> bool {
        self.cleanup();
        let Some(path) = resolve_movie_path(vfs, raw_name) else {
            eprintln!("[VIDEO] movie file not found: {:?}", decode_movie_name(raw_name));
            return false;
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let audio_buffer = Arc::new(MovieAudioBuffer::new());
        let (event_tx, event_rx) = mpsc::channel();
        let (frame_tx, frame_rx) = mpsc::sync_channel(VIDEO_CHANNEL_CAPACITY);
        spawn_decoder(
            path.clone(),
            cancel.clone(),
            audio_buffer.clone(),
            event_tx,
            frame_tx,
        );
        eprintln!(
            "[VIDEO] opening {} visible={} rect=({},{},{},{}) volume={}", path.display(),
            target.visible, target.x, target.y, target.width, target.height, self.volume
        );
        self.session = Some(MovieSession {
            cancel,
            event_rx,
            frame_rx,
            frame_channel_closed: false,
            frames: VecDeque::new(),
            audio_buffer,
            audio_format: None,
            audio_stream: None,
            audio_started: false,
            audio_output_generation: 0,
            audio_clock_offset: Duration::ZERO,
            fallback_clock: None,
            current_frame: None,
            target,
            decoder_ready: false,
            decoder_finished: false,
            failed: false,
            latest_end_pts: Duration::ZERO,
        });
        true
    }
    pub fn poll(&mut self, audio: &mut AudioBackend) -> bool {
        let Some(session) = self.session.as_mut() else {
            return false;
        };
        while let Ok(event) = session.event_rx.try_recv() {
            match event {
                DecoderEvent::Ready { width, height, audio } => {
                    session.decoder_ready = true;
                    session.audio_format = audio;
                    eprintln!(
                        "[VIDEO] decoder ready {}x{} audio={:?}", width, height, audio
                    );
                    if audio.is_none() {
                        session.fallback_clock = Some(Instant::now());
                    }
                }
                DecoderEvent::Finished { end_pts } => {
                    session.decoder_finished = true;
                    session.latest_end_pts = session.latest_end_pts.max(end_pts);
                    session.audio_buffer.finish();
                }
                DecoderEvent::Failed(error) => {
                    eprintln!("[VIDEO] decoder failed: {error}");
                    session.failed = true;
                    session.decoder_finished = true;
                    session.audio_buffer.finish();
                }
            }
        }
        while session.frames.len() < HOST_FRAME_CAPACITY {
            match session.frame_rx.try_recv() {
                Ok(frame) => {
                    session.latest_end_pts = session.latest_end_pts.max(frame.end_pts);
                    session.frames.push_back(frame);
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    session.frame_channel_closed = true;
                    session.decoder_finished = true;
                    break;
                }
            }
        }
        if session.audio_stream.is_some()
            && session.audio_output_generation != audio.output_generation()
        {
            if let Some(stream) = session.audio_stream.take() {
                session.audio_clock_offset = session
                    .audio_clock_offset
                    .saturating_add(stream.position());
                stream.stop();
            }
        }
        if session.audio_stream.is_none() {
            if let Some(format) = session.audio_format {
                let enough_audio = session.audio_buffer.available_samples()
                    >= format.samples_for(AUDIO_PREBUFFER);
                if enough_audio || session.decoder_finished || session.audio_started {
                    if let Some(source) = MovieAudioSource::new(
                        session.audio_buffer.clone(),
                        format,
                    ) {
                        if let Some((stream, generation)) = audio
                            .start_movie(source, self.volume.clamp(0, UNITY_VOLUME))
                        {
                            session.audio_stream = Some(stream);
                            session.audio_started = true;
                            session.audio_output_generation = generation;
                        }
                    }
                    if session.audio_stream.is_none() {
                        session.audio_buffer.discard();
                        session.fallback_clock.get_or_insert_with(Instant::now);
                    }
                }
            } else if session.decoder_ready {
                session.fallback_clock.get_or_insert_with(Instant::now);
            }
        }
        let mut clock = session.playback_position();
        let audio_exhausted = session
            .audio_stream
            .as_ref()
            .is_some_and(|stream| stream.is_drained())
            && session.audio_buffer.is_drained();
        if session.decoder_finished && audio_exhausted {
            clock = clock.max(session.latest_end_pts);
        }
        let mut changed = false;
        while session
            .frames
            .front()
            .is_some_and(|frame| frame.pts <= clock + VIDEO_DUE_TOLERANCE)
        {
            let frame = session.frames.pop_front().unwrap();
            if session.target.visible {
                let output_width = if session.target.width > 0 {
                    session.target.width as f32
                } else {
                    frame.width as f32
                };
                let output_height = if session.target.height > 0 {
                    session.target.height as f32
                } else {
                    frame.height as f32
                };
                session.current_frame = Some(RenderMovieFrame {
                    revision: self.next_revision,
                    width: frame.width,
                    height: frame.height,
                    pixels: frame.pixels,
                    x: session.target.x as f32,
                    y: session.target.y as f32,
                    output_width,
                    output_height,
                });
                self.next_revision = self.next_revision.wrapping_add(1).max(1);
                changed = true;
            }
        }
        changed
    }
    pub fn is_playing(&mut self, audio: &mut AudioBackend) -> (bool, bool) {
        let changed = self.poll(audio);
        let Some(session) = self.session.as_ref() else {
            return (false, changed);
        };
        if session.failed {
            return (false, changed);
        }
        let clock_done = session.playback_position() >= session.latest_end_pts;
        let audio_done = if let Some(stream) = session.audio_stream.as_ref() {
            stream.is_drained() && session.audio_buffer.is_drained()
        } else {
            clock_done
        };
        let frames_done = session.frames.is_empty() && session.frame_channel_closed;
        let timeline_done = if session.audio_stream.is_some() {
            audio_done
        } else {
            clock_done
        };
        let finished = session.decoder_finished && frames_done && timeline_done;
        (!finished, changed)
    }
    pub fn audio_attached(&self) -> bool {
        self.session.as_ref().is_some_and(|session| session.audio_stream.is_some())
    }
    pub fn current_frame(&self) -> Option<RenderMovieFrame> {
        self.session.as_ref().and_then(|session| session.current_frame.clone())
    }
    pub fn cleanup(&mut self) -> bool {
        let had_overlay = self
            .session
            .as_ref()
            .is_some_and(|session| session.current_frame.is_some());
        if let Some(session) = self.session.take() {
            let underflows = session.audio_buffer.underflows.load(Ordering::Relaxed);
            if underflows != 0 {
                eprintln!("[VIDEO] audio underflow samples={underflows}");
            }
            session.stop();
        }
        had_overlay
    }
}
fn decode_movie_name(raw_name: &[u8]) -> String {
    let raw_name = raw_name.split(|byte| *byte == 0).next().unwrap_or(raw_name);
    SHIFT_JIS.decode_without_bom_handling(raw_name).0.into_owned()
}
#[cfg(not(target_arch = "wasm32"))]
fn resolve_movie_name(vfs: &Vfs, name: &str) -> Option<PathBuf> {
    vfs.find_path(name).or_else(|| vfs.find_path(&format!("movie/{name}")))
}
#[cfg(target_arch = "wasm32")]
fn resolve_movie_name_remote(vfs: &Vfs, name: &str) -> Option<PathBuf> {
    vfs.find_remote_path(name)
        .or_else(|| vfs.find_remote_path(&format!("movie/{name}")))
        .map(PathBuf::from)
}
#[cfg(target_arch = "wasm32")]
fn mp4_twin_name(name: &str) -> Option<String> {
    let (stem, extension) = name.rsplit_once('.')?;
    (!extension.eq_ignore_ascii_case("mp4")).then(|| format!("{stem}.mp4"))
}
#[cfg(not(target_arch = "wasm32"))]
fn mpg_twin_name(name: &str) -> Option<String> {
    let (stem, extension) = name.rsplit_once('.')?;
    extension.eq_ignore_ascii_case("mpg").then(|| format!("{stem}.mp4"))
}
fn resolve_movie_path(vfs: &Vfs, raw_name: &[u8]) -> Option<PathBuf> {
    let name = decode_movie_name(raw_name);
    #[cfg(target_arch = "wasm32")]
    let lookup = |candidate: &str| resolve_movie_name_remote(vfs, candidate);
    #[cfg(not(target_arch = "wasm32"))]
    let lookup = |candidate: &str| resolve_movie_name(vfs, candidate);
    let direct = lookup(&name);
    #[cfg(target_arch = "wasm32")]
    let twin = mp4_twin_name(&name).and_then(|fallback| lookup(&fallback));
    #[cfg(not(target_arch = "wasm32"))]
    let twin = mpg_twin_name(&name).and_then(|fallback| lookup(&fallback));
    #[cfg(target_arch = "wasm32")]
    let resolved = twin.or(direct);
    #[cfg(not(target_arch = "wasm32"))]
    let resolved = direct.or(twin);
    resolved
}
pub fn spawn_decoder(
    path: PathBuf,
    cancel: Arc<AtomicBool>,
    audio_buffer: Arc<MovieAudioBuffer>,
    event_tx: mpsc::Sender<DecoderEvent>,
    frame_tx: mpsc::SyncSender<DecodedFrame>,
) {
    #[cfg(target_arch = "wasm32")]
    {
        match REMOTE_MOVIE_PLAYER.get() {
            Some(player) => {
                player
                    .play(
                        &path.to_string_lossy(),
                        cancel,
                        audio_buffer,
                        event_tx,
                        frame_tx,
                    )
            }
            None => {
                audio_buffer.finish();
                let _ = event_tx
                    .send(
                        DecoderEvent::Failed("no remote movie player installed".into()),
                    );
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let backend = MOVIE_DECODE_BACKEND
            .get()
            .cloned()
            .unwrap_or_else(|| std::sync::Arc::new(UnavailableMovieDecoder));
        let builder = thread::Builder::new().name("ostb-movie-decoder".into());
        let spawn_error_tx = event_tx.clone();
        let spawn_result = builder
            .spawn(move || {
                let args = MovieDecodeArgs {
                    path: &path,
                    cancel: &cancel,
                    audio_buffer: &audio_buffer,
                    event_tx: &event_tx,
                    frame_tx: &frame_tx,
                };
                if let Err(error) = backend.decode(args) {
                    audio_buffer.finish();
                    let _ = event_tx.send(DecoderEvent::Failed(error));
                }
            });
        if let Err(error) = spawn_result {
            let _ = spawn_error_tx
                .send(
                    DecoderEvent::Failed(
                        format!("failed to spawn decoder thread: {error}"),
                    ),
                );
        }
    }
}

