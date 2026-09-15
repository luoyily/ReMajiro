#[cfg(windows)]
pub fn install() {
    engine::video::install_movie_decode_backend(
        std::sync::Arc::new(MediaFoundationDecoder),
    );
}
#[cfg(not(windows))]
pub fn install() {}
#[cfg(windows)]
mod imp {
    use engine::render_model::RenderMoviePixels;
    use engine::video::{
        AudioFormat, DecodedFrame, DecoderEvent, MovieAudioBuffer, VIDEO_REORDER_DEPTH,
    };
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc;
    use std::sync::Arc;
    use std::time::Duration;
    use windows::core::{Interface, PCWSTR};
    use windows::Win32::Media::MediaFoundation::*;
    use windows::Win32::System::Com::{
        CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED,
    };
    pub struct MediaFoundationDecoder;
    impl engine::video::MovieDecodeBackend for MediaFoundationDecoder {
        fn decode(
            &self,
            args: engine::video::MovieDecodeArgs<'_>,
        ) -> Result<(), String> {
            decode(
                args.path,
                args.cancel,
                args.audio_buffer,
                args.event_tx,
                args.frame_tx,
            )
        }
    }
    struct MediaFoundationGuard {
        com_initialized: bool,
        mf_started: bool,
    }
    impl MediaFoundationGuard {
        unsafe fn start() -> Result<Self, String> {
            let mut guard = Self {
                com_initialized: false,
                mf_started: false,
            };
            CoInitializeEx(None, COINIT_MULTITHREADED)
                .ok()
                .map_err(|error| format!("CoInitializeEx: {error}"))?;
            guard.com_initialized = true;
            MFStartup(MF_VERSION, MFSTARTUP_FULL)
                .map_err(|error| format!("MFStartup: {error}"))?;
            guard.mf_started = true;
            Ok(guard)
        }
    }
    impl Drop for MediaFoundationGuard {
        fn drop(&mut self) {
            unsafe {
                if self.mf_started {
                    let _ = MFShutdown();
                }
                if self.com_initialized {
                    CoUninitialize();
                }
            }
        }
    }
    struct VideoInfo {
        stream: u32,
        width: u32,
        height: u32,
        stride: i32,
        format: VideoPixelFormat,
    }
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum VideoPixelFormat {
        Nv12,
        Bgra8,
    }
    struct AudioInfo {
        stream: u32,
        format: AudioFormat,
        bits_per_sample: u32,
    }
    pub(super) fn decode(
        path: &Path,
        cancel: &AtomicBool,
        audio_buffer: &MovieAudioBuffer,
        event_tx: &mpsc::Sender<DecoderEvent>,
        frame_tx: &mpsc::SyncSender<DecodedFrame>,
    ) -> Result<(), String> {
        unsafe {
            let _guard = MediaFoundationGuard::start()?;
            let mut attributes = None;
            MFCreateAttributes(&mut attributes, 3)
                .map_err(|error| format!("MFCreateAttributes: {error}"))?;
            let attributes = attributes.ok_or("MFCreateAttributes returned null")?;
            attributes
                .SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1)
                .map_err(|error| format!("enable RGB32 video processing: {error}"))?;
            attributes
                .SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1)
                .map_err(|error| format!("enable hardware video transforms: {error}"))?;
            let wide_path: Vec<u16> = path
                .as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            let reader = MFCreateSourceReaderFromURL(
                    PCWSTR(wide_path.as_ptr()),
                    Some(&attributes),
                )
                .map_err(|error| format!("open {}: {error}", path.display()))?;
            reader
                .SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false)
                .map_err(|error| format!("deselect streams: {error}"))?;
            let (video_stream, audio_stream) = discover_streams(&reader)?;
            let mut video = configure_video(&reader, video_stream)?;
            let mut audio = audio_stream
                .map(|stream| configure_audio(&reader, stream))
                .transpose()?;
            event_tx
                .send(DecoderEvent::Ready {
                    width: video.width,
                    height: video.height,
                    audio: audio.as_ref().map(|info| info.format),
                })
                .map_err(|_| {
                    "movie session was closed before decoder ready".to_owned()
                })?;
            let mut end_pts = Duration::ZERO;
            let mut reordered_frames = Vec::with_capacity(VIDEO_REORDER_DEPTH + 1);
            loop {
                if cancel.load(Ordering::Acquire) {
                    return Ok(());
                }
                let mut actual_stream = 0;
                let mut flags = 0;
                let mut timestamp_hns = 0i64;
                let mut sample = None;
                reader
                    .ReadSample(
                        MF_SOURCE_READER_ANY_STREAM.0 as u32,
                        0,
                        Some(&mut actual_stream),
                        Some(&mut flags),
                        Some(&mut timestamp_hns),
                        Some(&mut sample),
                    )
                    .map_err(|error| format!("ReadSample: {error}"))?;
                if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                    break;
                }
                if flags & MF_SOURCE_READERF_ERROR.0 as u32 != 0 {
                    return Err(
                        format!(
                            "source reader reported a fatal error on stream {actual_stream}"
                        ),
                    );
                }
                if flags & MF_SOURCE_READERF_NATIVEMEDIATYPECHANGED.0 as u32 != 0 {
                    if actual_stream == video.stream {
                        video = configure_video(&reader, video.stream)?;
                    } else if audio
                        .as_ref()
                        .is_some_and(|info| actual_stream == info.stream)
                    {
                        let new_audio = configure_audio(&reader, actual_stream)?;
                        ensure_stable_audio_format(audio.as_ref(), &new_audio)?;
                        audio = Some(new_audio);
                    }
                } else if flags & MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED.0 as u32 != 0
                {
                    if actual_stream == video.stream {
                        video = query_video_info(&reader, video.stream)?;
                    } else if audio
                        .as_ref()
                        .is_some_and(|info| actual_stream == info.stream)
                    {
                        let new_audio = query_audio_info(&reader, actual_stream)?;
                        ensure_stable_audio_format(audio.as_ref(), &new_audio)?;
                        audio = Some(new_audio);
                    }
                }
                let Some(sample) = sample else {
                    continue;
                };
                let pts = hns_to_duration(timestamp_hns);
                let sample_duration = sample
                    .GetSampleDuration()
                    .ok()
                    .map(hns_to_duration)
                    .unwrap_or_default();
                end_pts = end_pts.max(pts + sample_duration);
                if actual_stream == video.stream {
                    let pixels = copy_video_sample(&sample, &video)?;
                    let frame = DecodedFrame {
                        pts,
                        end_pts: pts + sample_duration,
                        width: video.width,
                        height: video.height,
                        pixels,
                    };
                    let insertion = reordered_frames
                        .partition_point(|queued: &DecodedFrame| {
                            queued.pts <= frame.pts
                        });
                    reordered_frames.insert(insertion, frame);
                    if reordered_frames.len() > VIDEO_REORDER_DEPTH {
                        let frame = reordered_frames.remove(0);
                        if frame_tx.send(frame).is_err() {
                            return Ok(());
                        }
                    }
                } else if let Some(audio) = &audio {
                    if actual_stream == audio.stream {
                        let samples = copy_audio_sample(&sample, audio)?;
                        if !audio_buffer.push(&samples, cancel) {
                            return Ok(());
                        }
                    }
                }
            }
            for frame in reordered_frames {
                if cancel.load(Ordering::Acquire) || frame_tx.send(frame).is_err() {
                    return Ok(());
                }
            }
            audio_buffer.finish();
            let _ = event_tx.send(DecoderEvent::Finished { end_pts });
            Ok(())
        }
    }
    unsafe fn discover_streams(
        reader: &IMFSourceReader,
    ) -> Result<(u32, Option<u32>), String> {
        let mut video = None;
        let mut audio = None;
        for stream in 0..32u32 {
            let Ok(native) = reader.GetNativeMediaType(stream, 0) else {
                if stream > 1 && (video.is_some() || audio.is_some()) {
                    break;
                }
                continue;
            };
            let Ok(major) = native.GetGUID(&MF_MT_MAJOR_TYPE) else {
                continue;
            };
            if major == MFMediaType_Video && video.is_none() {
                video = Some(stream);
            } else if major == MFMediaType_Audio && audio.is_none() {
                audio = Some(stream);
            }
        }
        video
            .map(|stream| (stream, audio))
            .ok_or_else(|| "movie has no decodable video stream".into())
    }
    unsafe fn configure_video(
        reader: &IMFSourceReader,
        stream: u32,
    ) -> Result<VideoInfo, String> {
        let format = if set_video_output_type(reader, stream, &MFVideoFormat_NV12)
            .is_ok()
        {
            VideoPixelFormat::Nv12
        } else {
            set_video_output_type(reader, stream, &MFVideoFormat_RGB32)
                .map_err(|error| format!("configure NV12/RGB32 decoder: {error}"))?;
            VideoPixelFormat::Bgra8
        };
        reader
            .SetStreamSelection(stream, true)
            .map_err(|error| format!("select video stream: {error}"))?;
        let info = query_video_info(reader, stream)?;
        eprintln!(
            "[VIDEO] decoder output={:?} {}x{} stride={}", format, info.width, info
            .height, info.stride
        );
        Ok(info)
    }
    unsafe fn set_video_output_type(
        reader: &IMFSourceReader,
        stream: u32,
        subtype: &windows::core::GUID,
    ) -> windows::core::Result<()> {
        let media_type = MFCreateMediaType()?;
        media_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        media_type.SetGUID(&MF_MT_SUBTYPE, subtype)?;
        reader.SetCurrentMediaType(stream, None, &media_type)
    }
    unsafe fn query_video_info(
        reader: &IMFSourceReader,
        stream: u32,
    ) -> Result<VideoInfo, String> {
        let current = reader
            .GetCurrentMediaType(stream)
            .map_err(|error| format!("query video type: {error}"))?;
        let major = current
            .GetGUID(&MF_MT_MAJOR_TYPE)
            .map_err(|error| format!("query video major type: {error}"))?;
        let subtype = current
            .GetGUID(&MF_MT_SUBTYPE)
            .map_err(|error| format!("query video subtype: {error}"))?;
        let format = if subtype == MFVideoFormat_NV12 {
            VideoPixelFormat::Nv12
        } else if subtype == MFVideoFormat_RGB32 {
            VideoPixelFormat::Bgra8
        } else {
            return Err(
                format!(
                    "unexpected decoded video format: major={major:?} subtype={subtype:?}"
                ),
            );
        };
        if major != MFMediaType_Video {
            return Err(
                format!(
                    "unexpected decoded video format: major={major:?} subtype={subtype:?}"
                ),
            );
        }
        let frame_size = current
            .GetUINT64(&MF_MT_FRAME_SIZE)
            .map_err(|error| format!("query video dimensions: {error}"))?;
        let width = (frame_size >> 32) as u32;
        let height = frame_size as u32;
        if width == 0 || height == 0 {
            return Err(format!("invalid movie dimensions {width}x{height}"));
        }
        let stride = current
            .GetUINT32(&MF_MT_DEFAULT_STRIDE)
            .map(|value| value as i32)
            .or_else(|_| MFGetStrideForBitmapInfoHeader(subtype.data1, width))
            .unwrap_or(
                match format {
                    VideoPixelFormat::Nv12 => width as i32,
                    VideoPixelFormat::Bgra8 => (width * 4) as i32,
                },
            );
        Ok(VideoInfo {
            stream,
            width,
            height,
            stride,
            format,
        })
    }
    unsafe fn configure_audio(
        reader: &IMFSourceReader,
        stream: u32,
    ) -> Result<AudioInfo, String> {
        let media_type = MFCreateMediaType()
            .map_err(|error| format!("create audio type: {error}"))?;
        media_type
            .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)
            .map_err(|error| format!("set audio major type: {error}"))?;
        media_type
            .SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM)
            .map_err(|error| format!("set PCM subtype: {error}"))?;
        reader
            .SetCurrentMediaType(stream, None, &media_type)
            .map_err(|error| format!("configure PCM decoder: {error}"))?;
        reader
            .SetStreamSelection(stream, true)
            .map_err(|error| format!("select audio stream: {error}"))?;
        query_audio_info(reader, stream)
    }
    unsafe fn query_audio_info(
        reader: &IMFSourceReader,
        stream: u32,
    ) -> Result<AudioInfo, String> {
        let current = reader
            .GetCurrentMediaType(stream)
            .map_err(|error| format!("query audio type: {error}"))?;
        let major = current
            .GetGUID(&MF_MT_MAJOR_TYPE)
            .map_err(|error| format!("query audio major type: {error}"))?;
        let subtype = current
            .GetGUID(&MF_MT_SUBTYPE)
            .map_err(|error| format!("query audio subtype: {error}"))?;
        if major != MFMediaType_Audio || subtype != MFAudioFormat_PCM {
            return Err(
                format!(
                    "unexpected decoded audio format: major={major:?} subtype={subtype:?}"
                ),
            );
        }
        let channels = current
            .GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS)
            .map_err(|error| format!("query audio channels: {error}"))?;
        let sample_rate = current
            .GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND)
            .map_err(|error| format!("query audio sample rate: {error}"))?;
        let bits_per_sample = current
            .GetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE)
            .map_err(|error| format!("query audio bit depth: {error}"))?;
        let channels = u16::try_from(channels)
            .map_err(|_| "audio channel count is too large")?;
        if channels == 0 || sample_rate == 0 {
            return Err("invalid decoded audio format".into());
        }
        if !matches!(bits_per_sample, 8 | 16 | 24 | 32) {
            return Err(format!("unsupported PCM bit depth {bits_per_sample}"));
        }
        Ok(AudioInfo {
            stream,
            format: AudioFormat {
                channels,
                sample_rate,
            },
            bits_per_sample,
        })
    }
    fn ensure_stable_audio_format(
        previous: Option<&AudioInfo>,
        current: &AudioInfo,
    ) -> Result<(), String> {
        if previous
            .is_some_and(|previous| {
                previous.format != current.format
                    || previous.bits_per_sample != current.bits_per_sample
            })
        {
            return Err(
                format!(
                    "decoded audio format changed during playback: {:?}/{}-bit -> {:?}/{}-bit",
                    previous.unwrap().format, previous.unwrap().bits_per_sample, current
                    .format, current.bits_per_sample
                ),
            );
        }
        Ok(())
    }
    unsafe fn copy_video_sample(
        sample: &IMFSample,
        info: &VideoInfo,
    ) -> Result<RenderMoviePixels, String> {
        let buffer = sample
            .ConvertToContiguousBuffer()
            .map_err(|error| format!("make video buffer contiguous: {error}"))?;
        if info.format == VideoPixelFormat::Nv12 {
            let mut data = std::ptr::null_mut();
            let mut length = 0u32;
            buffer
                .Lock(&mut data, None, Some(&mut length))
                .map_err(|error| format!("lock contiguous NV12 video buffer: {error}"))?;
            let result = copy_nv12_rows(
                data,
                length as usize,
                info.stride,
                info.width,
                info.height,
            );
            let _ = buffer.Unlock();
            return result
                .map(|(luma, chroma)| RenderMoviePixels::Nv12 {
                    luma: Arc::new(luma),
                    chroma: Arc::new(chroma),
                });
        }
        if let Ok(buffer_2d) = buffer.cast::<IMF2DBuffer>() {
            let mut scanline = std::ptr::null_mut();
            let mut stride = 0i32;
            buffer_2d
                .Lock2D(&mut scanline, &mut stride)
                .map_err(|error| format!("lock video buffer: {error}"))?;
            let result = copy_bgra_rows(scanline, stride, info.width, info.height);
            let _ = buffer_2d.Unlock2D();
            return result.map(|pixels| RenderMoviePixels::Bgra8(Arc::new(pixels)));
        }
        let mut data = std::ptr::null_mut();
        let mut length = 0u32;
        buffer
            .Lock(&mut data, None, Some(&mut length))
            .map_err(|error| format!("lock contiguous video buffer: {error}"))?;
        let stride = if info.stride == 0 {
            (info.width * 4) as i32
        } else {
            info.stride
        };
        let required = stride.unsigned_abs() as usize * info.height as usize;
        let result = if required > length as usize {
            Err(format!("short RGB32 sample: {} bytes, need {required}", length))
        } else {
            let scanline = if stride < 0 {
                data.add((info.height as usize - 1) * stride.unsigned_abs() as usize)
            } else {
                data
            };
            copy_bgra_rows(scanline, stride, info.width, info.height)
        };
        let _ = buffer.Unlock();
        result.map(|pixels| RenderMoviePixels::Bgra8(Arc::new(pixels)))
    }
    pub unsafe fn copy_nv12_rows(
        data: *const u8,
        length: usize,
        stride: i32,
        width: u32,
        height: u32,
    ) -> Result<(Vec<u8>, Vec<u8>), String> {
        if data.is_null() {
            return Err("NV12 buffer returned a null pointer".into());
        }
        if width == 0 || height == 0 || !width.is_multiple_of(2)
            || !height.is_multiple_of(2)
        {
            return Err(format!("invalid NV12 dimensions {width}x{height}"));
        }
        if stride <= 0 || (stride as u32) < width {
            return Err(format!("invalid NV12 stride {stride} for width {width}"));
        }
        let stride = stride as usize;
        let height = height as usize;
        let width = width as usize;
        let chroma_rows = height / 2;
        let required = stride
            .checked_mul(height + chroma_rows)
            .ok_or("NV12 buffer size overflow")?;
        if length < required {
            return Err(format!("short NV12 sample: {length} bytes, need {required}"));
        }
        let bytes = std::slice::from_raw_parts(data, required);
        let mut luma = vec![0u8; width * height];
        let mut chroma = vec![0u8; width * chroma_rows];
        for row in 0..height {
            luma[row * width..(row + 1) * width]
                .copy_from_slice(&bytes[row * stride..row * stride + width]);
        }
        let chroma_base = stride * height;
        for row in 0..chroma_rows {
            chroma[row * width..(row + 1) * width]
                .copy_from_slice(
                    &bytes[chroma_base
                        + row * stride..chroma_base + row * stride + width],
                );
        }
        Ok((luma, chroma))
    }
    unsafe fn copy_bgra_rows(
        scanline0: *const u8,
        stride: i32,
        width: u32,
        height: u32,
    ) -> Result<Vec<u8>, String> {
        if scanline0.is_null() {
            return Err("RGB32 buffer returned a null scanline".into());
        }
        let row_bytes = width as usize * 4;
        if (stride.unsigned_abs() as usize) < row_bytes {
            return Err(
                format!("RGB32 stride {} is smaller than row width {row_bytes}", stride),
            );
        }
        let mut bgra = vec![0u8; row_bytes * height as usize];
        for y in 0..height as usize {
            let source = scanline0.offset(y as isize * stride as isize);
            let source = std::slice::from_raw_parts(source, row_bytes);
            let destination = &mut bgra[y * row_bytes..(y + 1) * row_bytes];
            destination.copy_from_slice(source);
        }
        Ok(bgra)
    }
    unsafe fn copy_audio_sample(
        sample: &IMFSample,
        info: &AudioInfo,
    ) -> Result<Vec<f32>, String> {
        let buffer = sample
            .ConvertToContiguousBuffer()
            .map_err(|error| format!("make audio buffer contiguous: {error}"))?;
        let mut data = std::ptr::null_mut();
        let mut length = 0u32;
        buffer
            .Lock(&mut data, None, Some(&mut length))
            .map_err(|error| format!("lock audio buffer: {error}"))?;
        let bytes = std::slice::from_raw_parts(data, length as usize);
        let mut samples: Vec<f32> = match info.bits_per_sample {
            8 => bytes.iter().map(|sample| (*sample as f32 - 128.0) / 128.0).collect(),
            16 => {
                bytes
                    .chunks_exact(2)
                    .map(|sample| {
                        i16::from_le_bytes([sample[0], sample[1]]) as f32 / 32768.0
                    })
                    .collect()
            }
            24 => {
                bytes
                    .chunks_exact(3)
                    .map(|sample| {
                        let value = ((sample[0] as i32) | ((sample[1] as i32) << 8)
                            | ((sample[2] as i32) << 16)) << 8 >> 8;
                        value as f32 / 8_388_608.0
                    })
                    .collect()
            }
            32 => {
                bytes
                    .chunks_exact(4)
                    .map(|sample| {
                        i32::from_le_bytes([sample[0], sample[1], sample[2], sample[3]])
                            as f32 / 2_147_483_648.0
                    })
                    .collect()
            }
            _ => unreachable!(),
        };
        let _ = buffer.Unlock();
        let channels = info.format.channels as usize;
        samples.truncate(samples.len() / channels * channels);
        Ok(samples)
    }
    fn hns_to_duration(value: i64) -> Duration {
        Duration::from_nanos(value.max(0) as u64 * 100)
    }
}
#[cfg(windows)]
pub use imp::MediaFoundationDecoder;
#[cfg(windows)]
pub use imp::copy_nv12_rows;

