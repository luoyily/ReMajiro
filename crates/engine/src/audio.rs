use crate::audio_out::{audio_out, AudioOutBackend, AudioStream};
use crate::vfs::Vfs;
use crate::video::MovieAudioSource;
use crate::Instant;
use encoding_rs::SHIFT_JIS;
use std::sync::{
    atomic::{AtomicI32, Ordering},
    Arc,
};
use std::time::Duration;
const CHANNEL_COUNT: usize = 20;
const UNITY_VOLUME: i32 = 1000;
const SAVE_MUSIC_PRIMARY: usize = 0x000;
const SAVE_MUSIC_SECONDARY: usize = 0x100;
const SAVE_ANONYMOUS_PRIMARY: [usize; 4] = [0x200, 0x300, 0x400, 0x500];
const SAVE_ANONYMOUS_SECONDARY: [usize; 4] = [0x280, 0x380, 0x480, 0x580];
const SAVE_MUSIC_VOLUME: usize = 0x600;
const SAVE_MUSIC_LOOP: usize = 0x608;
const SAVE_AUX_VOLUME: usize = 0x620;
const SAVE_ANONYMOUS_VOLUME: [usize; 4] = [0x688, 0x68C, 0x690, 0x694];
const SAVE_MUSIC_NAME_SIZE: usize = 0x100;
const SAVE_ANONYMOUS_NAME_SIZE: usize = 0x80;
struct ResolvedAudio {
    name: Vec<u8>,
    data: Vec<u8>,
}
struct Fade {
    started_ms: i32,
    duration_ms: i32,
    start_volume: i32,
    target_volume: i32,
}
struct AudioNode {
    player: Option<Box<dyn AudioStream>>,
    assigned: bool,
    requested: Vec<u8>,
    requested_secondary: Vec<u8>,
    resolved: Vec<u8>,
    resolved_secondary: Vec<u8>,
    looped: bool,
    current_volume: i32,
    pan: Arc<AtomicI32>,
    fade: Option<Fade>,
    base_sample_rate: u32,
    end_state_cleared: bool,
}
impl Default for AudioNode {
    fn default() -> Self {
        Self {
            player: None,
            assigned: false,
            requested: Vec::new(),
            requested_secondary: Vec::new(),
            resolved: Vec::new(),
            resolved_secondary: Vec::new(),
            looped: false,
            current_volume: UNITY_VOLUME,
            pan: Arc::new(AtomicI32::new(0)),
            fade: None,
            base_sample_rate: 0,
            end_state_cleared: false,
        }
    }
}
impl AudioNode {
    fn status(&self) -> i32 {
        if !self.assigned {
            0
        } else if self.player.as_ref().is_none_or(|stream| stream.is_drained()) {
            if self.end_state_cleared { 1 } else { -1 }
        } else {
            1
        }
    }
    fn stop(&mut self) {
        if let Some(stream) = self.player.take() {
            stream.stop();
        }
        self.assigned = false;
        self.fade = None;
        self.end_state_cleared = false;
    }
    fn set_volume(&mut self, volume: i32) {
        self.current_volume = volume;
        self.fade = None;
        if let Some(stream) = &self.player {
            stream.set_volume(output_gain(volume));
        }
    }
    fn set_pan(&mut self, pan: i32) {
        self.pan.store(pan, Ordering::Relaxed);
        if let Some(stream) = &self.player {
            stream.set_pan(pan);
        }
    }
    fn fade_to(&mut self, target_volume: i32, duration_ms: i32, now: i32) {
        if !self.assigned {
            return;
        }
        self.fade = Some(Fade {
            started_ms: now,
            duration_ms: duration_ms.max(1),
            start_volume: self.current_volume,
            target_volume,
        });
    }
    fn tick(&mut self, now: i32) {
        if self.assigned && self.end_state_cleared
            && self.player.as_ref().is_none_or(|stream| stream.is_drained())
        {
            self.end_state_cleared = false;
        }
        let Some(fade) = &self.fade else {
            return;
        };
        let elapsed = now.wrapping_sub(fade.started_ms).max(0);
        let position = elapsed.min(fade.duration_ms);
        let value = fade.start_volume as i64
            + (fade.target_volume as i64 - fade.start_volume as i64) * position as i64
                / fade.duration_ms as i64;
        self.current_volume = value as i32;
        if let Some(stream) = &self.player {
            stream.set_volume(output_gain(self.current_volume));
        }
        if elapsed >= fade.duration_ms {
            self.current_volume = fade.target_volume;
            self.fade = None;
        }
    }
    fn retire_finished_one_shot(&mut self) {
        if self.assigned && !self.looped
            && self.player.as_ref().is_none_or(|stream| stream.is_drained())
        {
            self.stop();
        }
    }
}
struct DetachedFade {
    stream: Box<dyn AudioStream>,
    started_ms: i32,
    duration_ms: i32,
    start_volume: i32,
}
impl DetachedFade {
    fn tick(&self, now: i32) -> bool {
        let elapsed = now.wrapping_sub(self.started_ms).max(0);
        if elapsed >= self.duration_ms || self.stream.is_drained() {
            self.stream.stop();
            return true;
        }
        let volume = self.start_volume as i64
            - self.start_volume as i64 * elapsed as i64 / self.duration_ms as i64;
        self.stream.set_volume(output_gain(volume as i32));
        false
    }
}
#[derive(Default, Clone)]
struct MusicSnapshot {
    primary: Vec<u8>,
    secondary: Vec<u8>,
    looped: bool,
}
pub struct AudioBackend {
    audio_out: Arc<dyn AudioOutBackend>,
    output_open: bool,
    output_device_id: Option<String>,
    output_generation: u64,
    last_output_device_check: Instant,
    output_error_reported: bool,
    voice: Vec<AudioNode>,
    sound: Vec<AudioNode>,
    music: AudioNode,
    aux: AudioNode,
    anonymous: [AudioNode; 4],
    detached: Vec<DetachedFade>,
    voice_master: i32,
    sound_master: i32,
    music_master: i32,
    voice_pending_volume: i32,
    sound_pending_volume: i32,
    sound_pending_pan: i32,
    voice_pending_pan: [i32; CHANNEL_COUNT],
    aux_pending_volume: i32,
    aux_current_scale: i32,
    aux_pending_pan: i32,
    music_pending_volume: i32,
    music_current_scale: i32,
    anonymous_pending_volume: [i32; 4],
    anonymous_current_scale: [i32; 4],
    anonymous_pending_pan: [i32; 4],
    music_snapshot: MusicSnapshot,
}
impl Default for AudioBackend {
    fn default() -> Self {
        Self::new()
    }
}
impl AudioBackend {
    pub fn new() -> Self {
        Self {
            audio_out: audio_out(),
            output_open: false,
            output_device_id: None,
            output_generation: 0,
            last_output_device_check: Instant::now()
                .checked_sub(Duration::from_secs(1))
                .unwrap_or_else(Instant::now),
            output_error_reported: false,
            voice: (0..CHANNEL_COUNT).map(|_| AudioNode::default()).collect(),
            sound: (0..CHANNEL_COUNT).map(|_| AudioNode::default()).collect(),
            music: AudioNode::default(),
            aux: AudioNode::default(),
            anonymous: std::array::from_fn(|_| AudioNode::default()),
            detached: Vec::new(),
            voice_master: UNITY_VOLUME,
            sound_master: UNITY_VOLUME,
            music_master: UNITY_VOLUME,
            voice_pending_volume: UNITY_VOLUME,
            sound_pending_volume: UNITY_VOLUME,
            sound_pending_pan: 0,
            voice_pending_pan: [0; CHANNEL_COUNT],
            aux_pending_volume: UNITY_VOLUME,
            aux_current_scale: UNITY_VOLUME,
            aux_pending_pan: 0,
            music_pending_volume: UNITY_VOLUME,
            music_current_scale: UNITY_VOLUME,
            anonymous_pending_volume: [UNITY_VOLUME; 4],
            anonymous_current_scale: [UNITY_VOLUME; 4],
            anonymous_pending_pan: [0; 4],
            music_snapshot: MusicSnapshot::default(),
        }
    }
    pub(super) fn tick(&mut self, now: i32) {
        for node in &mut self.voice {
            node.tick(now);
        }
        for node in &mut self.sound {
            node.tick(now);
        }
        self.music.tick(now);
        self.aux.tick(now);
        for node in &mut self.anonymous {
            node.tick(now);
            node.retire_finished_one_shot();
        }
        self.detached.retain(|fade| !fade.tick(now));
    }
    pub(super) fn refresh_default_output(&mut self, vfs: &Vfs) {
        if !self.output_open
            || self.last_output_device_check.elapsed() < Duration::from_millis(500)
        {
            return;
        }
        self.last_output_device_check = Instant::now();
        let Some(default_id) = self.audio_out.default_device_id() else {
            return;
        };
        if self.output_device_id.as_ref() == Some(&default_id) {
            return;
        }
        if let Err(error) = self.audio_out.open_default_output() {
            if !self.output_error_reported {
                eprintln!("[AUDIO] failed to follow default output device: {error}");
                self.output_error_reported = true;
            }
            return;
        }
        let old_id = self.output_device_id.replace(default_id);
        self.output_generation = self.output_generation.wrapping_add(1).max(1);
        self.output_error_reported = false;
        for node in &mut self.voice {
            reconnect_node(
                &self.audio_out,
                &mut self.output_open,
                &mut self.output_device_id,
                &mut self.output_generation,
                &mut self.output_error_reported,
                vfs,
                node,
            );
        }
        for node in &mut self.sound {
            reconnect_node(
                &self.audio_out,
                &mut self.output_open,
                &mut self.output_device_id,
                &mut self.output_generation,
                &mut self.output_error_reported,
                vfs,
                node,
            );
        }
        reconnect_node(
            &self.audio_out,
            &mut self.output_open,
            &mut self.output_device_id,
            &mut self.output_generation,
            &mut self.output_error_reported,
            vfs,
            &mut self.music,
        );
        reconnect_node(
            &self.audio_out,
            &mut self.output_open,
            &mut self.output_device_id,
            &mut self.output_generation,
            &mut self.output_error_reported,
            vfs,
            &mut self.aux,
        );
        for node in &mut self.anonymous {
            reconnect_node(
                &self.audio_out,
                &mut self.output_open,
                &mut self.output_device_id,
                &mut self.output_generation,
                &mut self.output_error_reported,
                vfs,
                node,
            );
        }
        for fade in self.detached.drain(..) {
            fade.stream.stop();
        }
        eprintln!(
            "[AUDIO] default output changed {:?} -> {}; migrated active nodes", old_id
            .as_ref().map(ToString::to_string), self.output_device_id.as_deref()
            .unwrap_or_default()
        );
    }
    pub(super) fn output_generation(&self) -> u64 {
        self.output_generation
    }
    pub(super) fn capture_save_state(&self) -> Vec<u8> {
        let mut blob = vec![0; formats::save::AUDIO_BLOB_SIZE];
        write_save_cstr(
            &mut blob,
            SAVE_MUSIC_PRIMARY,
            SAVE_MUSIC_NAME_SIZE,
            &self.music.resolved,
        );
        write_save_cstr(
            &mut blob,
            SAVE_MUSIC_SECONDARY,
            SAVE_MUSIC_NAME_SIZE,
            &self.music.resolved_secondary,
        );
        write_save_i32(&mut blob, SAVE_MUSIC_VOLUME, self.music_current_scale);
        write_save_i32(&mut blob, SAVE_MUSIC_LOOP, i32::from(self.music.looped));
        write_save_i32(&mut blob, SAVE_AUX_VOLUME, self.aux_current_scale);
        for (index, node) in self.anonymous.iter().enumerate() {
            if node.looped && node.status() != 0 {
                write_save_cstr(
                    &mut blob,
                    SAVE_ANONYMOUS_PRIMARY[index],
                    SAVE_ANONYMOUS_NAME_SIZE,
                    &node.resolved,
                );
                write_save_cstr(
                    &mut blob,
                    SAVE_ANONYMOUS_SECONDARY[index],
                    SAVE_ANONYMOUS_NAME_SIZE,
                    &node.resolved_secondary,
                );
            }
            write_save_i32(
                &mut blob,
                SAVE_ANONYMOUS_VOLUME[index],
                self.anonymous_current_scale[index],
            );
        }
        blob
    }
    pub(super) fn reset_scene_state(&mut self) {
        let retired_music = self.music.resolved.clone();
        let retired_nodes = self
            .voice
            .iter()
            .chain(&self.sound)
            .chain(std::iter::once(&self.music))
            .chain(std::iter::once(&self.aux))
            .chain(&self.anonymous)
            .filter(|node| node.status() != 0)
            .count();
        let retired_fades = self.detached.len();
        for node in &mut self.voice {
            node.stop();
        }
        for node in &mut self.sound {
            node.stop();
        }
        self.music.stop();
        self.aux.stop();
        for node in &mut self.anonymous {
            node.stop();
        }
        for fade in self.detached.drain(..) {
            fade.stream.stop();
        }
        self.voice = (0..CHANNEL_COUNT).map(|_| AudioNode::default()).collect();
        self.sound = (0..CHANNEL_COUNT).map(|_| AudioNode::default()).collect();
        self.music = AudioNode::default();
        self.aux = AudioNode::default();
        self.anonymous = std::array::from_fn(|_| AudioNode::default());
        self.voice_pending_volume = UNITY_VOLUME;
        self.sound_pending_volume = UNITY_VOLUME;
        self.sound_pending_pan = 0;
        self.voice_pending_pan = [0; CHANNEL_COUNT];
        self.aux_pending_volume = UNITY_VOLUME;
        self.aux_current_scale = UNITY_VOLUME;
        self.aux_pending_pan = 0;
        self.music_pending_volume = UNITY_VOLUME;
        self.music_current_scale = UNITY_VOLUME;
        self.anonymous_pending_volume = [UNITY_VOLUME; 4];
        self.anonymous_current_scale = [UNITY_VOLUME; 4];
        self.anonymous_pending_pan = [0; 4];
        self.music_snapshot = MusicSnapshot::default();
        eprintln!(
            "[AUDIO] reset scene state: music={:?}, nodes={}, detached_fades={}",
            String::from_utf8_lossy(& retired_music), retired_nodes, retired_fades
        );
        vm::text_trace!(
            "[AUDIO] reset scene state: music={:?}, nodes={}, detached_fades={}",
            String::from_utf8_lossy(& retired_music), retired_nodes, retired_fades
        );
    }
    pub(super) fn restore_save_state(&mut self, vfs: &Vfs, blob: &[u8]) {
        if blob.len() != formats::save::AUDIO_BLOB_SIZE {
            eprintln!(
                "[AUDIO] rejected save state: expected {} bytes, got {}",
                formats::save::AUDIO_BLOB_SIZE, blob.len()
            );
            return;
        }
        let music_primary = read_save_cstr(
            blob,
            SAVE_MUSIC_PRIMARY,
            SAVE_MUSIC_NAME_SIZE,
        );
        let music_secondary = read_save_cstr(
            blob,
            SAVE_MUSIC_SECONDARY,
            SAVE_MUSIC_NAME_SIZE,
        );
        if music_primary.is_empty() {
            self.music_stop();
        } else {
            self.music_pending_volume = read_save_i32(blob, SAVE_MUSIC_VOLUME)
                .unwrap_or(UNITY_VOLUME);
            self.music_play(
                vfs,
                music_primary,
                music_secondary,
                read_save_i32(blob, SAVE_MUSIC_LOOP).unwrap_or_default() != 0,
            );
        }
        let mut anonymous_count = 0;
        for index in 0..self.anonymous.len() {
            let primary = read_save_cstr(
                blob,
                SAVE_ANONYMOUS_PRIMARY[index],
                SAVE_ANONYMOUS_NAME_SIZE,
            );
            if primary.is_empty() {
                continue;
            }
            let secondary = read_save_cstr(
                blob,
                SAVE_ANONYMOUS_SECONDARY[index],
                SAVE_ANONYMOUS_NAME_SIZE,
            );
            self.anonymous_pending_volume[index] = read_save_i32(
                    blob,
                    SAVE_ANONYMOUS_VOLUME[index],
                )
                .unwrap_or(UNITY_VOLUME);
            self.anonymous_play(vfs, index, primary, secondary, true);
            anonymous_count += 1;
        }
        eprintln!(
            "[AUDIO] restored save state: music={:?}, anonymous={}",
            String::from_utf8_lossy(music_primary), anonymous_count
        );
    }
    pub(super) fn start_movie(
        &mut self,
        source: MovieAudioSource,
        volume: i32,
    ) -> Option<(Box<dyn AudioStream>, u64)> {
        if !self.output_open {
            match self.audio_out.open_default_output() {
                Ok(()) => {
                    self.output_open = true;
                    self.output_device_id = self.audio_out.default_device_id();
                    self.output_generation = self
                        .output_generation
                        .wrapping_add(1)
                        .max(1);
                    self.output_error_reported = false;
                }
                Err(error) => {
                    if !self.output_error_reported {
                        eprintln!(
                            "[VIDEO] failed to open default audio output: {error}"
                        );
                        self.output_error_reported = true;
                    }
                    return None;
                }
            }
        }
        let stream = match self.audio_out.play_pcm(Box::new(source), output_gain(volume))
        {
            Ok(stream) => stream,
            Err(error) => {
                if !self.output_error_reported {
                    eprintln!("[VIDEO] failed to attach movie audio: {error}");
                    self.output_error_reported = true;
                }
                return None;
            }
        };
        Some((stream, self.output_generation))
    }
    pub(super) fn voice_check_file(&self, vfs: &Vfs, name: &[u8]) -> bool {
        resolve_audio(vfs, name).is_some()
    }
    pub(super) fn voice_play(
        &mut self,
        vfs: &Vfs,
        channel: u8,
        name: &[u8],
        looped: bool,
        skip_active: bool,
    ) {
        let Some(node) = self.voice.get_mut(channel as usize) else {
            return;
        };
        node.stop();
        if skip_active && !looped {
            node.requested = name.to_vec();
            return;
        }
        let volume = scaled_volume(self.voice_pending_volume, self.voice_master);
        let pan = self.voice_pending_pan[channel as usize];
        start_node(
            &self.audio_out,
            &mut self.output_open,
            &mut self.output_device_id,
            &mut self.output_generation,
            &mut self.output_error_reported,
            vfs,
            node,
            NodeStart {
                primary_name: name,
                secondary_name: &[],
                looped,
                volume,
                pan,
            },
        );
        self.voice_pending_volume = UNITY_VOLUME;
        self.voice_pending_pan[channel as usize] = 0;
    }
    pub(super) fn voice_stop(&mut self, channel: u8) {
        if let Some(node) = self.voice.get_mut(channel as usize) {
            node.stop();
        }
        self.voice_pending_volume = UNITY_VOLUME;
    }
    pub(super) fn voice_set_volume(&mut self, channel: u8, volume: i32) {
        self.voice_pending_volume = volume;
        if let Some(node) = self.voice.get_mut(channel as usize) {
            if node.assigned {
                node.set_volume(scaled_volume(volume, self.voice_master));
            }
        }
    }
    pub(super) fn voice_set_pan(&mut self, channel: u8, pan: i32) {
        let Some(pending) = self.voice_pending_pan.get_mut(channel as usize) else {
            return;
        };
        *pending = pan;
        if let Some(node) = self.voice.get_mut(channel as usize) {
            if node.assigned {
                node.set_pan(pan);
            }
        }
    }
    pub(super) fn voice_fade(
        &mut self,
        channel: u8,
        target: i32,
        duration_ms: i32,
        now: i32,
    ) {
        self.voice_pending_volume = target;
        if let Some(node) = self.voice.get_mut(channel as usize) {
            node.fade_to(scaled_volume(target, self.voice_master), duration_ms, now);
        }
    }
    pub(super) fn voice_fadeout(&mut self, channel: u8, duration_ms: i32, now: i32) {
        if let Some(node) = self.voice.get_mut(channel as usize) {
            detach_fadeout(&mut self.detached, node, duration_ms, now);
        }
        self.voice_pending_volume = UNITY_VOLUME;
    }
    pub(super) fn voice_status(&self, channel: u8) -> i32 {
        self.voice.get(channel as usize).map_or(0, AudioNode::status)
    }
    pub(super) fn voice_filename(&self, channel: u8) -> Vec<u8> {
        self.voice
            .get(channel as usize)
            .map(|node| node.requested.clone())
            .unwrap_or_default()
    }
    pub(super) fn sound_play(
        &mut self,
        vfs: &Vfs,
        channel: u8,
        name: &[u8],
        looped: bool,
    ) {
        let Some(node) = self.sound.get_mut(channel as usize) else {
            return;
        };
        let volume = scaled_volume(self.sound_pending_volume, self.sound_master);
        start_node(
            &self.audio_out,
            &mut self.output_open,
            &mut self.output_device_id,
            &mut self.output_generation,
            &mut self.output_error_reported,
            vfs,
            node,
            NodeStart {
                primary_name: name,
                secondary_name: &[],
                looped,
                volume,
                pan: self.sound_pending_pan,
            },
        );
        self.sound_pending_volume = UNITY_VOLUME;
        self.sound_pending_pan = 0;
    }
    pub(super) fn sound_stop(&mut self, channel: u8) {
        if let Some(node) = self.sound.get_mut(channel as usize) {
            node.stop();
        }
        self.sound_pending_volume = UNITY_VOLUME;
    }
    pub(super) fn sound_set_volume(&mut self, channel: u8, volume: i32) {
        self.sound_pending_volume = volume;
        if let Some(node) = self.sound.get_mut(channel as usize) {
            if node.assigned {
                node.set_volume(scaled_volume(volume, self.voice_master));
            }
        }
    }
    pub(super) fn sound_set_pan(&mut self, channel: u8, pan: i32) {
        self.sound_pending_pan = pan;
        if let Some(node) = self.sound.get_mut(channel as usize) {
            if node.assigned {
                node.set_pan(pan);
            }
        }
    }
    pub(super) fn sound_fade(
        &mut self,
        channel: u8,
        target: i32,
        duration_ms: i32,
        now: i32,
    ) {
        self.sound_pending_volume = target;
        if let Some(node) = self.sound.get_mut(channel as usize) {
            node.fade_to(scaled_volume(target, self.sound_master), duration_ms, now);
        }
    }
    pub(super) fn sound_fadeout(&mut self, channel: u8, duration_ms: i32, now: i32) {
        if let Some(node) = self.sound.get_mut(channel as usize) {
            detach_fadeout(&mut self.detached, node, duration_ms, now);
        }
        self.sound_pending_volume = UNITY_VOLUME;
    }
    pub(super) fn sound_status(&self, channel: u8) -> i32 {
        self.sound.get(channel as usize).map_or(0, AudioNode::status)
    }
    pub(super) fn sound_filename(&self, channel: u8) -> Vec<u8> {
        self.sound
            .get(channel as usize)
            .map(|node| node.requested.clone())
            .unwrap_or_default()
    }
    pub(super) fn music_play(
        &mut self,
        vfs: &Vfs,
        primary: &[u8],
        secondary: &[u8],
        looped: bool,
    ) {
        if primary.is_empty() {
            self.music_stop();
            return;
        }
        if self.music.status() != 0 {
            let resolved_primary = resolve_audio_name(vfs, primary);
            let resolved_secondary = if secondary.is_empty() {
                Some(Vec::new())
            } else {
                resolve_audio_name(vfs, secondary)
            };
            if resolved_primary
                .as_deref()
                .is_some_and(|name| name.eq_ignore_ascii_case(&self.music.resolved))
                && resolved_secondary
                    .as_deref()
                    .is_some_and(|name| {
                        name.eq_ignore_ascii_case(&self.music.resolved_secondary)
                    })
            {
                vm::text_trace!(
                    "[AUDIO] music_play dedup-skip (already playing): {:?}",
                    String::from_utf8_lossy(primary)
                );
                return;
            }
        }
        let scale = self.music_pending_volume;
        vm::text_trace!(
            "[AUDIO] music_play start: primary={:?} secondary={:?} looped={} prev_status={} prev_resolved={:?}",
            String::from_utf8_lossy(primary), String::from_utf8_lossy(secondary), looped,
            self.music.status(), String::from_utf8_lossy(& self.music.resolved)
        );
        start_node(
            &self.audio_out,
            &mut self.output_open,
            &mut self.output_device_id,
            &mut self.output_generation,
            &mut self.output_error_reported,
            vfs,
            &mut self.music,
            NodeStart {
                primary_name: primary,
                secondary_name: secondary,
                looped,
                volume: scaled_volume(scale, self.music_master),
                pan: 0,
            },
        );
        self.music_current_scale = scale;
        self.music_pending_volume = UNITY_VOLUME;
    }
    pub(super) fn music_replace(
        &mut self,
        vfs: &Vfs,
        primary: &[u8],
        secondary: &[u8],
        looped: bool,
    ) {
        if !self.music.assigned {
            self.music.requested = primary.to_vec();
            self.music.requested_secondary = secondary.to_vec();
            self.music.looped = looped;
            self.music.resolved = resolve_audio(vfs, primary)
                .map(|audio| audio.name)
                .unwrap_or_default();
            self.music.resolved_secondary = if secondary.is_empty() {
                Vec::new()
            } else {
                resolve_audio(vfs, secondary).map(|audio| audio.name).unwrap_or_default()
            };
            return;
        }
        start_node(
            &self.audio_out,
            &mut self.output_open,
            &mut self.output_device_id,
            &mut self.output_generation,
            &mut self.output_error_reported,
            vfs,
            &mut self.music,
            NodeStart {
                primary_name: primary,
                secondary_name: secondary,
                looped,
                volume: scaled_volume(self.music_current_scale, self.music_master),
                pan: 0,
            },
        );
    }
    pub(super) fn music_stop(&mut self) {
        vm::text_trace!(
            "[AUDIO] music_stop: {:?}", String::from_utf8_lossy(& self.music.resolved)
        );
        self.music.stop();
        self.music.requested.clear();
        self.music.requested_secondary.clear();
        self.music.resolved.clear();
        self.music.resolved_secondary.clear();
        self.music_pending_volume = UNITY_VOLUME;
    }
    pub(super) fn music_fadeout(&mut self, duration_ms: i32, now: i32) {
        vm::text_trace!(
            "[AUDIO] music_fadeout: {:?} duration_ms={}", String::from_utf8_lossy(& self
            .music.resolved), duration_ms
        );
        detach_fadeout(&mut self.detached, &mut self.music, duration_ms, now);
        self.music.requested.clear();
        self.music.requested_secondary.clear();
        self.music.resolved.clear();
        self.music.resolved_secondary.clear();
        self.music_current_scale = 0;
        self.music_pending_volume = UNITY_VOLUME;
    }
    pub(super) fn music_fade(&mut self, target: i32, duration_ms: i32, now: i32) {
        self.music_current_scale = target;
        self.music.fade_to(scaled_volume(target, self.music_master), duration_ms, now);
    }
    pub(super) fn music_set_volume(&mut self, volume: i32) {
        if self.music.status() != 0 {
            self.music.set_volume(scaled_volume(volume, self.music_master));
            self.music_current_scale = volume;
        } else {
            self.music_pending_volume = volume;
        }
    }
    pub(super) fn music_pause(&self) {
        if let Some(stream) = &self.music.player {
            stream.set_paused(true);
        }
    }
    pub(super) fn music_resume(&self) {
        if let Some(stream) = &self.music.player {
            stream.set_paused(false);
        }
    }
    pub(super) fn music_status(&self) -> i32 {
        self.music.status()
    }
    pub(super) fn music_set_frequency(&self, frequency: i32) {
        if frequency <= 0 || self.music.base_sample_rate == 0 {
            return;
        }
        if let Some(stream) = &self.music.player {
            stream.set_speed(frequency as f32 / self.music.base_sample_rate as f32);
        }
    }
    pub(super) fn music_clear_end_state(&mut self) {
        self.music.end_state_cleared = self.music.assigned
            && self.music.player.as_ref().is_none_or(|stream| stream.is_drained());
    }
    pub(super) fn music_snapshot(&mut self) {
        self.music_snapshot = MusicSnapshot {
            primary: self.music.resolved.clone(),
            secondary: self.music.resolved_secondary.clone(),
            looped: self.music.looped,
        };
    }
    pub(super) fn music_restore(&mut self, vfs: &Vfs) {
        let snapshot = self.music_snapshot.clone();
        if snapshot.primary.is_empty() {
            self.music_stop();
            self.music_current_scale = 0;
            return;
        }
        if self.music.resolved == snapshot.primary
            && self.music.resolved_secondary == snapshot.secondary
            && self.music.looped == snapshot.looped
        {
            return;
        }
        self.music_play(vfs, &snapshot.primary, &snapshot.secondary, snapshot.looped);
    }
    pub(super) fn aux_play(
        &mut self,
        vfs: &Vfs,
        name: &[u8],
        looped: bool,
        skip_active: bool,
    ) {
        self.aux.stop();
        self.aux.requested = name.to_vec();
        if skip_active && !looped {
            return;
        }
        let scale = self.aux_pending_volume;
        start_node(
            &self.audio_out,
            &mut self.output_open,
            &mut self.output_device_id,
            &mut self.output_generation,
            &mut self.output_error_reported,
            vfs,
            &mut self.aux,
            NodeStart {
                primary_name: name,
                secondary_name: &[],
                looped,
                volume: scaled_volume(scale, self.voice_master),
                pan: self.aux_pending_pan,
            },
        );
        self.aux_current_scale = scale;
        self.aux_pending_volume = UNITY_VOLUME;
        self.aux_pending_pan = 0;
    }
    pub(super) fn aux_stop(&mut self) {
        self.aux.stop();
        self.aux_pending_volume = UNITY_VOLUME;
    }
    pub(super) fn aux_set_volume(&mut self, volume: i32) {
        self.aux_pending_volume = volume;
        if self.aux.assigned {
            self.aux.set_volume(scaled_volume(volume, self.voice_master));
        }
    }
    pub(super) fn aux_set_pan(&mut self, pan: i32) {
        self.aux_pending_pan = pan;
        if self.aux.assigned {
            self.aux.set_pan(pan);
        }
    }
    pub(super) fn aux_fade(&mut self, target: i32, duration_ms: i32, now: i32) {
        self.aux.fade_to(scaled_volume(target, self.voice_master), duration_ms, now);
    }
    pub(super) fn aux_fadeout(&mut self, duration_ms: i32, now: i32) {
        detach_fadeout(&mut self.detached, &mut self.aux, duration_ms, now);
        self.aux_pending_volume = UNITY_VOLUME;
    }
    pub(super) fn aux_status(&self) -> i32 {
        self.aux.status()
    }
    pub(super) fn aux_filename(&self) -> Vec<u8> {
        self.aux.requested.clone()
    }
    pub(super) fn anonymous_play(
        &mut self,
        vfs: &Vfs,
        index: usize,
        primary: &[u8],
        secondary: &[u8],
        looped: bool,
    ) {
        let Some(node) = self.anonymous.get_mut(index) else {
            return;
        };
        let scale = self.anonymous_pending_volume[index];
        start_node(
            &self.audio_out,
            &mut self.output_open,
            &mut self.output_device_id,
            &mut self.output_generation,
            &mut self.output_error_reported,
            vfs,
            node,
            NodeStart {
                primary_name: primary,
                secondary_name: secondary,
                looped,
                volume: scaled_volume(scale, self.sound_master),
                pan: self.anonymous_pending_pan[index],
            },
        );
        self.anonymous_current_scale[index] = scale;
        self.anonymous_pending_volume[index] = UNITY_VOLUME;
        self.anonymous_pending_pan[index] = 0;
    }
    pub(super) fn anonymous_stop(&mut self, index: usize) {
        if let Some(node) = self.anonymous.get_mut(index) {
            node.stop();
        }
        if let Some(volume) = self.anonymous_pending_volume.get_mut(index) {
            *volume = UNITY_VOLUME;
        }
    }
    pub(super) fn anonymous_set_volume(&mut self, index: usize, volume: i32) {
        let Some(node) = self.anonymous.get_mut(index) else {
            return;
        };
        if index == 0 {
            self.anonymous_pending_volume[index] = volume;
        } else if node.status() != 0 {
            self.anonymous_current_scale[index] = volume;
        } else {
            self.anonymous_pending_volume[index] = volume;
        }
        if node.assigned {
            node.set_volume(scaled_volume(volume, self.sound_master));
        }
    }
    pub(super) fn anonymous_set_pan(&mut self, index: usize, pan: i32) {
        let Some(pending) = self.anonymous_pending_pan.get_mut(index) else {
            return;
        };
        *pending = pan;
        if let Some(node) = self.anonymous.get_mut(index) {
            if node.assigned {
                node.set_pan(pan);
            }
        }
    }
    pub(super) fn anonymous_fade(
        &mut self,
        index: usize,
        target: i32,
        duration_ms: i32,
        now: i32,
    ) {
        let Some(node) = self.anonymous.get_mut(index) else {
            return;
        };
        if index == 0 {
            self.anonymous_pending_volume[index] = target;
        } else {
            self.anonymous_current_scale[index] = target;
        }
        node.fade_to(scaled_volume(target, self.sound_master), duration_ms, now);
    }
    pub(super) fn anonymous_fadeout(
        &mut self,
        index: usize,
        duration_ms: i32,
        now: i32,
    ) {
        if let Some(node) = self.anonymous.get_mut(index) {
            detach_fadeout(&mut self.detached, node, duration_ms, now);
        }
        if let Some(volume) = self.anonymous_pending_volume.get_mut(index) {
            *volume = UNITY_VOLUME;
        }
    }
    pub(super) fn anonymous_status(&self, index: usize) -> i32 {
        self.anonymous.get(index).map_or(0, AudioNode::status)
    }
    pub(super) fn anonymous_filename(&self, index: usize) -> Vec<u8> {
        self.anonymous
            .get(index)
            .filter(|node| node.status() > 0)
            .map(|node| node.resolved.clone())
            .unwrap_or_default()
    }
    pub(super) fn anonymous_flag(&self, index: usize) -> i32 {
        self
            .anonymous
            .get(index)
            .filter(|node| node.assigned)
            .is_some_and(|node| node.looped) as i32
    }
    pub(super) fn set_sound_master(&mut self, volume: i32) {
        self.sound_master = volume;
        for (node, scale) in self.anonymous.iter_mut().zip(self.anonymous_current_scale)
        {
            if node.assigned {
                node.set_volume(scaled_volume(scale, volume));
            }
        }
    }
    pub(super) fn set_voice_master(&mut self, volume: i32) {
        self.voice_master = volume;
        if self.aux.assigned {
            self.aux.set_volume(scaled_volume(self.aux_current_scale, volume));
        }
    }
    pub(super) fn set_music_master(&mut self, volume: i32) {
        self.music_master = volume;
        if self.music.assigned {
            self.music.set_volume(scaled_volume(self.music_current_scale, volume));
        }
    }
}
fn write_save_cstr(blob: &mut [u8], offset: usize, size: usize, value: &[u8]) {
    let Some(field) = blob.get_mut(offset..offset.saturating_add(size)) else {
        return;
    };
    field.fill(0);
    let value = value.split(|byte| *byte == 0).next().unwrap_or(value);
    let copy_len = value.len().min(size.saturating_sub(1));
    field[..copy_len].copy_from_slice(&value[..copy_len]);
}
fn read_save_cstr(blob: &[u8], offset: usize, size: usize) -> &[u8] {
    let Some(field) = blob.get(offset..offset.saturating_add(size)) else {
        return &[];
    };
    field.split(|byte| *byte == 0).next().unwrap_or(field)
}
fn write_save_i32(blob: &mut [u8], offset: usize, value: i32) {
    let Some(field) = blob.get_mut(offset..offset.saturating_add(4)) else {
        return;
    };
    field.copy_from_slice(&value.to_le_bytes());
}
fn read_save_i32(blob: &[u8], offset: usize) -> Option<i32> {
    Some(i32::from_le_bytes(blob.get(offset..offset + 4)?.try_into().ok()?))
}
struct NodeStart<'a> {
    primary_name: &'a [u8],
    secondary_name: &'a [u8],
    looped: bool,
    volume: i32,
    pan: i32,
}
fn ensure_output_open(
    audio_out: &Arc<dyn AudioOutBackend>,
    output_open: &mut bool,
    output_device_id: &mut Option<String>,
    output_generation: &mut u64,
    output_error_reported: &mut bool,
) -> Result<(), String> {
    audio_out
        .open_default_output()
        .map(|()| {
            *output_open = true;
            *output_device_id = audio_out.default_device_id();
            *output_generation = output_generation.wrapping_add(1).max(1);
            *output_error_reported = false;
        })
}
fn reconnect_node(
    audio_out: &Arc<dyn AudioOutBackend>,
    output_open: &mut bool,
    output_device_id: &mut Option<String>,
    output_generation: &mut u64,
    output_error_reported: &mut bool,
    vfs: &Vfs,
    node: &mut AudioNode,
) {
    let Some(stream) = node.player.as_ref() else {
        return;
    };
    if !node.assigned || stream.is_drained() {
        return;
    }
    let position = stream.position();
    let paused = stream.is_paused();
    let primary = node.requested.clone();
    let secondary = node.requested_secondary.clone();
    let looped = node.looped;
    let volume = node.current_volume;
    let pan = node.pan.load(Ordering::Relaxed);
    let fade = node.fade.take();
    let end_state_cleared = node.end_state_cleared;
    if !start_node(
        audio_out,
        output_open,
        output_device_id,
        output_generation,
        output_error_reported,
        vfs,
        node,
        NodeStart {
            primary_name: &primary,
            secondary_name: &secondary,
            looped,
            volume,
            pan,
        },
    ) {
        return;
    }
    if position != Duration::ZERO {
        if let Some(stream) = node.player.as_ref() {
            if let Err(error) = stream.seek(position) {
                eprintln!(
                    "[AUDIO] could not preserve position for {:?} after device switch: {error}",
                    String::from_utf8_lossy(& primary)
                );
            }
        }
    }
    if paused {
        if let Some(stream) = node.player.as_ref() {
            stream.set_paused(true);
        }
    }
    node.fade = fade;
    node.end_state_cleared = end_state_cleared;
}
#[allow(clippy::too_many_arguments)]
fn start_node(
    audio_out: &Arc<dyn AudioOutBackend>,
    output_open: &mut bool,
    output_device_id: &mut Option<String>,
    output_generation: &mut u64,
    output_error_reported: &mut bool,
    vfs: &Vfs,
    node: &mut AudioNode,
    start: NodeStart<'_>,
) -> bool {
    let NodeStart { primary_name, secondary_name, looped, volume, pan } = start;
    node.stop();
    node.requested = primary_name.to_vec();
    node.requested_secondary = secondary_name.to_vec();
    node.looped = looped;
    let Some(primary) = resolve_audio(vfs, primary_name) else {
        eprintln!("[AUDIO] file not found: {:?}", String::from_utf8_lossy(primary_name));
        node.resolved.clear();
        node.resolved_secondary.clear();
        return false;
    };
    let secondary = if secondary_name.is_empty() {
        None
    } else {
        let Some(secondary) = resolve_audio(vfs, secondary_name) else {
            eprintln!(
                "[AUDIO] secondary file not found: {:?}",
                String::from_utf8_lossy(secondary_name)
            );
            node.resolved.clear();
            node.resolved_secondary.clear();
            return false;
        };
        Some(secondary)
    };
    if !*output_open {
        if let Err(error) = ensure_output_open(
            audio_out,
            output_open,
            output_device_id,
            output_generation,
            output_error_reported,
        ) {
            if !*output_error_reported {
                eprintln!("[AUDIO] failed to open default output device: {error}");
                *output_error_reported = true;
            }
            return false;
        }
    }
    let attached = audio_out
        .play_clip(crate::audio_out::ClipPlay {
            data: primary.data,
            loop_tail: secondary.as_ref().map(|secondary| secondary.data.clone()),
            looped,
            gain: output_gain(volume),
            pan,
        });
    let attached = match attached {
        Ok(attached) => attached,
        Err(error) => {
            eprintln!(
                "[AUDIO] failed to play {:?}: {error}",
                String::from_utf8_lossy(primary_name)
            );
            node.resolved.clear();
            node.resolved_secondary.clear();
            return false;
        }
    };
    node.resolved = primary.name;
    node.resolved_secondary = if looped {
        secondary.map(|secondary| secondary.name).unwrap_or_default()
    } else {
        Vec::new()
    };
    node.base_sample_rate = attached.sample_rate;
    node.current_volume = volume;
    node.fade = None;
    node.assigned = true;
    node.end_state_cleared = false;
    node.player = Some(attached.stream);
    true
}
fn detach_fadeout(
    detached: &mut Vec<DetachedFade>,
    node: &mut AudioNode,
    duration_ms: i32,
    now: i32,
) {
    let stream = node.player.take();
    node.assigned = false;
    node.fade = None;
    if let Some(stream) = stream {
        if duration_ms <= 0 {
            stream.stop();
        } else {
            detached
                .push(DetachedFade {
                    stream,
                    started_ms: now,
                    duration_ms,
                    start_volume: node.current_volume,
                });
        }
    }
}
fn scaled_volume(value: i32, master: i32) -> i32 {
    let scaled = value as i64 * master as i64 / UNITY_VOLUME as i64;
    scaled.clamp(i32::MIN as i64, i32::MAX as i64) as i32
}
fn output_gain(volume: i32) -> f32 {
    volume.clamp(0, UNITY_VOLUME) as f32 / UNITY_VOLUME as f32
}
fn resolve_audio_name(vfs: &Vfs, raw_name: &[u8]) -> Option<Vec<u8>> {
    let raw_name = raw_name.split(|byte| *byte == 0).next().unwrap_or(raw_name);
    let bare = raw_name
        .rsplit(|byte| *byte == b'/' || *byte == b'\\')
        .next()
        .unwrap_or(raw_name);
    let stem = bare
        .iter()
        .rposition(|byte| *byte == b'.')
        .map_or(bare, |position| &bare[..position]);
    if stem.is_empty() {
        return None;
    }
    for extension in [b".wav".as_slice(), b".ogg".as_slice()] {
        let mut candidate = stem.to_vec();
        candidate.extend_from_slice(extension);
        let (decoded, _, _) = SHIFT_JIS.decode(&candidate);
        if vfs.exists(decoded.as_ref()) {
            return Some(candidate);
        }
    }
    None
}
fn resolve_audio(vfs: &Vfs, raw_name: &[u8]) -> Option<ResolvedAudio> {
    let name = resolve_audio_name(vfs, raw_name)?;
    let (decoded, _, _) = SHIFT_JIS.decode(&name);
    let data = vfs.find(decoded.as_ref())?;
    Some(ResolvedAudio { name, data })
}

