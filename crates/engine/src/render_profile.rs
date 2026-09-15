use crate::Instant;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
const GPU_QUERY_SLOTS: usize = 4;
const SUMMARY_QUERY_CAPACITY: u32 = 128;
const DEEP_QUERY_CAPACITY: u32 = 2_048;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderProfileMode {
    Summary,
    Deep,
}
#[derive(Clone, Debug)]
pub struct RenderProfileConfig {
    pub mode: RenderProfileMode,
    pub output: PathBuf,
}
impl RenderProfileConfig {
    pub fn new(mode: RenderProfileMode, output: impl Into<PathBuf>) -> Self {
        Self {
            mode,
            output: output.into(),
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct RuntimeCpuTimings {
    pub schedule_us: u128,
    pub host_frame_us: u128,
}
#[derive(Clone, Debug, Default, Serialize)]
pub(crate) struct SubmitProfile {
    pub submit_id: u64,
    pub total_us: u128,
    pub sensitivity_us: u128,
    pub pages_us: u128,
    pub quad_build_us: u128,
    pub dirty_pages: usize,
    pub released_pages: usize,
    pub page_ops: usize,
    pub quads: usize,
}
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub(crate) struct ResourceCounters {
    pub textures_created: u64,
    pub texture_bytes_created: u64,
    pub texture_upload_bytes: u64,
    pub texture_copy_bytes: u64,
    pub buffers_created: u64,
    pub bind_groups_created: u64,
    pub render_passes: u64,
    pub draw_calls: u64,
}
#[derive(Clone, Debug, Default, Serialize)]
pub(crate) struct OperationProfile {
    pub counts: BTreeMap<String, u64>,
    pub affected_pixels: u64,
    pub estimated_rw_bytes: u64,
}
#[derive(Clone, Debug, Serialize)]
pub(crate) struct TextureMemoryEntry {
    pub id: u64,
    pub category: String,
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
    pub aliases: Vec<String>,
}
#[derive(Clone, Debug, Default, Serialize)]
pub(crate) struct TextureMemorySnapshot {
    pub resident_bytes: u64,
    pub fixed_target_bytes: u64,
    pub surface_estimated_bytes: u64,
    pub unique_textures: usize,
    pub page_cache_entries: usize,
    pub shared_page_cache_entries: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub textures: Vec<TextureMemoryEntry>,
}
#[derive(Clone, Debug, Default, Serialize)]
pub(crate) struct DrawCpuProfile {
    pub total_us: u128,
    pub surface_acquire_us: u128,
    pub page_ops_encode_us: u128,
    pub composition_encode_us: u128,
    pub queue_submit_us: u128,
    pub present_us: u128,
    pub profile_collect_us: u128,
}
#[derive(Clone, Debug, Serialize)]
pub(crate) struct CpuFrameProfile {
    pub frame_id: u64,
    pub elapsed_ms: u128,
    pub runtime: RuntimeCpuTimings,
    pub submit: SubmitProfile,
    pub draw: DrawCpuProfile,
    pub operations: OperationProfile,
    pub draw_modes: BTreeMap<u32, u64>,
    pub resources: ResourceCounters,
    pub memory: TextureMemorySnapshot,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub marks: Vec<MarkEvent>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub op_pages: Vec<OpPageStat>,
}
#[derive(Clone, Debug, Serialize)]
pub(crate) struct MarkEvent {
    pub name: String,
    pub detail: String,
}
#[derive(Clone, Debug, Serialize)]
pub(crate) struct OpPageStat {
    pub op: String,
    pub page: u32,
    pub count: u64,
    pub pixels: u64,
    pub max_op_pixels: u64,
}
#[derive(Clone, Debug, Serialize)]
pub(crate) struct GpuSpanTag {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub draw_mode: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pixels: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_bytes: Option<u64>,
}
impl GpuSpanTag {
    pub(crate) fn stage(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            page: None,
            resource: None,
            draw_mode: None,
            pixels: None,
            estimated_bytes: None,
        }
    }
}
#[derive(Clone, Debug)]
struct GpuSpanDescriptor {
    tag: GpuSpanTag,
    begin: u32,
    end: u32,
}
#[derive(Clone, Debug, Serialize)]
struct GpuSpanResult {
    #[serde(flatten)]
    tag: GpuSpanTag,
    gpu_us: f64,
}
#[derive(Clone, Debug, Serialize)]
struct GpuFrameProfile {
    event: &'static str,
    frame_id: u64,
    overflowed_spans: u32,
    spans: Vec<GpuSpanResult>,
}
#[derive(Clone, Debug)]
struct PendingGpuFrame {
    frame_id: u64,
    query_count: u32,
    overflowed_spans: u32,
    spans: Vec<GpuSpanDescriptor>,
}
struct GpuQuerySlot {
    query_set: wgpu::QuerySet,
    resolve_buffer: wgpu::Buffer,
    readback_buffer: wgpu::Buffer,
    pending: Option<PendingGpuFrame>,
}
struct QueryCompletion {
    slot: usize,
    success: bool,
}
struct GpuQueryProfiler {
    slots: Vec<GpuQuerySlot>,
    next_slot: usize,
    capacity: u32,
    timestamp_period_ns: f64,
    encoder_timestamps: bool,
    pass_timestamps: bool,
    sender: Sender<QueryCompletion>,
    receiver: Receiver<QueryCompletion>,
    skipped_frames: u64,
}
#[derive(Clone, Debug)]
pub(crate) struct TimestampSpan {
    query_set: wgpu::QuerySet,
    begin: u32,
    end: u32,
}
impl TimestampSpan {
    pub(crate) fn write_begin(&self, encoder: &mut wgpu::CommandEncoder) {
        encoder.write_timestamp(&self.query_set, self.begin);
    }
    pub(crate) fn write_end(&self, encoder: &mut wgpu::CommandEncoder) {
        encoder.write_timestamp(&self.query_set, self.end);
    }
    pub(crate) fn write_begin_in_pass(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.write_timestamp(&self.query_set, self.begin);
    }
    pub(crate) fn write_end_in_pass(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.write_timestamp(&self.query_set, self.end);
    }
    pub(crate) fn pass_writes(&self) -> wgpu::RenderPassTimestampWrites<'_> {
        wgpu::RenderPassTimestampWrites {
            query_set: &self.query_set,
            beginning_of_pass_write_index: Some(self.begin),
            end_of_pass_write_index: Some(self.end),
        }
    }
}
pub(crate) struct ActiveGpuFrame {
    slot: usize,
    query_set: wgpu::QuerySet,
    resolve_buffer: wgpu::Buffer,
    readback_buffer: wgpu::Buffer,
    capacity: u32,
    next_query: u32,
    spans: Vec<GpuSpanDescriptor>,
    overflowed_spans: u32,
    deep: bool,
    encoder_timestamps: bool,
    inside_pass_timestamps: bool,
}
impl ActiveGpuFrame {
    fn reserve(&mut self, tag: GpuSpanTag) -> Option<TimestampSpan> {
        if self.next_query.saturating_add(2) > self.capacity {
            self.overflowed_spans = self.overflowed_spans.saturating_add(1);
            return None;
        }
        let begin = self.next_query;
        let end = begin + 1;
        self.next_query += 2;
        self.spans
            .push(GpuSpanDescriptor {
                tag,
                begin,
                end,
            });
        Some(TimestampSpan {
            query_set: self.query_set.clone(),
            begin,
            end,
        })
    }
    pub(crate) fn encoder_span(&mut self, tag: GpuSpanTag) -> Option<TimestampSpan> {
        self.encoder_timestamps.then(|| self.reserve(tag)).flatten()
    }
    pub(crate) fn pass_span(&mut self, tag: GpuSpanTag) -> Option<TimestampSpan> {
        self.reserve(tag)
    }
    pub(crate) fn detail_span(&mut self, tag: GpuSpanTag) -> Option<TimestampSpan> {
        (self.deep && self.inside_pass_timestamps).then(|| self.reserve(tag)).flatten()
    }
    fn encode_resolve(&self, encoder: &mut wgpu::CommandEncoder) {
        if self.next_query == 0 {
            return;
        }
        let byte_count = u64::from(self.next_query) * 8;
        encoder
            .resolve_query_set(
                &self.query_set,
                0..self.next_query,
                &self.resolve_buffer,
                0,
            );
        encoder
            .copy_buffer_to_buffer(
                &self.resolve_buffer,
                0,
                &self.readback_buffer,
                0,
                byte_count,
            );
    }
}
#[derive(Default)]
struct Aggregate {
    count: u64,
    total_us: f64,
    max_us: f64,
}
#[derive(Serialize)]
struct AggregateRecord {
    count: u64,
    total_us: f64,
    average_us: f64,
    max_us: f64,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct GpuResourceKey {
    name: String,
    page: Option<u32>,
    resource: Option<String>,
    draw_mode: Option<u32>,
}
#[derive(Clone, Serialize)]
struct GpuResourceAggregateRecord {
    name: String,
    page: Option<u32>,
    resource: Option<String>,
    draw_mode: Option<u32>,
    count: u64,
    total_us: f64,
    average_us: f64,
    max_us: f64,
}
#[derive(Clone, Serialize)]
struct CpuHotFrame {
    frame_id: u64,
    elapsed_ms: u128,
    total_us: u128,
    surface_acquire_us: u128,
    page_ops_encode_us: u128,
    composition_encode_us: u128,
    texture_copy_bytes: u64,
    texture_upload_bytes: u64,
    resident_bytes: u64,
    page_ops: usize,
    quads: usize,
    draw_calls: u64,
}
impl From<&CpuFrameProfile> for CpuHotFrame {
    fn from(frame: &CpuFrameProfile) -> Self {
        Self {
            frame_id: frame.frame_id,
            elapsed_ms: frame.elapsed_ms,
            total_us: frame.draw.total_us,
            surface_acquire_us: frame.draw.surface_acquire_us,
            page_ops_encode_us: frame.draw.page_ops_encode_us,
            composition_encode_us: frame.draw.composition_encode_us,
            texture_copy_bytes: frame.resources.texture_copy_bytes,
            texture_upload_bytes: frame.resources.texture_upload_bytes,
            resident_bytes: frame.memory.resident_bytes,
            page_ops: frame.submit.page_ops,
            quads: frame.submit.quads,
            draw_calls: frame.resources.draw_calls,
        }
    }
}
#[derive(Serialize)]
struct HeaderEvent {
    event: &'static str,
    format_version: u32,
    mode: RenderProfileMode,
    adapter: String,
    backend: String,
    driver: String,
    driver_info: String,
    presentation_scale: u32,
    surface_width: u32,
    surface_height: u32,
    timestamp_query: bool,
    timestamp_inside_encoders: bool,
    timestamp_inside_passes: bool,
    timestamp_period_ns: Option<f64>,
    memory_note: &'static str,
}
#[derive(Serialize)]
struct CpuFrameEvent<'a> {
    event: &'static str,
    #[serde(flatten)]
    frame: &'a CpuFrameProfile,
}
#[derive(Serialize)]
struct SummaryEvent {
    event: &'static str,
    cpu_frames: usize,
    gpu_frames: usize,
    gpu_skipped_frames: u64,
    cpu_draw_average_us: f64,
    cpu_draw_p95_us: f64,
    cpu_draw_p99_us: f64,
    gpu_frame_average_us: f64,
    gpu_frame_p95_us: f64,
    gpu_frame_p99_us: f64,
    peak_resident_bytes: u64,
    peak_frame_id: u64,
    gpu_spans: BTreeMap<String, AggregateRecord>,
    gpu_resources: Vec<GpuResourceAggregateRecord>,
    slowest_cpu_frames: Vec<CpuHotFrame>,
    highest_copy_frames: Vec<CpuHotFrame>,
    peak_textures: Vec<TextureMemoryEntry>,
}
pub(crate) struct RenderProfiler {
    output: PathBuf,
    writer: BufWriter<File>,
    started: Instant,
    gpu: Option<GpuQueryProfiler>,
    cpu_draw_us: Vec<f64>,
    gpu_frame_us: Vec<f64>,
    gpu_aggregates: BTreeMap<String, Aggregate>,
    gpu_resource_aggregates: BTreeMap<GpuResourceKey, Aggregate>,
    slowest_cpu_frames: Vec<CpuHotFrame>,
    highest_copy_frames: Vec<CpuHotFrame>,
    peak_resident_bytes: u64,
    peak_frame_id: u64,
    peak_textures: Vec<TextureMemoryEntry>,
    last_texture_signature: Option<u64>,
    gpu_frames: usize,
    finished: bool,
}
impl RenderProfiler {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        config: RenderProfileConfig,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        adapter_info: &wgpu::AdapterInfo,
        presentation_scale: u32,
        surface_width: u32,
        surface_height: u32,
        timestamp_query: bool,
        encoder_timestamps: bool,
        pass_timestamps: bool,
    ) -> std::io::Result<Self> {
        if let Some(parent) = config
            .output
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)?;
        }
        let file = File::create(&config.output)?;
        let mut profiler = Self {
            output: config.output,
            writer: BufWriter::new(file),
            started: Instant::now(),
            gpu: timestamp_query
                .then(|| {
                    GpuQueryProfiler::new(
                        device,
                        queue,
                        config.mode,
                        encoder_timestamps,
                        pass_timestamps,
                    )
                }),
            cpu_draw_us: Vec::new(),
            gpu_frame_us: Vec::new(),
            gpu_aggregates: BTreeMap::new(),
            gpu_resource_aggregates: BTreeMap::new(),
            slowest_cpu_frames: Vec::new(),
            highest_copy_frames: Vec::new(),
            peak_resident_bytes: 0,
            peak_frame_id: 0,
            peak_textures: Vec::new(),
            last_texture_signature: None,
            gpu_frames: 0,
            finished: false,
        };
        let header = HeaderEvent {
            event: "header",
            format_version: 1,
            mode: config.mode,
            adapter: adapter_info.name.clone(),
            backend: format!("{:?}", adapter_info.backend),
            driver: adapter_info.driver.clone(),
            driver_info: adapter_info.driver_info.clone(),
            presentation_scale,
            surface_width,
            surface_height,
            timestamp_query,
            timestamp_inside_encoders: encoder_timestamps,
            timestamp_inside_passes: pass_timestamps,
            timestamp_period_ns: timestamp_query
                .then(|| f64::from(queue.get_timestamp_period())),
            memory_note: "texture bytes are RGBA/BGRA payload estimates and exclude driver heap padding",
        };
        profiler.write_json(&header)?;
        profiler.writer.flush()?;
        std::eprintln!(
            "[RENDER-PROFILE] mode={:?} output={} gpu_timestamps={} encoder={} passes={}",
            config.mode, profiler.output.display(), timestamp_query, encoder_timestamps,
            pass_timestamps
        );
        Ok(profiler)
    }
    pub(crate) fn elapsed_ms(&self) -> u128 {
        self.started.elapsed().as_millis()
    }
    pub(crate) fn begin_gpu_frame(&mut self, frame_id: u64) -> Option<ActiveGpuFrame> {
        self.gpu.as_mut()?.begin_frame(frame_id)
    }
    pub(crate) fn collect_gpu_frames(&mut self) {
        let Some(gpu) = self.gpu.as_mut() else {
            return;
        };
        let records = gpu.take_completed();
        for record in records {
            self.record_gpu_frame(record);
        }
    }
    pub(crate) fn resolve_gpu_frame(
        &mut self,
        active: &ActiveGpuFrame,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        active.encode_resolve(encoder);
    }
    pub(crate) fn finish_gpu_frame(&mut self, frame_id: u64, active: ActiveGpuFrame) {
        if let Some(gpu) = self.gpu.as_mut() {
            gpu.finish_frame(frame_id, active);
        }
    }
    pub(crate) fn record_cpu_frame(&mut self, mut frame: CpuFrameProfile) {
        self.cpu_draw_us.push(frame.draw.total_us as f64);
        let hot_frame = CpuHotFrame::from(&frame);
        self.slowest_cpu_frames.push(hot_frame.clone());
        self.slowest_cpu_frames.sort_by_key(|frame| std::cmp::Reverse(frame.total_us));
        self.slowest_cpu_frames.truncate(20);
        self.highest_copy_frames.push(hot_frame);
        self.highest_copy_frames
            .sort_by(|left, right| {
                right
                    .texture_copy_bytes
                    .cmp(&left.texture_copy_bytes)
                    .then_with(|| right.total_us.cmp(&left.total_us))
            });
        self.highest_copy_frames.truncate(20);
        if frame.memory.resident_bytes > self.peak_resident_bytes {
            self.peak_resident_bytes = frame.memory.resident_bytes;
            self.peak_frame_id = frame.frame_id;
            self.peak_textures = frame.memory.textures.clone();
        }
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        frame.memory.resident_bytes.hash(&mut hasher);
        frame.memory.fixed_target_bytes.hash(&mut hasher);
        frame.memory.page_cache_entries.hash(&mut hasher);
        for texture in &frame.memory.textures {
            texture.id.hash(&mut hasher);
            texture.bytes.hash(&mut hasher);
            texture.aliases.hash(&mut hasher);
        }
        let signature = hasher.finish();
        if self.last_texture_signature == Some(signature) {
            frame.memory.textures.clear();
        } else {
            self.last_texture_signature = Some(signature);
        }
        let _ = self
            .write_json(
                &CpuFrameEvent {
                    event: "frame_cpu",
                    frame: &frame,
                },
            );
    }
    pub(crate) fn poll_gpu(&mut self, device: &wgpu::Device) {
        if self.gpu.is_none() {
            return;
        }
        let _ = device.poll(wgpu::PollType::Poll);
        self.collect_gpu_frames();
    }
    pub(crate) fn finish(&mut self, device: &wgpu::Device) {
        if self.finished {
            return;
        }
        if self.gpu.is_some() {
            let _ = device.poll(wgpu::PollType::Wait);
            self.poll_gpu(device);
        }
        let gpu_spans = self
            .gpu_aggregates
            .iter()
            .map(|(name, aggregate)| {
                (
                    name.clone(),
                    AggregateRecord {
                        count: aggregate.count,
                        total_us: aggregate.total_us,
                        average_us: average(
                            aggregate.total_us,
                            aggregate.count as usize,
                        ),
                        max_us: aggregate.max_us,
                    },
                )
            })
            .collect();
        let mut gpu_resources = self
            .gpu_resource_aggregates
            .iter()
            .map(|(key, aggregate)| GpuResourceAggregateRecord {
                name: key.name.clone(),
                page: key.page,
                resource: key.resource.clone(),
                draw_mode: key.draw_mode,
                count: aggregate.count,
                total_us: aggregate.total_us,
                average_us: average(aggregate.total_us, aggregate.count as usize),
                max_us: aggregate.max_us,
            })
            .collect::<Vec<_>>();
        gpu_resources.sort_by(|left, right| right.total_us.total_cmp(&left.total_us));
        let summary = SummaryEvent {
            event: "summary",
            cpu_frames: self.cpu_draw_us.len(),
            gpu_frames: self.gpu_frames,
            gpu_skipped_frames: self
                .gpu
                .as_ref()
                .map(|gpu| gpu.skipped_frames)
                .unwrap_or(0),
            cpu_draw_average_us: values_average(&self.cpu_draw_us),
            cpu_draw_p95_us: percentile(&self.cpu_draw_us, 0.95),
            cpu_draw_p99_us: percentile(&self.cpu_draw_us, 0.99),
            gpu_frame_average_us: values_average(&self.gpu_frame_us),
            gpu_frame_p95_us: percentile(&self.gpu_frame_us, 0.95),
            gpu_frame_p99_us: percentile(&self.gpu_frame_us, 0.99),
            peak_resident_bytes: self.peak_resident_bytes,
            peak_frame_id: self.peak_frame_id,
            gpu_spans,
            gpu_resources,
            slowest_cpu_frames: self.slowest_cpu_frames.clone(),
            highest_copy_frames: self.highest_copy_frames.clone(),
            peak_textures: self.peak_textures.clone(),
        };
        let _ = self.write_json(&summary);
        let _ = self.write_markdown_summary(&summary);
        let _ = self.writer.flush();
        self.finished = true;
        std::eprintln!(
            "[RENDER-PROFILE] wrote {} CPU frames and {} GPU frames to {}", self
            .cpu_draw_us.len(), self.gpu_frames, self.output.display()
        );
    }
    fn record_gpu_frame(&mut self, frame: GpuFrameProfile) {
        self.gpu_frames += 1;
        for span in &frame.spans {
            let aggregate = self
                .gpu_aggregates
                .entry(span.tag.name.clone())
                .or_default();
            aggregate.count += 1;
            aggregate.total_us += span.gpu_us;
            aggregate.max_us = aggregate.max_us.max(span.gpu_us);
            if span.tag.page.is_some() || span.tag.resource.is_some()
                || span.tag.draw_mode.is_some()
            {
                let key = GpuResourceKey {
                    name: span.tag.name.clone(),
                    page: span.tag.page,
                    resource: span.tag.resource.clone(),
                    draw_mode: span.tag.draw_mode,
                };
                let aggregate = self.gpu_resource_aggregates.entry(key).or_default();
                aggregate.count += 1;
                aggregate.total_us += span.gpu_us;
                aggregate.max_us = aggregate.max_us.max(span.gpu_us);
            }
            if span.tag.name == "frame" {
                self.gpu_frame_us.push(span.gpu_us);
            }
        }
        let _ = self.write_json(&frame);
    }
    fn write_json(&mut self, value: &impl Serialize) -> std::io::Result<()> {
        serde_json::to_writer(&mut self.writer, value)?;
        self.writer.write_all(b"\n")
    }
    fn write_markdown_summary(&self, summary: &SummaryEvent) -> std::io::Result<()> {
        let path = self.output.with_extension("summary.md");
        let mut file = BufWriter::new(File::create(path)?);
        writeln!(file, "# Renderer profile summary\n")?;
        writeln!(file, "- CPU frames: {}", summary.cpu_frames)?;
        writeln!(file, "- GPU frames: {}", summary.gpu_frames)?;
        writeln!(file, "- GPU samples skipped: {}", summary.gpu_skipped_frames)?;
        writeln!(
            file, "- Peak tracked textures: {:.2} MiB at frame {}\n", summary
            .peak_resident_bytes as f64 / (1024.0 * 1024.0), summary.peak_frame_id
        )?;
        writeln!(file, "## Frame time\n")?;
        writeln!(file, "| Source | Average | P95 | P99 |")?;
        writeln!(file, "|---|---:|---:|---:|")?;
        writeln!(
            file, "| CPU draw | {:.3} ms | {:.3} ms | {:.3} ms |", summary
            .cpu_draw_average_us / 1000.0, summary.cpu_draw_p95_us / 1000.0, summary
            .cpu_draw_p99_us / 1000.0
        )?;
        writeln!(
            file, "| GPU frame | {:.3} ms | {:.3} ms | {:.3} ms |\n", summary
            .gpu_frame_average_us / 1000.0, summary.gpu_frame_p95_us / 1000.0, summary
            .gpu_frame_p99_us / 1000.0
        )?;
        writeln!(file, "## Slowest CPU frames\n")?;
        writeln!(
            file,
            "| Frame | Elapsed | Total | Surface | PageOps | Composition | Copy | Resident |"
        )?;
        writeln!(file, "|---:|---:|---:|---:|---:|---:|---:|---:|")?;
        for frame in &summary.slowest_cpu_frames {
            writeln!(
                file,
                "| {} | {:.3} s | {:.3} ms | {:.3} ms | {:.3} ms | {:.3} ms | {:.2} GiB | {:.2} MiB |",
                frame.frame_id, frame.elapsed_ms as f64 / 1000.0, frame.total_us as f64 /
                1000.0, frame.surface_acquire_us as f64 / 1000.0, frame
                .page_ops_encode_us as f64 / 1000.0, frame.composition_encode_us as f64 /
                1000.0, frame.texture_copy_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
                frame.resident_bytes as f64 / (1024.0 * 1024.0)
            )?;
        }
        writeln!(file, "\n## Highest texture-copy frames\n")?;
        writeln!(
            file, "| Frame | Elapsed | Copy | Upload | PageOps | Quads | Draw calls |"
        )?;
        writeln!(file, "|---:|---:|---:|---:|---:|---:|---:|")?;
        for frame in &summary.highest_copy_frames {
            writeln!(
                file, "| {} | {:.3} s | {:.2} GiB | {:.2} MiB | {} | {} | {} |", frame
                .frame_id, frame.elapsed_ms as f64 / 1000.0, frame.texture_copy_bytes as
                f64 / (1024.0 * 1024.0 * 1024.0), frame.texture_upload_bytes as f64 /
                (1024.0 * 1024.0), frame.page_ops, frame.quads, frame.draw_calls
            )?;
        }
        let mut spans = summary.gpu_spans.iter().collect::<Vec<_>>();
        spans.sort_by(|left, right| right.1.total_us.total_cmp(&left.1.total_us));
        writeln!(file, "## GPU spans\n")?;
        writeln!(file, "| Span | Count | Total | Average | Maximum |")?;
        writeln!(file, "|---|---:|---:|---:|---:|")?;
        for (name, aggregate) in spans.into_iter().take(30) {
            writeln!(
                file, "| {} | {} | {:.3} ms | {:.3} ms | {:.3} ms |", name.replace('|',
                "\\|"), aggregate.count, aggregate.total_us / 1000.0, aggregate
                .average_us / 1000.0, aggregate.max_us / 1000.0
            )?;
        }
        writeln!(file, "\n## GPU resources\n")?;
        writeln!(
            file, "| Span | Resource | Page | Mode | Count | Total | Average | Maximum |"
        )?;
        writeln!(file, "|---|---|---:|---:|---:|---:|---:|---:|")?;
        for aggregate in summary.gpu_resources.iter().take(30) {
            writeln!(
                file, "| {} | {} | {} | {} | {} | {:.3} ms | {:.3} ms | {:.3} ms |",
                aggregate.name.replace('|', "\\|"), aggregate.resource.as_deref()
                .unwrap_or("-").replace('|', "\\|"), aggregate.page.map(| value | value
                .to_string()).unwrap_or_else(|| "-".to_owned()), aggregate.draw_mode
                .map(| value | value.to_string()).unwrap_or_else(|| "-".to_owned()),
                aggregate.count, aggregate.total_us / 1000.0, aggregate.average_us /
                1000.0, aggregate.max_us / 1000.0
            )?;
        }
        writeln!(file, "\n## Largest textures at peak\n")?;
        writeln!(file, "| ID | Category | Size | MiB | Aliases |")?;
        writeln!(file, "|---:|---|---:|---:|---|")?;
        for texture in &summary.peak_textures {
            writeln!(
                file, "| {} | {} | {}x{} | {:.2} | {} |", texture.id, texture.category
                .replace('|', "\\|"), texture.width, texture.height, texture.bytes as f64
                / (1024.0 * 1024.0), texture.aliases.join(", ").replace('|', "\\|")
            )?;
        }
        file.flush()
    }
}
impl GpuQueryProfiler {
    fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mode: RenderProfileMode,
        encoder_timestamps: bool,
        pass_timestamps: bool,
    ) -> Self {
        let capacity = match mode {
            RenderProfileMode::Summary => SUMMARY_QUERY_CAPACITY,
            RenderProfileMode::Deep => DEEP_QUERY_CAPACITY,
        };
        let byte_size = u64::from(capacity) * 8;
        let slots = (0..GPU_QUERY_SLOTS)
            .map(|_| GpuQuerySlot {
                query_set: device
                    .create_query_set(
                        &wgpu::QuerySetDescriptor {
                            label: Some("render profiler timestamp queries"),
                            ty: wgpu::QueryType::Timestamp,
                            count: capacity,
                        },
                    ),
                resolve_buffer: device
                    .create_buffer(
                        &wgpu::BufferDescriptor {
                            label: Some("render profiler query resolve"),
                            size: byte_size,
                            usage: wgpu::BufferUsages::QUERY_RESOLVE
                                | wgpu::BufferUsages::COPY_SRC,
                            mapped_at_creation: false,
                        },
                    ),
                readback_buffer: device
                    .create_buffer(
                        &wgpu::BufferDescriptor {
                            label: Some("render profiler query readback"),
                            size: byte_size,
                            usage: wgpu::BufferUsages::COPY_DST
                                | wgpu::BufferUsages::MAP_READ,
                            mapped_at_creation: false,
                        },
                    ),
                pending: None,
            })
            .collect();
        let (sender, receiver) = mpsc::channel();
        Self {
            slots,
            next_slot: 0,
            capacity,
            timestamp_period_ns: f64::from(queue.get_timestamp_period()),
            encoder_timestamps,
            pass_timestamps,
            sender,
            receiver,
            skipped_frames: 0,
        }
    }
    fn begin_frame(&mut self, _frame_id: u64) -> Option<ActiveGpuFrame> {
        let slot_index = (0..self.slots.len())
            .map(|offset| (self.next_slot + offset) % self.slots.len())
            .find(|&index| self.slots[index].pending.is_none());
        let Some(slot_index) = slot_index else {
            self.skipped_frames += 1;
            return None;
        };
        self.next_slot = (slot_index + 1) % self.slots.len();
        let slot = &self.slots[slot_index];
        Some(ActiveGpuFrame {
            slot: slot_index,
            query_set: slot.query_set.clone(),
            resolve_buffer: slot.resolve_buffer.clone(),
            readback_buffer: slot.readback_buffer.clone(),
            capacity: self.capacity,
            next_query: 0,
            spans: Vec::new(),
            overflowed_spans: 0,
            deep: self.capacity == DEEP_QUERY_CAPACITY,
            encoder_timestamps: self.encoder_timestamps,
            inside_pass_timestamps: self.pass_timestamps,
        })
    }
    fn finish_frame(&mut self, frame_id: u64, active: ActiveGpuFrame) {
        let slot_index = active.slot;
        if active.next_query == 0 {
            return;
        }
        let byte_count = u64::from(active.next_query) * 8;
        self.slots[slot_index].pending = Some(PendingGpuFrame {
            frame_id,
            query_count: active.next_query,
            overflowed_spans: active.overflowed_spans,
            spans: active.spans,
        });
        let sender = self.sender.clone();
        active
            .readback_buffer
            .slice(0..byte_count)
            .map_async(
                wgpu::MapMode::Read,
                move |result| {
                    let _ = sender
                        .send(QueryCompletion {
                            slot: slot_index,
                            success: result.is_ok(),
                        });
                },
            );
    }
    fn take_completed(&mut self) -> Vec<GpuFrameProfile> {
        let mut records = Vec::new();
        while let Ok(completion) = self.receiver.try_recv() {
            let slot = &mut self.slots[completion.slot];
            let Some(pending) = slot.pending.take() else {
                continue;
            };
            if !completion.success {
                slot.readback_buffer.unmap();
                continue;
            }
            let byte_count = u64::from(pending.query_count) * 8;
            let view = slot.readback_buffer.slice(0..byte_count).get_mapped_range();
            let timestamps = view
                .chunks_exact(8)
                .map(|bytes| u64::from_le_bytes(
                    bytes.try_into().expect("eight-byte timestamp"),
                ))
                .collect::<Vec<_>>();
            drop(view);
            slot.readback_buffer.unmap();
            let spans = pending
                .spans
                .into_iter()
                .filter_map(|span| {
                    let begin = *timestamps.get(span.begin as usize)?;
                    let end = *timestamps.get(span.end as usize)?;
                    Some(GpuSpanResult {
                        tag: span.tag,
                        gpu_us: end.saturating_sub(begin) as f64
                            * self.timestamp_period_ns / 1_000.0,
                    })
                })
                .collect();
            records
                .push(GpuFrameProfile {
                    event: "frame_gpu",
                    frame_id: pending.frame_id,
                    overflowed_spans: pending.overflowed_spans,
                    spans,
                });
        }
        records
    }
}
fn average(total: f64, count: usize) -> f64 {
    if count == 0 { 0.0 } else { total / count as f64 }
}
fn values_average(values: &[f64]) -> f64 {
    average(values.iter().sum(), values.len())
}
fn percentile(values: &[f64], percentile: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let index = ((sorted.len() - 1) as f64 * percentile).ceil() as usize;
    sorted[index.min(sorted.len() - 1)]
}

