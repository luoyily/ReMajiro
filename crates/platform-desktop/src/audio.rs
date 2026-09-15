use engine::audio_out::{AttachedClip, AudioOutBackend, AudioStream, ClipPlay, PcmStream};
use rodio::{
    cpal::{
        traits::{DeviceTrait, HostTrait},
        DeviceId,
    },
    source::SeekError, ChannelCount, Decoder, DeviceSinkBuilder, MixerDeviceSink, Player,
    SampleRate, Source,
};
use std::io::Cursor;
use std::sync::{Arc, Mutex};
use std::time::Duration;
pub fn install() {
    engine::audio_out::install_audio_out(Arc::new(RodioAudioOut::new()));
}
pub struct RodioAudioOut {
    inner: Mutex<OutputState>,
}
#[derive(Default)]
struct OutputState {
    sink: Option<MixerDeviceSink>,
}
impl RodioAudioOut {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(OutputState::default()),
        }
    }
    fn mixer(&self) -> Result<rodio::mixer::Mixer, String> {
        self.inner
            .lock()
            .unwrap()
            .sink
            .as_ref()
            .map(|sink| sink.mixer().clone())
            .ok_or_else(|| "audio output is not open".into())
    }
}
impl Default for RodioAudioOut {
    fn default() -> Self {
        Self::new()
    }
}
impl AudioOutBackend for RodioAudioOut {
    fn default_device_id(&self) -> Option<String> {
        let device = rodio::cpal::default_host().default_output_device()?;
        device.id().ok().map(|id| id.to_string())
    }
    fn open_default_output(&self) -> Result<(), String> {
        let (sink, _device_id) = open_default_output()?;
        self.inner.lock().unwrap().sink = Some(sink);
        Ok(())
    }
    fn play_clip(&self, play: ClipPlay) -> Result<AttachedClip, String> {
        let mixer = self.mixer()?;
        let player = Player::connect_new(&mixer);
        let pan = Arc::new(std::sync::atomic::AtomicI32::new(play.pan));
        let decode = |data: Vec<u8>| {
            Decoder::try_from(Cursor::new(data))
                .map_err(|error| format!("decode: {error}"))
        };
        let sample_rate;
        if play.looped {
            match play.loop_tail {
                Some(tail) => {
                    let first = decode(play.data)?;
                    sample_rate = first.sample_rate().get();
                    player.append(PanSource::new(first, Arc::clone(&pan)));
                    let tail = Decoder::new_looped(Cursor::new(tail))
                        .map_err(|error| format!("decode loop tail: {error}"))?;
                    player.append(PanSource::new(tail, Arc::clone(&pan)));
                }
                None => {
                    let source = Decoder::new_looped(Cursor::new(play.data))
                        .map_err(|error| format!("decode: {error}"))?;
                    sample_rate = source.sample_rate().get();
                    player.append(PanSource::new(source, Arc::clone(&pan)));
                }
            }
        } else {
            let first = decode(play.data)?;
            sample_rate = first.sample_rate().get();
            player.append(PanSource::new(first, Arc::clone(&pan)));
        }
        player.set_volume(play.gain);
        Ok(AttachedClip {
            stream: Box::new(RodioStream { player, pan }),
            sample_rate,
        })
    }
    fn play_pcm(
        &self,
        stream: Box<dyn PcmStream>,
        gain: f32,
    ) -> Result<Box<dyn AudioStream>, String> {
        let mixer = self.mixer()?;
        let player = Player::connect_new(&mixer);
        player.append(PcmSource { inner: stream });
        player.set_volume(gain);
        Ok(
            Box::new(RodioStream {
                player,
                pan: Arc::new(std::sync::atomic::AtomicI32::new(0)),
            }),
        )
    }
}
fn open_default_output() -> Result<(MixerDeviceSink, DeviceId), String> {
    let device = rodio::cpal::default_host()
        .default_output_device()
        .ok_or_else(|| "no default output device".to_string())?;
    let device_id = device
        .id()
        .map_err(|error| format!("failed to identify default output device: {error}"))?;
    let builder = DeviceSinkBuilder::from_device(device)
        .map_err(|error| format!("failed to configure default output device: {error}"))?;
    let mut sink = builder
        .open_sink_or_fallback()
        .map_err(|error| format!("failed to open default output device: {error}"))?;
    sink.log_on_drop(false);
    Ok((sink, device_id))
}
pub struct RodioStream {
    player: Player,
    pan: Arc<std::sync::atomic::AtomicI32>,
}
impl AudioStream for RodioStream {
    fn set_volume(&self, gain: f32) {
        self.player.set_volume(gain);
    }
    fn set_speed(&self, ratio: f32) {
        self.player.set_speed(ratio);
    }
    fn set_pan(&self, pan: i32) {
        self.pan.store(pan, std::sync::atomic::Ordering::Relaxed);
    }
    fn set_paused(&self, paused: bool) {
        if paused {
            self.player.pause();
        } else {
            self.player.play();
        }
    }
    fn stop(&self) {
        self.player.stop();
    }
    fn is_drained(&self) -> bool {
        self.player.empty()
    }
    fn is_paused(&self) -> bool {
        self.player.is_paused()
    }
    fn position(&self) -> Duration {
        self.player.get_pos()
    }
    fn seek(&self, position: Duration) -> Result<(), String> {
        self.player.try_seek(position).map_err(|error: SeekError| error.to_string())
    }
}
struct PcmSource {
    inner: Box<dyn PcmStream>,
}
impl Iterator for PcmSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        self.inner.next_sample()
    }
}
impl Source for PcmSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> ChannelCount {
        ChannelCount::new(self.inner.channels()).expect("channels must be non-zero")
    }
    fn sample_rate(&self) -> SampleRate {
        SampleRate::new(self.inner.sample_rate()).expect("sample rate must be non-zero")
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}
pub(crate) struct PanSource<S> {
    input: S,
    pan: Arc<std::sync::atomic::AtomicI32>,
    channel_index: u16,
    pending_mono_right: Option<f32>,
}
impl<S> PanSource<S> {
    pub(crate) fn new(input: S, pan: Arc<std::sync::atomic::AtomicI32>) -> Self {
        Self {
            input,
            pan,
            channel_index: 0,
            pending_mono_right: None,
        }
    }
    fn gains(&self) -> (f32, f32) {
        let pan = self
            .pan
            .load(std::sync::atomic::Ordering::Relaxed)
            .clamp(-10_000, 10_000);
        if pan < 0 {
            (1.0, 10.0_f32.powf(pan as f32 / 2000.0))
        } else if pan > 0 {
            (10.0_f32.powf(-(pan as f32) / 2000.0), 1.0)
        } else {
            (1.0, 1.0)
        }
    }
}
impl<S> Iterator for PanSource<S>
where
    S: Source,
{
    type Item = f32;
    fn next(&mut self) -> Option<Self::Item> {
        if self.input.channels().get() == 1 {
            if let Some(right) = self.pending_mono_right.take() {
                return Some(right);
            }
            let sample = self.input.next()?;
            let (left_gain, right_gain) = self.gains();
            self.pending_mono_right = Some(sample * right_gain);
            return Some(sample * left_gain);
        }
        let sample = self.input.next()?;
        let channels = self.input.channels().get();
        let (left_gain, right_gain) = self.gains();
        let gain = match self.channel_index {
            0 => left_gain,
            1 => right_gain,
            _ => 1.0,
        };
        self.channel_index = (self.channel_index + 1) % channels;
        Some(sample * gain)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let (lower, upper) = self.input.size_hint();
        if self.input.channels().get() == 1 {
            let pending = usize::from(self.pending_mono_right.is_some());
            (
                lower.saturating_mul(2).saturating_add(pending),
                upper.map(|value| value.saturating_mul(2).saturating_add(pending)),
            )
        } else {
            (lower, upper)
        }
    }
}
impl<S> Source for PanSource<S>
where
    S: Source,
{
    fn current_span_len(&self) -> Option<usize> {
        let len = self.input.current_span_len()?;
        if self.input.channels().get() == 1 {
            Some(
                len
                    .saturating_mul(2)
                    .saturating_add(usize::from(self.pending_mono_right.is_some())),
            )
        } else {
            Some(len)
        }
    }
    fn channels(&self) -> ChannelCount {
        if self.input.channels().get() == 1 {
            ChannelCount::new(2).expect("two channels are non-zero")
        } else {
            self.input.channels()
        }
    }
    fn sample_rate(&self) -> SampleRate {
        self.input.sample_rate()
    }
    fn total_duration(&self) -> Option<Duration> {
        self.input.total_duration()
    }
    fn try_seek(&mut self, position: Duration) -> Result<(), SeekError> {
        self.pending_mono_right = None;
        self.channel_index = 0;
        self.input.try_seek(position)
    }
}

