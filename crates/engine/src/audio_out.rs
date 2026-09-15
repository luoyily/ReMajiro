use std::sync::{Arc, OnceLock};
use std::time::Duration;
pub trait AudioStream: Send {
    fn set_volume(&self, gain: f32);
    fn set_speed(&self, ratio: f32);
    fn set_pan(&self, pan: i32);
    fn set_paused(&self, paused: bool);
    fn stop(&self);
    fn is_drained(&self) -> bool;
    fn is_paused(&self) -> bool;
    fn position(&self) -> Duration;
    fn seek(&self, position: Duration) -> Result<(), String>;
}
pub struct ClipPlay {
    pub data: Vec<u8>,
    pub loop_tail: Option<Vec<u8>>,
    pub looped: bool,
    pub gain: f32,
    pub pan: i32,
}
pub struct AttachedClip {
    pub stream: Box<dyn AudioStream>,
    pub sample_rate: u32,
}
pub trait PcmStream: Send {
    fn channels(&self) -> u16;
    fn sample_rate(&self) -> u32;
    fn next_sample(&mut self) -> Option<f32>;
}
pub trait AudioOutBackend: Send + Sync {
    fn default_device_id(&self) -> Option<String>;
    fn open_default_output(&self) -> Result<(), String>;
    fn play_clip(&self, play: ClipPlay) -> Result<AttachedClip, String>;
    fn play_pcm(
        &self,
        stream: Box<dyn PcmStream>,
        gain: f32,
    ) -> Result<Box<dyn AudioStream>, String>;
}
static AUDIO_OUT: OnceLock<Arc<dyn AudioOutBackend>> = OnceLock::new();
pub fn install_audio_out(backend: Arc<dyn AudioOutBackend>) {
    let _ = AUDIO_OUT.set(backend);
}
pub(crate) fn audio_out() -> Arc<dyn AudioOutBackend> {
    AUDIO_OUT.get().cloned().unwrap_or_else(|| Arc::new(SilentAudioOut))
}
pub struct SilentAudioOut;
impl AudioOutBackend for SilentAudioOut {
    fn default_device_id(&self) -> Option<String> {
        None
    }
    fn open_default_output(&self) -> Result<(), String> {
        Err("no audio backend installed in this build".into())
    }
    fn play_clip(&self, _play: ClipPlay) -> Result<AttachedClip, String> {
        Err("no audio backend installed in this build".into())
    }
    fn play_pcm(
        &self,
        _stream: Box<dyn PcmStream>,
        _gain: f32,
    ) -> Result<Box<dyn AudioStream>, String> {
        Err("no audio backend installed in this build".into())
    }
}
