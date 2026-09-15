use crate::gpu::{GpuContext, Vertex};
use crate::patch::PresentationConfig;
use crate::render_model::{
    PageColorMode, PageCopyMode, PageFillMode, PageOp, RenderFrame, RenderMovieFrame,
    RenderMoviePixels,
};
use crate::render_profile::{
    ActiveGpuFrame, CpuFrameProfile, DrawCpuProfile, GpuSpanTag, MarkEvent, OpPageStat,
    OperationProfile, RenderProfileConfig, RenderProfiler, ResourceCounters,
    RuntimeCpuTimings, SubmitProfile, TextureMemoryEntry, TextureMemorySnapshot,
};
use crate::Instant;
use std::cell::Cell;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::{Arc, Weak};
use wgpu::util::DeviceExt;
struct GpuImage {
    id: u64,
    category: &'static str,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    width: u32,
    height: u32,
    byte_size: u64,
}
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct QuadUniform {
    native_transparency: u32,
    source_has_alpha: u32,
    draw_mode: u32,
    _padding: u32,
    color_a: u32,
    color_b: u32,
    parameter: i32,
    _padding_b: u32,
}
#[derive(Clone)]
struct Quad {
    page: Option<u32>,
    image: std::sync::Arc<GpuImage>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    u0: f32,
    v0: f32,
    u1: f32,
    v1: f32,
    alpha: f32,
    rotation_degrees: f32,
    scale_x: f32,
    scale_y: f32,
    source_has_alpha: bool,
    draw_mode: u32,
}
#[derive(Clone)]
enum MovieGpuFrame {
    Bgra8(Arc<GpuImage>),
    Nv12 {
        luma: Arc<GpuImage>,
        chroma: Arc<GpuImage>,
        bind_group: Arc<wgpu::BindGroup>,
    },
}
#[derive(Clone)]
struct MovieOverlay {
    frame: MovieGpuFrame,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}
struct CachedPage {
    revision: u64,
    image: Arc<GpuImage>,
    last_used_submit: u64,
    logical_pixels: Arc<Vec<u8>>,
    logical_width: u32,
    logical_height: u32,
    debug_name: Option<Arc<str>>,
    shared: bool,
    presentation_sensitive: bool,
    seed_identity: Option<PageTextureIdentity>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum PageTextureIdentity {
    Content(u64, u32, u32),
}
fn seed_pool_budget_bytes() -> u64 {
    const DEFAULT_MB: u64 = 256;
    crate::diag::env_knob_u64("OSTB_SEED_POOL_MB", DEFAULT_MB)
        .saturating_mul(1024 * 1024)
}
fn hash_rgba_pixels(pixels: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut chunks = pixels.chunks_exact(8);
    for chunk in &mut chunks {
        let word = u64::from_le_bytes(chunk.try_into().expect("chunk is 8 bytes"));
        hash = (hash ^ word).wrapping_mul(0x100_0000_01b3);
    }
    for byte in chunks.remainder() {
        hash = (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3);
    }
    hash
}
fn propagate_presentation_sensitivity(
    mut sensitive: HashSet<u32>,
    patched_pages: impl IntoIterator<Item = u32>,
    operations: &[PageOp],
) -> HashSet<u32> {
    sensitive.extend(patched_pages);
    sensitive
        .extend(
            operations
                .iter()
                .filter_map(|operation| match operation {
                    PageOp::PresentationOverlay { destination, .. } => Some(*destination),
                    PageOp::Copy { destination, source_is_presented_scene: true, .. }
                    | PageOp::TransformCopy {
                        destination,
                        source_is_presented_scene: true,
                        ..
                    } => Some(*destination),
                    _ => None,
                }),
        );
    for operation in operations {
        match operation {
            PageOp::Copy { source, destination, source_is_presented_scene, .. }
            | PageOp::TransformCopy {
                source,
                destination,
                source_is_presented_scene,
                ..
            } => {
                if *source_is_presented_scene || sensitive.contains(source) {
                    sensitive.insert(*destination);
                }
            }
            PageOp::Swap { source, destination, .. } => {
                if sensitive.contains(source) || sensitive.contains(destination) {
                    sensitive.insert(*source);
                    sensitive.insert(*destination);
                }
            }
            PageOp::PresentationOverlay { destination, .. } => {
                sensitive.insert(*destination);
            }
            PageOp::Points { .. }
            | PageOp::Fill { .. }
            | PageOp::Color { .. }
            | PageOp::UploadLogical { .. }
            | PageOp::UploadLogicalRect { .. } => {}
        }
    }
    for operation in operations {
        match operation {
            PageOp::Copy { .. }
            | PageOp::TransformCopy { .. }
            | PageOp::Swap { .. }
            | PageOp::PresentationOverlay { .. }
            | PageOp::Points { .. }
            | PageOp::Fill { .. }
            | PageOp::Color { .. }
            | PageOp::UploadLogical { .. }
            | PageOp::UploadLogicalRect { .. } => {}
        }
    }
    sensitive
}
fn should_retain_page_for_presentation_replay(
    presentation_scale: u32,
    has_cached_page: bool,
    presentation_sensitive: bool,
) -> bool {
    presentation_scale > 1 && has_cached_page && presentation_sensitive
}
fn page_texture_scale(presentation_scale: u32, presentation_sensitive: bool) -> u32 {
    if presentation_sensitive { presentation_scale.max(1) } else { 1 }
}
fn operation_page_handles(operation: &PageOp) -> [Option<u32>; 2] {
    match operation {
        PageOp::Copy { source, source_is_presented_scene, destination, .. }
        | PageOp::TransformCopy {
            source,
            source_is_presented_scene,
            destination,
            ..
        } => [(!source_is_presented_scene).then_some(*source), Some(*destination)],
        PageOp::Swap { source, destination, .. } => [Some(*source), Some(*destination)],
        PageOp::Points { destination, .. }
        | PageOp::Fill { destination, .. }
        | PageOp::Color { destination, .. }
        | PageOp::PresentationOverlay { destination, .. }
        | PageOp::UploadLogical { destination, .. }
        | PageOp::UploadLogicalRect { destination, .. } => [Some(*destination), None],
    }
}
struct SharedPageImage {
    pixels: Weak<Vec<u8>>,
    image: Weak<GpuImage>,
    width: u32,
    height: u32,
}
fn keep_shared_page_image(cached_image_id: Option<u64>, mutated_image_id: u64) -> bool {
    cached_image_id.is_some_and(|cached_image_id| cached_image_id != mutated_image_id)
}
const PAGE_OP_SCRATCH_MAX_ENTRIES: usize = 8;
const PAGE_OP_SCRATCH_BUDGET_BYTES: u64 = 64 * 1024 * 1024;
const HD_PAGE_CACHE_SOFT_BUDGET_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const HD_PAGE_CACHE_HARD_BUDGET_BYTES: u64 = 3 * 1024 * 1024 * 1024;
const HD_PAGE_CACHE_GRACE_SUBMITS: u64 = 180;
fn inactive_page_is_evictable(resident_bytes: u64, age_submits: u64) -> bool {
    resident_bytes > HD_PAGE_CACHE_HARD_BUDGET_BYTES
        || age_submits >= HD_PAGE_CACHE_GRACE_SUBMITS
}
fn page_op_scratch_bytes((width, height): (u32, u32)) -> u64 {
    u64::from(width).saturating_mul(u64::from(height)).saturating_mul(4)
}
fn trim_page_op_scratch_cache<T>(
    cache: &mut HashMap<(u32, u32), T>,
    lru: &mut VecDeque<(u32, u32)>,
    requested: (u32, u32),
) {
    let requested_bytes = page_op_scratch_bytes(requested);
    let mut resident_bytes = cache
        .keys()
        .copied()
        .map(page_op_scratch_bytes)
        .sum::<u64>();
    while !cache.is_empty()
        && (cache.len() >= PAGE_OP_SCRATCH_MAX_ENTRIES
            || resident_bytes.saturating_add(requested_bytes)
                > PAGE_OP_SCRATCH_BUDGET_BYTES)
    {
        let Some(oldest) = lru.pop_front() else {
            cache.clear();
            break;
        };
        if cache.remove(&oldest).is_some() {
            resident_bytes = resident_bytes
                .saturating_sub(page_op_scratch_bytes(oldest));
        }
    }
}
pub struct Renderer {
    pub gpu: GpuContext,
    internal_w: u32,
    internal_h: u32,
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    quads: Vec<Quad>,
    movie_overlay: Option<MovieOverlay>,
    movie_cache: Option<(u64, MovieGpuFrame)>,
    display_image: Option<std::sync::Arc<GpuImage>>,
    display_epoch: u64,
    viewport_offset: (i32, i32),
    page_cache: HashMap<u32, CachedPage>,
    retained_page_handles: HashSet<u32>,
    evicted_page_image: Option<Arc<GpuImage>>,
    page_op_scratch: HashMap<(u32, u32), Arc<GpuImage>>,
    page_op_scratch_lru: VecDeque<(u32, u32)>,
    pending_page_ops: Vec<PageOp>,
    pending_marks: Vec<crate::render_model::FrameMark>,
    pending_released_pages: HashSet<u32>,
    pending_display_page: Option<Option<u32>>,
    shared_page_images: HashMap<usize, SharedPageImage>,
    seed_pool: HashMap<PageTextureIdentity, Arc<GpuImage>>,
    seed_pool_order: VecDeque<PageTextureIdentity>,
    seed_pool_bytes: u64,
    seed_pool_budget: Option<u64>,
    frame_bind_group: wgpu::BindGroup,
    linear_sampler: wgpu::Sampler,
    composition_texture: wgpu::Texture,
    composition_view: wgpu::TextureView,
    destination_snapshot: wgpu::Texture,
    destination_snapshot_view: wgpu::TextureView,
    presentation_scale: u32,
    profiler: Option<RenderProfiler>,
    page_dump_readbacks: Vec<PageDumpReadback>,
    runtime_cpu: RuntimeCpuTimings,
    last_submit_profile: SubmitProfile,
    next_submit_id: u64,
    next_frame_id: u64,
    next_gpu_image_id: Cell<u64>,
    profile_counters: Cell<ResourceCounters>,
}
fn prepare_surface_and_features(
    adapter: &wgpu::Adapter,
    surface: &wgpu::Surface<'static>,
    profile: Option<RenderProfileConfig>,
) -> (wgpu::TextureFormat, wgpu::Features, bool, bool, bool, wgpu::CompositeAlphaMode) {
    let caps = surface.get_capabilities(adapter);
    let surface_format = caps
        .formats
        .iter()
        .copied()
        .find(|f| !f.is_srgb())
        .unwrap_or(caps.formats[0]);
    let supported_features = adapter.features();
    let timestamp_query = profile.is_some()
        && supported_features.contains(wgpu::Features::TIMESTAMP_QUERY);
    let timestamp_inside_encoders = timestamp_query
        && supported_features.contains(wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS);
    let timestamp_inside_passes = timestamp_query
        && supported_features.contains(wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES);
    let mut required_features = wgpu::Features::empty();
    if timestamp_query {
        required_features |= wgpu::Features::TIMESTAMP_QUERY;
    }
    if timestamp_inside_encoders {
        required_features |= wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS;
    }
    if timestamp_inside_passes {
        required_features |= wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES;
    }
    let alpha_mode = caps.alpha_modes[0];
    (
        surface_format,
        required_features,
        timestamp_query,
        timestamp_inside_encoders,
        timestamp_inside_passes,
        alpha_mode,
    )
}
impl Renderer {
    pub fn new_with_presentation_and_profile(
        instance: &wgpu::Instance,
        surface: wgpu::Surface<'static>,
        size: winit::dpi::PhysicalSize<u32>,
        presentation: PresentationConfig,
        profile: Option<RenderProfileConfig>,
        internal_size: (u32, u32),
    ) -> Self {
        let adapter = pollster::block_on(
                instance
                    .request_adapter(
                        &wgpu::RequestAdapterOptions {
                            power_preference: wgpu::PowerPreference::HighPerformance,
                            compatible_surface: Some(&surface),
                            force_fallback_adapter: false,
                        },
                    ),
            )
            .expect("no suitable GPU adapter");
        Self::from_adapter(adapter, surface, size, presentation, profile, internal_size)
    }
    pub fn from_adapter(
        adapter: wgpu::Adapter,
        surface: wgpu::Surface<'static>,
        size: winit::dpi::PhysicalSize<u32>,
        presentation: PresentationConfig,
        profile: Option<RenderProfileConfig>,
        internal_size: (u32, u32),
    ) -> Self {
        let (
            surface_format,
            required_features,
            timestamp_query,
            timestamp_encoders,
            timestamp_passes,
            alpha_mode,
        ) = prepare_surface_and_features(&adapter, &surface, profile.clone());
        let adapter_info = adapter.get_info();
        let gpu = GpuContext::new_with_features(
            &adapter,
            surface_format,
            required_features,
        );
        Self::assemble(
            gpu,
            adapter_info,
            surface,
            surface_format,
            alpha_mode,
            (timestamp_query, timestamp_encoders, timestamp_passes),
            size,
            presentation,
            profile,
            internal_size,
        )
    }
    pub async fn from_adapter_async(
        adapter: wgpu::Adapter,
        surface: wgpu::Surface<'static>,
        size: winit::dpi::PhysicalSize<u32>,
        presentation: PresentationConfig,
        profile: Option<RenderProfileConfig>,
        internal_size: (u32, u32),
    ) -> Self {
        let (
            surface_format,
            required_features,
            timestamp_query,
            timestamp_encoders,
            timestamp_passes,
            alpha_mode,
        ) = prepare_surface_and_features(&adapter, &surface, profile.clone());
        let adapter_info = adapter.get_info();
        let gpu = GpuContext::request_with_features(
                &adapter,
                surface_format,
                required_features,
            )
            .await;
        Self::assemble(
            gpu,
            adapter_info,
            surface,
            surface_format,
            alpha_mode,
            (timestamp_query, timestamp_encoders, timestamp_passes),
            size,
            presentation,
            profile,
            internal_size,
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn assemble(
        gpu: GpuContext,
        adapter_info: wgpu::AdapterInfo,
        surface: wgpu::Surface<'static>,
        surface_format: wgpu::TextureFormat,
        alpha_mode: wgpu::CompositeAlphaMode,
        timestamps: (bool, bool, bool),
        size: winit::dpi::PhysicalSize<u32>,
        presentation: PresentationConfig,
        profile: Option<RenderProfileConfig>,
        internal_size: (u32, u32),
    ) -> Self {
        let (timestamp_query, timestamp_inside_encoders, timestamp_inside_passes) = timestamps;
        eprintln!(
            "[GPU] adapter={} backend={:?} max_texture_dimension_2d={}", adapter_info
            .name, adapter_info.backend, gpu.max_texture_dim
        );
        let (w, h) = clamp_surface_size(size.width, size.height, gpu.max_texture_dim);
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: w,
            height: h,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&gpu.device, &surface_config);
        let frame_bind_group = gpu
            .make_frame_bind_group(internal_size.0, internal_size.1);
        let linear_sampler = gpu
            .device
            .create_sampler(
                &wgpu::SamplerDescriptor {
                    label: Some("output linear sampler"),
                    address_mode_u: wgpu::AddressMode::ClampToEdge,
                    address_mode_v: wgpu::AddressMode::ClampToEdge,
                    address_mode_w: wgpu::AddressMode::ClampToEdge,
                    mag_filter: wgpu::FilterMode::Linear,
                    min_filter: wgpu::FilterMode::Linear,
                    ..Default::default()
                },
            );
        let presentation_scale = presentation.scale.max(1);
        let (composition_texture, composition_view) = create_composition_texture(
            &gpu.device,
            "frame composition",
            presentation_scale,
            internal_size.0,
            internal_size.1,
        );
        let (destination_snapshot, destination_snapshot_view) = create_composition_texture(
            &gpu.device,
            "draw mode destination snapshot",
            presentation_scale,
            internal_size.0,
            internal_size.1,
        );
        let profiler = profile
            .map(|config| {
                RenderProfiler::new(
                        config,
                        &gpu.device,
                        &gpu.queue,
                        &adapter_info,
                        presentation_scale,
                        w,
                        h,
                        timestamp_query,
                        timestamp_inside_encoders,
                        timestamp_inside_passes,
                    )
                    .unwrap_or_else(|error| {
                        panic!("render profiler initialization failed: {error}")
                    })
            });
        Self {
            gpu,
            internal_w: internal_size.0,
            internal_h: internal_size.1,
            surface,
            surface_config,
            quads: Vec::new(),
            movie_overlay: None,
            movie_cache: None,
            display_image: None,
            display_epoch: 0,
            viewport_offset: (0, 0),
            page_cache: HashMap::new(),
            retained_page_handles: HashSet::new(),
            evicted_page_image: None,
            page_op_scratch: HashMap::new(),
            page_op_scratch_lru: VecDeque::new(),
            pending_page_ops: Vec::new(),
            pending_released_pages: HashSet::new(),
            pending_display_page: None,
            shared_page_images: HashMap::new(),
            seed_pool: HashMap::new(),
            seed_pool_order: VecDeque::new(),
            seed_pool_bytes: 0,
            seed_pool_budget: Some(seed_pool_budget_bytes()),
            frame_bind_group,
            linear_sampler,
            composition_texture,
            composition_view,
            destination_snapshot,
            destination_snapshot_view,
            presentation_scale,
            profiler,
            page_dump_readbacks: Vec::new(),
            runtime_cpu: RuntimeCpuTimings::default(),
            last_submit_profile: SubmitProfile::default(),
            pending_marks: Vec::new(),
            next_submit_id: 1,
            next_frame_id: 1,
            next_gpu_image_id: Cell::new(1),
            profile_counters: Cell::new(ResourceCounters::default()),
        }
    }
    pub fn resize(&mut self, size: winit::dpi::PhysicalSize<u32>) {
        let (w, h) = clamp_surface_size(
            size.width,
            size.height,
            self.gpu.max_texture_dim,
        );
        self.surface_config.width = w;
        self.surface_config.height = h;
        self.surface.configure(&self.gpu.device, &self.surface_config);
    }
    pub fn record_runtime_cpu(&mut self, timings: RuntimeCpuTimings) {
        if self.profiler.is_some() {
            self.runtime_cpu = timings;
        }
    }
    fn update_profile_counters(&self, update: impl FnOnce(&mut ResourceCounters)) {
        if self.profiler.is_none() {
            return;
        }
        let mut counters = self.profile_counters.get();
        update(&mut counters);
        self.profile_counters.set(counters);
    }
    fn alloc_gpu_image_id(&self) -> u64 {
        let id = self.next_gpu_image_id.get();
        self.next_gpu_image_id.set(id.wrapping_add(1).max(1));
        id
    }
    fn upload_rgba(
        &self,
        pixels: &[u8],
        width: u32,
        height: u32,
    ) -> std::sync::Arc<GpuImage> {
        self.upload_pixels(
            pixels,
            width,
            height,
            wgpu::TextureFormat::Rgba8Unorm,
            "majiro image",
        )
    }
    fn upload_rgba_nearest_scaled(
        &self,
        pixels: &[u8],
        width: u32,
        height: u32,
        scale: u32,
    ) -> Arc<GpuImage> {
        self.upload_rgba_nearest_scaled_profiled(
            pixels,
            width,
            height,
            scale,
            None,
            None,
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn upload_rgba_nearest_scaled_profiled(
        &self,
        pixels: &[u8],
        width: u32,
        height: u32,
        scale: u32,
        gpu_profile: Option<&mut ActiveGpuFrame>,
        profile_tag: Option<GpuSpanTag>,
    ) -> Arc<GpuImage> {
        if scale <= 1 {
            return self.upload_rgba(pixels, width, height);
        }
        let source = self.upload_rgba(pixels, width, height);
        let destination = self
            .empty_rgba_image(
                width.saturating_mul(scale),
                height.saturating_mul(scale),
                "GPU-upscaled Majiro image",
            );
        let vertices = page_op_vertices(
            [0, 0, width as i32, height as i32],
            [0, 0, width as i32, height as i32],
            width,
            height,
        );
        let (vertex_buffer, bind_group) = self
            .make_draw_resources(
                &vertices,
                &source.view,
                &source.view,
                &self.gpu.sampler,
                QuadUniform {
                    native_transparency: 0,
                    source_has_alpha: 1,
                    draw_mode: 18,
                    _padding: 0,
                    color_a: 0,
                    color_b: 0,
                    parameter: 0,
                    _padding_b: 0,
                },
            );
        let page_bind_group = self.gpu.make_logical_bind_group(width, height);
        let mut encoder = self
            .gpu
            .device
            .create_command_encoder(
                &wgpu::CommandEncoderDescriptor {
                    label: Some("GPU nearest-neighbour upscale encoder"),
                },
            );
        {
            let span = profile_tag.and_then(|tag| gpu_profile?.pass_span(tag));
            let timestamp_writes = span.as_ref().map(|span| span.pass_writes());
            self.update_profile_counters(|counters| counters.render_passes += 1);
            let mut pass = encoder
                .begin_render_pass(
                    &wgpu::RenderPassDescriptor {
                        label: Some("GPU nearest-neighbour upscale"),
                        color_attachments: &[
                            Some(wgpu::RenderPassColorAttachment {
                                view: &destination.view,
                                resolve_target: None,
                                depth_slice: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                                    store: wgpu::StoreOp::Store,
                                },
                            }),
                        ],
                        depth_stencil_attachment: None,
                        timestamp_writes,
                        occlusion_query_set: None,
                    },
                );
            pass.set_pipeline(&self.gpu.page_op_pipeline);
            pass.set_bind_group(0, &page_bind_group, &[]);
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            pass.set_bind_group(1, &bind_group, &[]);
            pass.draw(0..6, 0..1);
        }
        self.gpu.queue.submit(Some(encoder.finish()));
        destination
    }
    fn upload_bgra(
        &self,
        pixels: &[u8],
        width: u32,
        height: u32,
    ) -> std::sync::Arc<GpuImage> {
        self.upload_pixels(
            pixels,
            width,
            height,
            wgpu::TextureFormat::Bgra8Unorm,
            "movie frame",
        )
    }
    fn upload_movie_frame(&self, movie: &RenderMovieFrame) -> Option<MovieGpuFrame> {
        match &movie.pixels {
            RenderMoviePixels::Bgra8(pixels) => {
                let expected = movie.width as usize * movie.height as usize * 4;
                if pixels.len() != expected {
                    eprintln!(
                        "[VIDEO] rejected BGRA frame revision {}: {} bytes for {}x{}",
                        movie.revision, pixels.len(), movie.width, movie.height
                    );
                    return None;
                }
                Some(
                    MovieGpuFrame::Bgra8(
                        self.upload_bgra(pixels, movie.width, movie.height),
                    ),
                )
            }
            RenderMoviePixels::Nv12 { luma, chroma } => {
                if movie.width == 0 || movie.height == 0
                    || !movie.width.is_multiple_of(2) || !movie.height.is_multiple_of(2)
                {
                    eprintln!(
                        "[VIDEO] rejected NV12 frame revision {}: invalid dimensions {}x{}",
                        movie.revision, movie.width, movie.height
                    );
                    return None;
                }
                let expected_luma = movie.width as usize * movie.height as usize;
                let expected_chroma = expected_luma / 2;
                if luma.len() != expected_luma || chroma.len() != expected_chroma {
                    eprintln!(
                        "[VIDEO] rejected NV12 frame revision {}: Y={} UV={} bytes for {}x{}",
                        movie.revision, luma.len(), chroma.len(), movie.width, movie
                        .height
                    );
                    return None;
                }
                let luma = self
                    .upload_movie_plane(
                        luma,
                        movie.width,
                        movie.height,
                        wgpu::TextureFormat::R8Unorm,
                        1,
                        "movie NV12 luma",
                    );
                let chroma = self
                    .upload_movie_plane(
                        chroma,
                        movie.width / 2,
                        movie.height / 2,
                        wgpu::TextureFormat::Rg8Unorm,
                        2,
                        "movie NV12 chroma",
                    );
                self.update_profile_counters(|counters| {
                    counters.bind_groups_created += 1;
                });
                let bind_group = Arc::new(
                    self
                        .gpu
                        .device
                        .create_bind_group(
                            &wgpu::BindGroupDescriptor {
                                label: Some("NV12 movie bind group"),
                                layout: &self.gpu.movie_texture_bind_group_layout,
                                entries: &[
                                    wgpu::BindGroupEntry {
                                        binding: 0,
                                        resource: wgpu::BindingResource::TextureView(&luma.view),
                                    },
                                    wgpu::BindGroupEntry {
                                        binding: 1,
                                        resource: wgpu::BindingResource::TextureView(&chroma.view),
                                    },
                                    wgpu::BindGroupEntry {
                                        binding: 2,
                                        resource: wgpu::BindingResource::Sampler(
                                            &self.linear_sampler,
                                        ),
                                    },
                                ],
                            },
                        ),
                );
                Some(MovieGpuFrame::Nv12 {
                    luma,
                    chroma,
                    bind_group,
                })
            }
        }
    }
    fn upload_movie_plane(
        &self,
        pixels: &[u8],
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        bytes_per_texel: u32,
        label: &'static str,
    ) -> Arc<GpuImage> {
        let unpadded_row = width * bytes_per_texel;
        let byte_size = u64::from(unpadded_row) * u64::from(height);
        self.update_profile_counters(|counters| {
            counters.textures_created += 1;
            counters.texture_bytes_created += byte_size;
            counters.texture_upload_bytes += byte_size;
        });
        let texture = self
            .gpu
            .device
            .create_texture(
                &wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING
                        | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                },
            );
        let padded_row = padded_row_bytes(unpadded_row);
        if unpadded_row == padded_row {
            self.gpu
                .queue
                .write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    pixels,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(unpadded_row),
                        rows_per_image: Some(height),
                    },
                    wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                );
        } else {
            let mut staged = vec![0u8; (padded_row * height) as usize];
            for row in 0..height as usize {
                let source = row * unpadded_row as usize;
                let destination = row * padded_row as usize;
                staged[destination..destination + unpadded_row as usize]
                    .copy_from_slice(&pixels[source..source + unpadded_row as usize]);
            }
            self.gpu
                .queue
                .write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &staged,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(padded_row),
                        rows_per_image: Some(height),
                    },
                    wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                );
        }
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Arc::new(GpuImage {
            id: self.alloc_gpu_image_id(),
            category: label,
            texture,
            view,
            width,
            height,
            byte_size,
        })
    }
    fn upload_pixels(
        &self,
        pixels: &[u8],
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        label: &'static str,
    ) -> std::sync::Arc<GpuImage> {
        let byte_size = u64::from(width) * u64::from(height) * 4;
        self.update_profile_counters(|counters| {
            counters.textures_created += 1;
            counters.texture_bytes_created += byte_size;
            counters.texture_upload_bytes += byte_size;
        });
        let texture = self
            .gpu
            .device
            .create_texture(
                &wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING
                        | wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST
                        | wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                },
            );
        let unpadded_row = width * 4;
        let padded_row = padded_row_bytes(unpadded_row);
        if unpadded_row == padded_row {
            self.gpu
                .queue
                .write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    pixels,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(padded_row),
                        rows_per_image: Some(height),
                    },
                    wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                );
        } else {
            let mut staged = vec![0u8; (padded_row * height) as usize];
            for y in 0..height as usize {
                let src = y * unpadded_row as usize;
                let dst = y * padded_row as usize;
                staged[dst..dst + unpadded_row as usize]
                    .copy_from_slice(&pixels[src..src + unpadded_row as usize]);
            }
            self.gpu
                .queue
                .write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &staged,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(padded_row),
                        rows_per_image: Some(height),
                    },
                    wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                );
        }
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        std::sync::Arc::new(GpuImage {
            id: self.alloc_gpu_image_id(),
            category: label,
            texture,
            view,
            width,
            height,
            byte_size,
        })
    }
    pub fn submit_frame(&mut self, frame: RenderFrame) {
        let profile_enabled = self.profiler.is_some();
        let total_started = profile_enabled.then(Instant::now);
        let submit_id = self.next_submit_id;
        self.next_submit_id = self.next_submit_id.wrapping_add(1).max(1);
        let RenderFrame {
            mut pages,
            rehydrate_pages,
            page_ops,
            released_pages,
            retained_pages,
            display_page,
            display_epoch,
            quads,
            movie,
            viewport_offset,
            marks,
        } = frame;
        self.pending_marks = marks;
        let dirty_page_count = pages.len();
        pages.extend(rehydrate_pages);
        let page_op_count = page_ops.len();
        let released_page_count = released_pages.len();
        let quad_count = quads.len();
        self.viewport_offset = viewport_offset;
        self.retained_page_handles = retained_pages.into_iter().collect();
        self.retained_page_handles.extend(quads.iter().map(|quad| quad.page));
        self.retained_page_handles.extend(display_page);
        let sensitivity_started = profile_enabled.then(Instant::now);
        let sensitive_pages = propagate_presentation_sensitivity(
            self
                .page_cache
                .iter()
                .filter_map(|(&handle, cached)| {
                    cached.presentation_sensitive.then_some(handle)
                })
                .collect(),
            pages
                .iter()
                .filter_map(|page| page.presentation.is_some().then_some(page.handle)),
            &page_ops,
        );
        let sensitivity_us = sensitivity_started
            .map(|started| started.elapsed().as_micros())
            .unwrap_or(0);
        let pages_started = profile_enabled.then(Instant::now);
        for page in pages {
            let expected_len = page.width as usize * page.height as usize * 4;
            if page.pixels.len() != expected_len {
                eprintln!(
                    "[GFX] rejected page {} revision {}: {} bytes for {}x{}", page
                    .handle, page.revision, page.pixels.len(), page.width, page.height
                );
                continue;
            }
            let presentation_sensitive = sensitive_pages.contains(&page.handle);
            let existing_revision = self
                .page_cache
                .get(&page.handle)
                .map(|cached| cached.revision);
            if existing_revision == Some(page.revision) {
                if let Some(cached) = self.page_cache.get_mut(&page.handle) {
                    cached.last_used_submit = submit_id;
                    if presentation_sensitive {
                        cached.presentation_sensitive = true;
                    }
                    if page.debug_name.is_some() {
                        cached.debug_name = page.debug_name.clone();
                    }
                    let required_scale = page_texture_scale(
                        self.presentation_scale,
                        cached.presentation_sensitive,
                    );
                    if cached.image.width == page.width.saturating_mul(required_scale)
                        && cached.image.height
                            == page.height.saturating_mul(required_scale)
                    {
                        continue;
                    }
                }
            }
            let selected = if presentation_sensitive {
                if let Some(image) = page.presentation.as_ref() {
                    (
                        image.pixels.clone(),
                        image.width,
                        image.height,
                        image.width,
                        image.height,
                        false,
                    )
                } else if self.presentation_scale > 1 {
                    (
                        page.presentation_seed.clone(),
                        page.width,
                        page.height,
                        page.width * self.presentation_scale,
                        page.height * self.presentation_scale,
                        true,
                    )
                } else {
                    (
                        page.pixels.clone(),
                        page.width,
                        page.height,
                        page.width,
                        page.height,
                        false,
                    )
                }
            } else {
                (
                    page.pixels.clone(),
                    page.width,
                    page.height,
                    page.width,
                    page.height,
                    false,
                )
            };
            let pixel_key = Arc::as_ptr(&selected.0) as usize;
            let shared_image = existing_revision
                .is_none()
                .then(|| {
                    self.shared_page_images
                        .get(&pixel_key)
                        .and_then(|shared| {
                            if shared.width != selected.3 || shared.height != selected.4
                            {
                                return None;
                            }
                            let shared_pixels = shared.pixels.upgrade()?;
                            if !Arc::ptr_eq(&shared_pixels, &selected.0) {
                                return None;
                            }
                            shared.image.upgrade()
                        })
                });
            let shared_image = shared_image.flatten();
            let retained = should_retain_page_for_presentation_replay(
                    self.presentation_scale,
                    existing_revision.is_some(),
                    presentation_sensitive,
                )
                .then(|| self.page_cache.get(&page.handle))
                .flatten()
                .map(|cached| {
                    (cached.image.clone(), cached.shared, cached.logical_pixels.clone())
                });
            let retained_presentation = retained
                .and_then(|(image, shared, logical_pixels)| {
                    if image.width == selected.3 && image.height == selected.4 {
                        Some((image, shared))
                    } else if selected.5 && image.width == page.width
                        && image.height == page.height
                    {
                        Some((
                            self
                                .upload_rgba_nearest_scaled(
                                    &logical_pixels,
                                    page.width,
                                    page.height,
                                    self.presentation_scale,
                                ),
                            false,
                        ))
                    } else {
                        None
                    }
                });
            let retained_shared = retained_presentation
                .as_ref()
                .is_some_and(|value| value.1);
            let retained_image = retained_presentation.map(|value| value.0);
            let was_shared = shared_image.is_some();
            if let Some(shared) = shared_image.as_ref() {
                for cached in self.page_cache.values_mut() {
                    if Arc::ptr_eq(&cached.image, shared) {
                        cached.shared = true;
                    }
                }
            }
            if page_dump_dir().is_some() && page.width >= 256 && page.height >= 256 {
                let revision_tag = if existing_revision.is_none() {
                    "new"
                } else {
                    "upd"
                };
                let fullscreen = page.width >= 1000 && page.height >= 560;
                if fullscreen {
                    dump_page_debug_gated(
                        &format!("gpu_seed_{revision_tag}"),
                        page.handle,
                        page.revision,
                        selected.3,
                        selected.4,
                        &selected.0,
                        true,
                    );
                    dump_page_debug_gated(
                        "cpu_authority",
                        page.handle,
                        page.revision,
                        page.width,
                        page.height,
                        &page.pixels,
                        true,
                    );
                }
            }
            let seed_identity = (existing_revision.is_none())
                .then(|| {
                    PageTextureIdentity::Content(
                        hash_rgba_pixels(&selected.0),
                        selected.3,
                        selected.4,
                    )
                });
            let image = if let Some(image) = retained_image.or(shared_image) {
                image
            } else if let Some(pooled) = seed_identity
                .as_ref()
                .and_then(|identity| self.pooled_seed(identity))
            {
                pooled
            } else {
                let image = if selected.5 {
                    self.upload_rgba_nearest_scaled(
                        &selected.0,
                        selected.1,
                        selected.2,
                        self.presentation_scale,
                    )
                } else {
                    self.upload_rgba(&selected.0, selected.1, selected.2)
                };
                self.shared_page_images
                    .insert(
                        pixel_key,
                        SharedPageImage {
                            pixels: Arc::downgrade(&selected.0),
                            image: Arc::downgrade(&image),
                            width: selected.3,
                            height: selected.4,
                        },
                    );
                if let Some(identity) = seed_identity {
                    self.pool_released_seed(identity, Arc::clone(&image));
                }
                image
            };
            self.page_cache
                .insert(
                    page.handle,
                    CachedPage {
                        revision: page.revision,
                        image,
                        last_used_submit: submit_id,
                        logical_pixels: page.pixels,
                        logical_width: page.width,
                        logical_height: page.height,
                        debug_name: page.debug_name,
                        shared: retained_shared || was_shared || seed_identity.is_some(),
                        presentation_sensitive,
                        seed_identity: if retained_shared || was_shared {
                            None
                        } else {
                            seed_identity
                        },
                    },
                );
        }
        let pages_us = pages_started
            .map(|started| started.elapsed().as_micros())
            .unwrap_or(0);
        for handle in page_ops.iter().flat_map(operation_page_handles).flatten() {
            self.retained_page_handles.insert(handle);
            if let Some(cached) = self.page_cache.get_mut(&handle) {
                cached.last_used_submit = submit_id;
            }
        }
        if self.presentation_scale > 1 {
            self.pending_page_ops.extend(page_ops);
        }
        self.pending_released_pages.extend(released_pages);
        if display_epoch != self.display_epoch {
            self.display_epoch = display_epoch;
            self.pending_display_page = Some(display_page);
        }
        let quad_build_started = profile_enabled.then(Instant::now);
        self.quads = quads
            .into_iter()
            .filter_map(|quad| {
                let cached = match self.page_cache.get(&quad.page) {
                    Some(cached) => cached,
                    None => {
                        eprintln!(
                            "[QUAD-DROP] page={} not in renderer cache (sprite page vanished); quad {:?}",
                            quad.page, (quad.x, quad.y, quad.width, quad.height, quad
                            .alpha)
                        );
                        return None;
                    }
                };
                let image = cached.image.clone();
                let image_width = cached.logical_width as f32;
                let image_height = cached.logical_height as f32;
                Some(Quad {
                    page: Some(quad.page),
                    image,
                    x: quad.x - viewport_offset.0 as f32,
                    y: quad.y - viewport_offset.1 as f32,
                    w: quad.width,
                    h: quad.height,
                    u0: quad.source_x / image_width,
                    v0: quad.source_y / image_height,
                    u1: (quad.source_x + quad.source_width) / image_width,
                    v1: (quad.source_y + quad.source_height) / image_height,
                    alpha: quad.alpha,
                    rotation_degrees: quad.rotation_degrees,
                    scale_x: quad.scale_x,
                    scale_y: quad.scale_y,
                    source_has_alpha: quad.source_has_alpha,
                    draw_mode: quad.draw_mode,
                })
            })
            .collect();
        if let Some(movie) = movie {
            let frame = if let Some((revision, frame)) = &self.movie_cache {
                if *revision == movie.revision {
                    Some(frame.clone())
                } else {
                    self.upload_movie_frame(&movie)
                }
            } else {
                self.upload_movie_frame(&movie)
            };
            if let Some(frame) = frame {
                self.movie_cache = Some((movie.revision, frame.clone()));
                self.movie_overlay = Some(MovieOverlay {
                    frame,
                    x: movie.x,
                    y: movie.y,
                    w: movie.output_width,
                    h: movie.output_height,
                });
            } else {
                self.movie_overlay = None;
                self.movie_cache = None;
            }
        } else {
            self.movie_overlay = None;
            self.movie_cache = None;
        }
        let quad_build_us = quad_build_started
            .map(|started| started.elapsed().as_micros())
            .unwrap_or(0);
        if let Some(total_started) = total_started {
            self.last_submit_profile = SubmitProfile {
                submit_id,
                total_us: total_started.elapsed().as_micros(),
                sensitivity_us,
                pages_us,
                quad_build_us,
                dirty_pages: dirty_page_count,
                released_pages: released_page_count,
                page_ops: page_op_count,
                quads: quad_count,
            };
        }
    }
    fn texture_memory_snapshot(&self) -> TextureMemorySnapshot {
        fn add_image(
            textures: &mut HashMap<u64, TextureMemoryEntry>,
            image: &GpuImage,
            alias: String,
        ) {
            let entry = textures
                .entry(image.id)
                .or_insert_with(|| TextureMemoryEntry {
                    id: image.id,
                    category: image.category.to_owned(),
                    width: image.width,
                    height: image.height,
                    bytes: image.byte_size,
                    aliases: Vec::new(),
                });
            if !entry.aliases.contains(&alias) {
                entry.aliases.push(alias);
            }
        }
        let mut textures = HashMap::new();
        let mut shared_page_cache_entries = 0;
        for (&handle, cached) in &self.page_cache {
            shared_page_cache_entries += usize::from(cached.shared);
            let alias = cached
                .debug_name
                .as_deref()
                .map(|name| format!("page:{handle}:{name}"))
                .unwrap_or_else(|| format!("page:{handle}"));
            add_image(&mut textures, &cached.image, alias);
        }
        for (&(width, height), image) in &self.page_op_scratch {
            add_image(&mut textures, image, format!("page_op_scratch:{width}x{height}"));
        }
        for (identity, image) in &self.seed_pool {
            add_image(&mut textures, image, format!("seed_pool:{identity:?}"));
        }
        if let Some(image) = &self.display_image {
            add_image(&mut textures, image, "display_snapshot".to_owned());
        }
        if let Some((revision, frame)) = &self.movie_cache {
            match frame {
                MovieGpuFrame::Bgra8(image) => {
                    add_image(
                        &mut textures,
                        image,
                        format!("movie_frame_revision:{revision}"),
                    )
                }
                MovieGpuFrame::Nv12 { luma, chroma, .. } => {
                    add_image(
                        &mut textures,
                        luma,
                        format!("movie_frame_revision:{revision}:luma"),
                    );
                    add_image(
                        &mut textures,
                        chroma,
                        format!("movie_frame_revision:{revision}:chroma"),
                    );
                }
            }
        }
        for quad in &self.quads {
            if let Some(page) = quad.page {
                add_image(&mut textures, &quad.image, format!("quad_page:{page}"));
            }
        }
        if let Some(movie) = &self.movie_overlay {
            match &movie.frame {
                MovieGpuFrame::Bgra8(image) => {
                    add_image(&mut textures, image, "movie_overlay".to_owned())
                }
                MovieGpuFrame::Nv12 { luma, chroma, .. } => {
                    add_image(&mut textures, luma, "movie_overlay:luma".to_owned());
                    add_image(&mut textures, chroma, "movie_overlay:chroma".to_owned());
                }
            }
        }
        let texture_bytes = textures.values().map(|entry| entry.bytes).sum::<u64>();
        let scaled_width = u64::from(self.internal_w)
            * u64::from(self.presentation_scale);
        let scaled_height = u64::from(self.internal_h)
            * u64::from(self.presentation_scale);
        let fixed_target_bytes = scaled_width * scaled_height * 4 * 2;
        let surface_estimated_bytes = u64::from(self.surface_config.width)
            * u64::from(self.surface_config.height) * 4;
        let unique_textures = textures.len();
        let mut textures = textures.into_values().collect::<Vec<_>>();
        textures
            .sort_by(|left, right| {
                right.bytes.cmp(&left.bytes).then_with(|| left.id.cmp(&right.id))
            });
        textures.truncate(32);
        TextureMemorySnapshot {
            resident_bytes: texture_bytes + fixed_target_bytes,
            fixed_target_bytes,
            surface_estimated_bytes,
            unique_textures,
            page_cache_entries: self.page_cache.len(),
            shared_page_cache_entries,
            textures,
        }
    }
    fn page_debug_name(&self, handle: u32) -> Option<String> {
        self.page_cache
            .get(&handle)
            .and_then(|page| page.debug_name.as_deref())
            .map(str::to_owned)
    }
    fn page_op_gpu_tag(&self, operation: &PageOp) -> GpuSpanTag {
        let (name, page, pixels) = match operation {
            PageOp::Points { destination, points } => {
                ("page_op.points", *destination, points.len() as u64)
            }
            PageOp::Fill { destination, rect, .. } => {
                ("page_op.fill", *destination, rect_pixels(*rect))
            }
            PageOp::Copy { destination, destination_rect, .. } => {
                ("page_op.copy", *destination, rect_pixels(*destination_rect))
            }
            PageOp::TransformCopy { destination, destination_rect, .. } => {
                ("page_op.transform_copy", *destination, rect_pixels(*destination_rect))
            }
            PageOp::Swap { destination, source_rect, .. } => {
                ("page_op.swap", *destination, rect_pixels(*source_rect) * 2)
            }
            PageOp::Color { destination, rect, .. } => {
                ("page_op.color", *destination, rect_pixels(*rect))
            }
            PageOp::PresentationOverlay { destination, width, height, .. } => {
                (
                    "page_op.presentation_overlay",
                    *destination,
                    u64::from(*width) * u64::from(*height),
                )
            }
            PageOp::UploadLogical { destination, width, height, .. } => {
                (
                    "page_op.upload_logical",
                    *destination,
                    u64::from(*width) * u64::from(*height),
                )
            }
            PageOp::UploadLogicalRect { destination, rect, .. } => {
                ("page_op.upload_logical_rect", *destination, rect_pixels(*rect))
            }
        };
        GpuSpanTag {
            name: name.to_owned(),
            page: Some(page),
            resource: self.page_debug_name(page),
            draw_mode: None,
            pixels: Some(pixels),
            estimated_bytes: Some(pixels.saturating_mul(8)),
        }
    }
    fn quad_gpu_tag(&self, quad: &Quad, name: &str) -> GpuSpanTag {
        let logical_pixels = (quad.w.max(0.0) * quad.h.max(0.0)).round() as u64;
        let scale = u64::from(self.presentation_scale);
        GpuSpanTag {
            name: name.to_owned(),
            page: quad.page,
            resource: quad
                .page
                .and_then(|page| self.page_debug_name(page))
                .or_else(|| {
                    (quad.page.is_none()).then(|| "display_or_movie".to_owned())
                }),
            draw_mode: Some(quad.draw_mode),
            pixels: Some(logical_pixels.saturating_mul(scale).saturating_mul(scale)),
            estimated_bytes: None,
        }
    }
    fn begin_composition_pass<'pass>(
        &'pass self,
        encoder: &'pass mut wgpu::CommandEncoder,
        label: &str,
        load: wgpu::LoadOp<wgpu::Color>,
    ) -> wgpu::RenderPass<'pass> {
        self.update_profile_counters(|counters| counters.render_passes += 1);
        encoder
            .begin_render_pass(
                &wgpu::RenderPassDescriptor {
                    label: Some(label),
                    color_attachments: &[
                        Some(wgpu::RenderPassColorAttachment {
                            view: &self.composition_view,
                            resolve_target: None,
                            depth_slice: None,
                            ops: wgpu::Operations {
                                load,
                                store: wgpu::StoreOp::Store,
                            },
                        }),
                    ],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                },
            )
    }
    fn clear_composition(&self, encoder: &mut wgpu::CommandEncoder, label: &str) {
        let _pass = self
            .begin_composition_pass(
                encoder,
                label,
                wgpu::LoadOp::Clear(wgpu::Color::BLACK),
            );
    }
    fn make_draw_resources(
        &self,
        vertices: &[Vertex],
        source_view: &wgpu::TextureView,
        destination_view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
        uniform: QuadUniform,
    ) -> (wgpu::Buffer, wgpu::BindGroup) {
        self.update_profile_counters(|counters| {
            counters.buffers_created += 2;
            counters.bind_groups_created += 1;
            counters.draw_calls += 1;
        });
        let vertex_buffer = self
            .gpu
            .device
            .create_buffer_init(
                &wgpu::util::BufferInitDescriptor {
                    label: Some("quad verts"),
                    contents: bytemuck::cast_slice(vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                },
            );
        let uniform_buffer = self
            .gpu
            .device
            .create_buffer_init(
                &wgpu::util::BufferInitDescriptor {
                    label: Some("quad uniform"),
                    contents: bytemuck::bytes_of(&uniform),
                    usage: wgpu::BufferUsages::UNIFORM,
                },
            );
        let bind_group = self
            .gpu
            .device
            .create_bind_group(
                &wgpu::BindGroupDescriptor {
                    label: Some("quad bind group"),
                    layout: &self.gpu.texture_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(source_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: uniform_buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::TextureView(
                                destination_view,
                            ),
                        },
                    ],
                },
            );
        (vertex_buffer, bind_group)
    }
    fn empty_rgba_image(
        &self,
        width: u32,
        height: u32,
        label: &'static str,
    ) -> Arc<GpuImage> {
        let byte_size = u64::from(width) * u64::from(height) * 4;
        self.update_profile_counters(|counters| {
            counters.textures_created += 1;
            counters.texture_bytes_created += byte_size;
        });
        let texture = self
            .gpu
            .device
            .create_texture(
                &wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING
                        | wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST
                        | wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                },
            );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Arc::new(GpuImage {
            id: self.alloc_gpu_image_id(),
            category: label,
            texture,
            view,
            width,
            height,
            byte_size,
        })
    }
    fn copy_image(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        source: &GpuImage,
        destination: &GpuImage,
    ) {
        debug_assert_eq!(
            (source.width, source.height), (destination.width, destination.height)
        );
        self.update_profile_counters(|counters| {
            counters.texture_copy_bytes
                += u64::from(source.width) * u64::from(source.height) * 4;
        });
        encoder
            .copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &source.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &destination.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width: source.width,
                    height: source.height,
                    depth_or_array_layers: 1,
                },
            );
    }
    fn materialize_page(
        &mut self,
        handle: u32,
        encoder: &mut wgpu::CommandEncoder,
    ) -> bool {
        let Some(cached) = self.page_cache.get(&handle) else {
            return false;
        };
        if !cached.shared {
            return true;
        }
        let old = cached.image.clone();
        let materialized = self
            .empty_rgba_image(old.width, old.height, "materialized presentation page");
        self.copy_image(encoder, &old, &materialized);
        let cached = self.page_cache.get_mut(&handle).expect("page checked above");
        cached.image = materialized;
        cached.shared = false;
        cached.seed_identity = None;
        true
    }
    fn invalidate_shared_page_image(&mut self, image: &Arc<GpuImage>) {
        self.shared_page_images
            .retain(|_, shared| {
                keep_shared_page_image(
                    shared.image.upgrade().map(|cached| cached.id),
                    image.id,
                )
            });
    }
    fn operation_scratch(&mut self, width: u32, height: u32) -> Arc<GpuImage> {
        let key = (width, height);
        if let Some(image) = self.page_op_scratch.get(&key).cloned() {
            self.page_op_scratch_lru.retain(|cached| *cached != key);
            self.page_op_scratch_lru.push_back(key);
            return image;
        }
        trim_page_op_scratch_cache(
            &mut self.page_op_scratch,
            &mut self.page_op_scratch_lru,
            key,
        );
        let image = self
            .empty_rgba_image(width, height, "reusable presentation PageOp snapshot");
        self.page_op_scratch.insert(key, image.clone());
        self.page_op_scratch_lru.push_back(key);
        image
    }
    fn pool_released_seed(
        &mut self,
        identity: PageTextureIdentity,
        image: Arc<GpuImage>,
    ) {
        if self.seed_pool_budget.is_none() {
            return;
        }
        if self.seed_pool.contains_key(&identity) {
            return;
        }
        let bytes = gpu_image_bytes(&image);
        let budget = self.seed_pool_budget.expect("checked above");
        while self.seed_pool_bytes + bytes > budget {
            let Some(oldest) = self.seed_pool_order.pop_front() else {
                break;
            };
            if let Some(evicted) = self.seed_pool.remove(&oldest) {
                self.seed_pool_bytes = self
                    .seed_pool_bytes
                    .saturating_sub(gpu_image_bytes(&evicted));
            }
        }
        self.seed_pool_bytes += bytes;
        self.seed_pool.insert(identity, image);
        self.seed_pool_order.push_back(identity);
    }
    fn pooled_seed(&mut self, identity: &PageTextureIdentity) -> Option<Arc<GpuImage>> {
        let image = self.seed_pool.get(identity).cloned();
        if image.is_some() {
            let key = *identity;
            self.seed_pool_order.retain(|pooled| pooled != &key);
            self.seed_pool_order.push_back(key);
        }
        image
    }
    fn trim_inactive_page_textures(&mut self) {
        if self.presentation_scale <= 1 {
            return;
        }
        let placeholder = if let Some(image) = self.evicted_page_image.clone() {
            image
        } else {
            let image = self.upload_rgba(&[0, 0, 0, 0], 1, 1);
            self.evicted_page_image = Some(image.clone());
            image
        };
        let placeholder_id = placeholder.id;
        let retained_image_ids = self
            .retained_page_handles
            .iter()
            .filter_map(|handle| self.page_cache.get(handle).map(|page| page.image.id))
            .collect::<HashSet<_>>();
        let mut image_aliases = HashMap::<u64, usize>::new();
        let mut image_bytes = HashMap::<u64, u64>::new();
        for page in self.page_cache.values() {
            if page.image.id == placeholder_id {
                continue;
            }
            *image_aliases.entry(page.image.id).or_default() += 1;
            image_bytes
                .entry(page.image.id)
                .or_insert_with(|| {
                    u64::from(page.image.width)
                        .saturating_mul(u64::from(page.image.height))
                        .saturating_mul(4)
                });
        }
        let mut resident_bytes = image_bytes.values().copied().sum::<u64>();
        if resident_bytes <= HD_PAGE_CACHE_SOFT_BUDGET_BYTES {
            return;
        }
        let current_submit = self.next_submit_id.saturating_sub(1);
        let mut candidates = self
            .page_cache
            .iter()
            .filter_map(|(&handle, page)| {
                (page.image.id != placeholder_id
                    && !self.retained_page_handles.contains(&handle)
                    && !retained_image_ids.contains(&page.image.id))
                    .then_some((page.last_used_submit, handle, page.image.id))
            })
            .collect::<Vec<_>>();
        candidates.sort_unstable();
        for (last_used_submit, handle, image_id) in candidates {
            if resident_bytes <= HD_PAGE_CACHE_SOFT_BUDGET_BYTES {
                break;
            }
            let age = current_submit.saturating_sub(last_used_submit);
            if !inactive_page_is_evictable(resident_bytes, age) {
                continue;
            }
            let Some(page) = self.page_cache.get_mut(&handle) else {
                continue;
            };
            if page.image.id != image_id {
                continue;
            }
            page.image = placeholder.clone();
            page.shared = false;
            page.seed_identity = None;
            if let Some(aliases) = image_aliases.get_mut(&image_id) {
                *aliases = aliases.saturating_sub(1);
                if *aliases == 0 {
                    resident_bytes = resident_bytes
                        .saturating_sub(
                            image_bytes.get(&image_id).copied().unwrap_or(0),
                        );
                }
            }
        }
    }
    fn apply_page_ops(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        gpu_profile: &mut Option<ActiveGpuFrame>,
    ) {
        let operations = std::mem::take(&mut self.pending_page_ops);
        let dump_readbacks = page_dump_dir().is_some();
        let mut dumped_handles: Vec<(u32, u64)> = Vec::new();
        for operation in operations {
            let operation_tag = gpu_profile
                .as_ref()
                .map(|_| self.page_op_gpu_tag(&operation));
            let replay = match &operation {
                PageOp::Swap { source, destination, .. } => {
                    [source, destination]
                        .into_iter()
                        .any(|handle| {
                            self.page_cache
                                .get(handle)
                                .is_some_and(|page| page.presentation_sensitive)
                        })
                }
                PageOp::Points { destination, .. }
                | PageOp::Fill { destination, .. }
                | PageOp::Copy { destination, .. }
                | PageOp::TransformCopy { destination, .. }
                | PageOp::Color { destination, .. }
                | PageOp::PresentationOverlay { destination, .. }
                | PageOp::UploadLogical { destination, .. }
                | PageOp::UploadLogicalRect { destination, .. } => {
                    self.page_cache
                        .get(destination)
                        .is_some_and(|page| page.presentation_sensitive)
                }
            };
            if !replay {
                continue;
            }
            if dump_readbacks {
                let touched: Vec<u32> = match &operation {
                    PageOp::Swap { source, destination, .. } => {
                        vec![* source, * destination]
                    }
                    PageOp::Copy { source, destination, .. }
                    | PageOp::TransformCopy { source, destination, .. } => {
                        vec![* source, * destination]
                    }
                    PageOp::Points { destination, .. }
                    | PageOp::Fill { destination, .. }
                    | PageOp::Color { destination, .. }
                    | PageOp::PresentationOverlay { destination, .. }
                    | PageOp::UploadLogical { destination, .. }
                    | PageOp::UploadLogicalRect { destination, .. } => {
                        vec![* destination]
                    }
                };
                for handle in touched {
                    if dumped_handles.iter().any(|(h, _)| *h == handle) {
                        continue;
                    }
                    if let Some(revision) = self
                        .page_cache
                        .get(&handle)
                        .map(|page| page.revision)
                    {
                        dumped_handles.push((handle, revision));
                    }
                }
            }
            if let PageOp::UploadLogical { destination, width, height, pixels } = operation {
                let image = self
                    .upload_rgba_nearest_scaled_profiled(
                        &pixels,
                        width,
                        height,
                        self.presentation_scale,
                        gpu_profile.as_mut(),
                        operation_tag.clone(),
                    );
                if let Some(cached) = self.page_cache.get_mut(&destination) {
                    cached.image = image;
                    cached.logical_width = width;
                    cached.logical_height = height;
                    cached.shared = false;
                    cached.seed_identity = None;
                }
                continue;
            }
            if let PageOp::Swap {
                source,
                destination,
                source_rect,
                destination_x,
                destination_y,
                swap_alpha,
            } = operation {
                if !self.materialize_page(source, encoder)
                    || !self.materialize_page(destination, encoder)
                {
                    continue;
                }
                let (source_logical_width, source_logical_height, source_image) = {
                    let state = self.page_cache.get(&source).expect("materialized");
                    (state.logical_width, state.logical_height, state.image.clone())
                };
                let (
                    destination_logical_width,
                    destination_logical_height,
                    destination_image,
                ) = {
                    let state = self.page_cache.get(&destination).expect("materialized");
                    (state.logical_width, state.logical_height, state.image.clone())
                };
                let Some((sx, sy, dx, dy, width, height)) = normalize_swap_rect(
                    source_rect,
                    destination_x,
                    destination_y,
                    source_logical_width,
                    source_logical_height,
                    destination_logical_width,
                    destination_logical_height,
                ) else {
                    continue;
                };
                let swap_encoder_span = operation_tag
                    .clone()
                    .and_then(|tag| gpu_profile.as_mut()?.encoder_span(tag));
                if let Some(span) = &swap_encoder_span {
                    span.write_begin(encoder);
                }
                self.invalidate_shared_page_image(&source_image);
                self.invalidate_shared_page_image(&destination_image);
                let source_snapshot = self
                    .empty_rgba_image(
                        source_image.width,
                        source_image.height,
                        "presentation swap source snapshot",
                    );
                let destination_snapshot = self
                    .empty_rgba_image(
                        destination_image.width,
                        destination_image.height,
                        "presentation swap destination snapshot",
                    );
                self.copy_image(encoder, &source_image, &source_snapshot);
                self.copy_image(encoder, &destination_image, &destination_snapshot);
                if swap_alpha {
                    let scale = self.presentation_scale;
                    let extent = wgpu::Extent3d {
                        width: width * scale,
                        height: height * scale,
                        depth_or_array_layers: 1,
                    };
                    self.update_profile_counters(|counters| {
                        counters.texture_copy_bytes
                            += u64::from(extent.width) * u64::from(extent.height) * 4
                                * 2;
                    });
                    encoder
                        .copy_texture_to_texture(
                            texture_region(
                                &destination_snapshot.texture,
                                dx * scale,
                                dy * scale,
                            ),
                            texture_region(
                                &source_image.texture,
                                sx * scale,
                                sy * scale,
                            ),
                            extent,
                        );
                    encoder
                        .copy_texture_to_texture(
                            texture_region(
                                &source_snapshot.texture,
                                sx * scale,
                                sy * scale,
                            ),
                            texture_region(
                                &destination_image.texture,
                                dx * scale,
                                dy * scale,
                            ),
                            extent,
                        );
                } else {
                    let uniform = QuadUniform {
                        native_transparency: 0,
                        source_has_alpha: 0,
                        draw_mode: 10,
                        _padding: 0,
                        color_a: 0,
                        color_b: 0,
                        parameter: 0,
                        _padding_b: 0,
                    };
                    let source_vertices = page_op_vertices(
                        [sx as i32, sy as i32, width as i32, height as i32],
                        [dx as i32, dy as i32, width as i32, height as i32],
                        destination_logical_width,
                        destination_logical_height,
                    );
                    let destination_vertices = page_op_vertices(
                        [dx as i32, dy as i32, width as i32, height as i32],
                        [sx as i32, sy as i32, width as i32, height as i32],
                        source_logical_width,
                        source_logical_height,
                    );
                    let source_resources = self
                        .make_draw_resources(
                            &source_vertices,
                            &destination_snapshot.view,
                            &source_snapshot.view,
                            &self.gpu.sampler,
                            uniform,
                        );
                    let destination_resources = self
                        .make_draw_resources(
                            &destination_vertices,
                            &source_snapshot.view,
                            &destination_snapshot.view,
                            &self.gpu.sampler,
                            uniform,
                        );
                    let source_projection = self
                        .gpu
                        .make_logical_bind_group(
                            source_logical_width,
                            source_logical_height,
                        );
                    let destination_projection = self
                        .gpu
                        .make_logical_bind_group(
                            destination_logical_width,
                            destination_logical_height,
                        );
                    {
                        let mut tag = operation_tag
                            .clone()
                            .unwrap_or_else(|| GpuSpanTag::stage("page_op.swap.source"));
                        tag.name = "page_op.swap.source".to_owned();
                        let span = (swap_encoder_span.is_none())
                            .then(|| gpu_profile.as_mut()?.pass_span(tag))
                            .flatten();
                        let timestamp_writes = span
                            .as_ref()
                            .map(|span| span.pass_writes());
                        self.update_profile_counters(|counters| {
                            counters.render_passes += 1;
                        });
                        let mut pass = encoder
                            .begin_render_pass(
                                &wgpu::RenderPassDescriptor {
                                    label: Some("presentation RGB-only swap source"),
                                    color_attachments: &[
                                        Some(wgpu::RenderPassColorAttachment {
                                            view: &source_image.view,
                                            resolve_target: None,
                                            depth_slice: None,
                                            ops: wgpu::Operations {
                                                load: wgpu::LoadOp::Load,
                                                store: wgpu::StoreOp::Store,
                                            },
                                        }),
                                    ],
                                    depth_stencil_attachment: None,
                                    timestamp_writes,
                                    occlusion_query_set: None,
                                },
                            );
                        pass.set_pipeline(&self.gpu.page_op_pipeline);
                        pass.set_bind_group(0, &source_projection, &[]);
                        pass.set_vertex_buffer(0, source_resources.0.slice(..));
                        pass.set_bind_group(1, &source_resources.1, &[]);
                        pass.draw(0..6, 0..1);
                    }
                    {
                        let mut tag = operation_tag
                            .clone()
                            .unwrap_or_else(|| GpuSpanTag::stage(
                                "page_op.swap.destination",
                            ));
                        tag.name = "page_op.swap.destination".to_owned();
                        let span = (swap_encoder_span.is_none())
                            .then(|| gpu_profile.as_mut()?.pass_span(tag))
                            .flatten();
                        let timestamp_writes = span
                            .as_ref()
                            .map(|span| span.pass_writes());
                        self.update_profile_counters(|counters| {
                            counters.render_passes += 1;
                        });
                        let mut pass = encoder
                            .begin_render_pass(
                                &wgpu::RenderPassDescriptor {
                                    label: Some("presentation RGB-only swap destination"),
                                    color_attachments: &[
                                        Some(wgpu::RenderPassColorAttachment {
                                            view: &destination_image.view,
                                            resolve_target: None,
                                            depth_slice: None,
                                            ops: wgpu::Operations {
                                                load: wgpu::LoadOp::Load,
                                                store: wgpu::StoreOp::Store,
                                            },
                                        }),
                                    ],
                                    depth_stencil_attachment: None,
                                    timestamp_writes,
                                    occlusion_query_set: None,
                                },
                            );
                        pass.set_pipeline(&self.gpu.page_op_pipeline);
                        pass.set_bind_group(0, &destination_projection, &[]);
                        pass.set_vertex_buffer(0, destination_resources.0.slice(..));
                        pass.set_bind_group(1, &destination_resources.1, &[]);
                        pass.draw(0..6, 0..1);
                    }
                }
                if let Some(span) = &swap_encoder_span {
                    span.write_end(encoder);
                }
                continue;
            }
            if let PageOp::Points { destination, points } = operation {
                if !self.materialize_page(destination, encoder) {
                    continue;
                }
                let Some(destination_state) = self.page_cache.get(&destination) else {
                    continue;
                };
                let destination_image = destination_state.image.clone();
                let logical_width = destination_state.logical_width;
                let logical_height = destination_state.logical_height;
                self.invalidate_shared_page_image(&destination_image);
                let dummy = self.upload_rgba(&[0, 0, 0, 0], 1, 1);
                let resources = points
                    .iter()
                    .map(|point| {
                        let vertices = page_op_vertices(
                            [point.x, point.y, 1, 1],
                            [0, 0, 1, 1],
                            1,
                            1,
                        );
                        self.make_draw_resources(
                            &vertices,
                            &dummy.view,
                            &dummy.view,
                            &self.gpu.sampler,
                            QuadUniform {
                                native_transparency: 0,
                                source_has_alpha: 0,
                                draw_mode: if point.alpha_view { 6 } else { 1 },
                                _padding: 0,
                                color_a: point.color,
                                color_b: 0,
                                parameter: 0,
                                _padding_b: 0,
                            },
                        )
                    })
                    .collect::<Vec<_>>();
                let page_bind_group = self
                    .gpu
                    .make_logical_bind_group(logical_width, logical_height);
                let span = operation_tag
                    .clone()
                    .and_then(|tag| gpu_profile.as_mut()?.pass_span(tag));
                let timestamp_writes = span.as_ref().map(|span| span.pass_writes());
                self.update_profile_counters(|counters| counters.render_passes += 1);
                let mut pass = encoder
                    .begin_render_pass(
                        &wgpu::RenderPassDescriptor {
                            label: Some("batched presentation point operations"),
                            color_attachments: &[
                                Some(wgpu::RenderPassColorAttachment {
                                    view: &destination_image.view,
                                    resolve_target: None,
                                    depth_slice: None,
                                    ops: wgpu::Operations {
                                        load: wgpu::LoadOp::Load,
                                        store: wgpu::StoreOp::Store,
                                    },
                                }),
                            ],
                            depth_stencil_attachment: None,
                            timestamp_writes,
                            occlusion_query_set: None,
                        },
                    );
                pass.set_pipeline(&self.gpu.page_op_pipeline);
                pass.set_bind_group(0, &page_bind_group, &[]);
                for (vertex_buffer, bind_group) in &resources {
                    pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                    pass.set_bind_group(1, bind_group, &[]);
                    pass.draw(0..6, 0..1);
                }
                continue;
            }
            let destination_handle = match &operation {
                PageOp::Fill { destination, .. }
                | PageOp::Copy { destination, .. }
                | PageOp::TransformCopy { destination, .. }
                | PageOp::Color { destination, .. }
                | PageOp::PresentationOverlay { destination, .. } => *destination,
                PageOp::UploadLogicalRect { destination, .. } => *destination,
                PageOp::UploadLogical { .. }
                | PageOp::Swap { .. }
                | PageOp::Points { .. } => unreachable!(),
            };
            let Some(destination_state) = self.page_cache.get(&destination_handle) else {
                continue;
            };
            if destination_state.shared
                && !self.materialize_page(destination_handle, encoder)
            {
                continue;
            }
            let Some(destination_state) = self.page_cache.get(&destination_handle) else {
                continue;
            };
            let destination = destination_state.image.clone();
            let logical_width = destination_state.logical_width;
            let logical_height = destination_state.logical_height;
            let (linear_filter, scissor_rect) = match &operation {
                PageOp::TransformCopy { linear_filter, clip_rect, .. } => {
                    let Some(scissor) = normalize_scissor_rect(
                        *clip_rect,
                        logical_width,
                        logical_height,
                        self.presentation_scale,
                    ) else {
                        continue;
                    };
                    (*linear_filter, Some(scissor))
                }
                _ => (false, None),
            };
            self.invalidate_shared_page_image(&destination);
            let encoder_operation_span = operation_tag
                .clone()
                .and_then(|tag| gpu_profile.as_mut()?.encoder_span(tag));
            if let Some(span) = &encoder_operation_span {
                span.write_begin(encoder);
            }
            let scratch = self.operation_scratch(destination.width, destination.height);
            self.copy_image(encoder, &destination, &scratch);
            let (source, source_is_presented_scene, vertices, uniform) = match operation {
                PageOp::Fill { rect, color, mode, parameter, .. } => {
                    (
                        Some(scratch.clone()),
                        false,
                        page_op_vertices(rect, rect, logical_width, logical_height),
                        QuadUniform {
                            native_transparency: 0,
                            source_has_alpha: 0,
                            draw_mode: match mode {
                                PageFillMode::Replace => 1,
                                PageFillMode::Clear => 5,
                                PageFillMode::Blend => 2,
                                PageFillMode::Multiply => 3,
                                PageFillMode::Overlay => 4,
                                PageFillMode::AlphaReplace => 6,
                                PageFillMode::AlphaBlend => 7,
                                PageFillMode::ColourOnly => 8,
                            },
                            _padding: 0,
                            color_a: color,
                            color_b: 0,
                            parameter,
                            _padding_b: 0,
                        },
                    )
                }
                PageOp::Copy {
                    source,
                    source_is_presented_scene,
                    source_rect,
                    destination_rect,
                    mode,
                    parameter,
                    source_has_alpha,
                    destination_has_alpha,
                    source_alpha_view,
                    destination_alpha_view,
                    ..
                } => {
                    let (source_width, source_height, source_image) = if source_is_presented_scene {
                        (self.internal_w, self.internal_h, None)
                    } else {
                        let Some(source_state) = self.page_cache.get(&source) else {
                            continue;
                        };
                        let source_image = if source == destination_handle {
                            scratch.clone()
                        } else {
                            source_state.image.clone()
                        };
                        (
                            source_state.logical_width,
                            source_state.logical_height,
                            Some(source_image),
                        )
                    };
                    let Some((source_rect, destination_rect)) = normalize_copy_rects(
                        source_rect,
                        destination_rect,
                        source_width,
                        source_height,
                        logical_width,
                        logical_height,
                    ) else {
                        continue;
                    };
                    (
                        source_image,
                        source_is_presented_scene,
                        page_op_vertices(
                            destination_rect,
                            source_rect,
                            source_width,
                            source_height,
                        ),
                        QuadUniform {
                            native_transparency: 0,
                            source_has_alpha: u32::from(source_has_alpha),
                            draw_mode: match mode {
                                PageCopyMode::Replace => 10,
                                PageCopyMode::Alpha => 11,
                                PageCopyMode::Multiply => 12,
                                PageCopyMode::ReverseMultiply => 13,
                                PageCopyMode::AlphaPlane => 14,
                                PageCopyMode::AlphaMultiply => 15,
                                PageCopyMode::AlphaReverseMultiply => 16,
                                PageCopyMode::Additive => 30,
                                PageCopyMode::SpriteMultiply => 31,
                                PageCopyMode::Lighten => 32,
                            },
                            _padding: 0,
                            color_a: u32::from(destination_has_alpha)
                                | (u32::from(source_alpha_view) << 1)
                                | (u32::from(destination_alpha_view) << 2),
                            color_b: 0,
                            parameter,
                            _padding_b: 0,
                        },
                    )
                }
                PageOp::Color { rect, mode, color_a, color_b, parameter, .. } => {
                    (
                        Some(scratch.clone()),
                        false,
                        page_op_vertices(rect, rect, logical_width, logical_height),
                        QuadUniform {
                            native_transparency: 0,
                            source_has_alpha: 0,
                            draw_mode: match mode {
                                PageColorMode::Invert => 20,
                                PageColorMode::Sepia => 21,
                            },
                            _padding: 0,
                            color_a,
                            color_b,
                            parameter,
                            _padding_b: 0,
                        },
                    )
                }
                PageOp::TransformCopy {
                    source,
                    source_is_presented_scene,
                    source_rect,
                    destination_rect,
                    angle_degrees,
                    scale_x,
                    scale_y,
                    mode,
                    parameter,
                    source_has_alpha,
                    destination_has_alpha,
                    ..
                } => {
                    let (source_width, source_height, source_image) = if source_is_presented_scene {
                        (self.internal_w, self.internal_h, None)
                    } else {
                        let Some(source_state) = self.page_cache.get(&source) else {
                            continue;
                        };
                        let source_image = if source == destination_handle {
                            scratch.clone()
                        } else {
                            source_state.image.clone()
                        };
                        (
                            source_state.logical_width,
                            source_state.logical_height,
                            Some(source_image),
                        )
                    };
                    (
                        source_image,
                        source_is_presented_scene,
                        transformed_page_op_vertices(
                            destination_rect,
                            source_rect,
                            source_width,
                            source_height,
                            angle_degrees,
                            scale_x,
                            scale_y,
                        ),
                        QuadUniform {
                            native_transparency: 0,
                            source_has_alpha: u32::from(source_has_alpha),
                            draw_mode: match mode {
                                PageCopyMode::Replace => 33,
                                PageCopyMode::Alpha => 11,
                                PageCopyMode::Multiply => 12,
                                PageCopyMode::ReverseMultiply => 13,
                                PageCopyMode::AlphaPlane => 14,
                                PageCopyMode::AlphaMultiply => 15,
                                PageCopyMode::AlphaReverseMultiply => 16,
                                PageCopyMode::Additive => 30,
                                PageCopyMode::SpriteMultiply => 31,
                                PageCopyMode::Lighten => 32,
                            },
                            _padding: 0,
                            color_a: u32::from(destination_has_alpha),
                            color_b: 0,
                            parameter,
                            _padding_b: 0,
                        },
                    )
                }
                PageOp::PresentationOverlay {
                    physical_x,
                    physical_y,
                    width,
                    height,
                    pixels,
                    destination_has_alpha,
                    ..
                } => {
                    let source = self.upload_rgba(&pixels, width, height);
                    (
                        Some(source),
                        false,
                        page_op_physical_vertices(
                            physical_x,
                            physical_y,
                            width,
                            height,
                            self.presentation_scale,
                        ),
                        QuadUniform {
                            native_transparency: 0,
                            source_has_alpha: 1,
                            draw_mode: 17,
                            _padding: 0,
                            color_a: u32::from(destination_has_alpha),
                            color_b: 0,
                            parameter: 0,
                            _padding_b: 0,
                        },
                    )
                }
                PageOp::UploadLogicalRect {
                    page_width,
                    page_height,
                    rect,
                    pixels,
                    ..
                } => {
                    let Some((rect, rect_pixels)) = extract_logical_rect(
                        &pixels,
                        page_width,
                        page_height,
                        rect,
                    ) else {
                        continue;
                    };
                    let source = self
                        .upload_rgba(&rect_pixels, rect[2] as u32, rect[3] as u32);
                    (
                        Some(source),
                        false,
                        page_op_vertices(
                            rect,
                            [0, 0, rect[2], rect[3]],
                            rect[2] as u32,
                            rect[3] as u32,
                        ),
                        QuadUniform {
                            native_transparency: 0,
                            source_has_alpha: 1,
                            draw_mode: 18,
                            _padding: 0,
                            color_a: 0,
                            color_b: 0,
                            parameter: 0,
                            _padding_b: 0,
                        },
                    )
                }
                PageOp::UploadLogical { .. }
                | PageOp::Swap { .. }
                | PageOp::Points { .. } => unreachable!(),
            };
            let source_view = if source_is_presented_scene {
                &self.composition_view
            } else {
                &source.as_ref().expect("ordinary PageOp source").view
            };
            let (vertex_buffer, bind_group) = self
                .make_draw_resources(
                    &vertices,
                    source_view,
                    &scratch.view,
                    if linear_filter { &self.linear_sampler } else { &self.gpu.sampler },
                    uniform,
                );
            let page_bind_group = self
                .gpu
                .make_logical_bind_group(logical_width, logical_height);
            let span = (encoder_operation_span.is_none())
                .then(|| {
                    operation_tag.and_then(|tag| gpu_profile.as_mut()?.pass_span(tag))
                })
                .flatten();
            let timestamp_writes = span.as_ref().map(|span| span.pass_writes());
            self.update_profile_counters(|counters| counters.render_passes += 1);
            let mut pass = encoder
                .begin_render_pass(
                    &wgpu::RenderPassDescriptor {
                        label: Some("presentation page operation"),
                        color_attachments: &[
                            Some(wgpu::RenderPassColorAttachment {
                                view: &destination.view,
                                resolve_target: None,
                                depth_slice: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Load,
                                    store: wgpu::StoreOp::Store,
                                },
                            }),
                        ],
                        depth_stencil_attachment: None,
                        timestamp_writes,
                        occlusion_query_set: None,
                    },
                );
            pass.set_pipeline(&self.gpu.page_op_pipeline);
            pass.set_bind_group(0, &page_bind_group, &[]);
            if let Some([x, y, width, height]) = scissor_rect {
                pass.set_scissor_rect(x, y, width, height);
            }
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            pass.set_bind_group(1, &bind_group, &[]);
            pass.draw(0..6, 0..1);
            drop(pass);
            if let Some(span) = &encoder_operation_span {
                span.write_end(encoder);
            }
        }
        for quad in &mut self.quads {
            if let Some(handle) = quad.page {
                if let Some(cached) = self.page_cache.get(&handle) {
                    quad.image = cached.image.clone();
                }
            }
        }
        if let Some(display_page) = self.pending_display_page.take() {
            self.display_image = display_page
                .and_then(|handle| {
                    let source = self.page_cache.get(&handle)?.image.clone();
                    let snapshot = self
                        .empty_rgba_image(
                            source.width,
                            source.height,
                            "explicit display page snapshot",
                        );
                    self.copy_image(encoder, &source, &snapshot);
                    Some(snapshot)
                });
        }
        self.queue_post_op_dump_readbacks(encoder, &dumped_handles);
        for handle in std::mem::take(&mut self.pending_released_pages) {
            if let Some(cached) = self.page_cache.remove(&handle) {
                if let Some(identity) = cached.seed_identity {
                    self.pool_released_seed(identity, cached.image);
                }
            }
        }
        self.trim_inactive_page_textures();
        self.shared_page_images
            .retain(|_, shared| {
                shared.pixels.strong_count() != 0 && shared.image.strong_count() != 0
            });
    }
    fn queue_post_op_dump_readbacks(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        handles: &[(u32, u64)],
    ) {
        for &(handle, revision) in handles.iter().take(6) {
            let Some(cached) = self.page_cache.get(&handle) else {
                continue;
            };
            let image = cached.image.clone();
            let label = format!("post_page{:05}_r{}", handle, revision);
            if let Some(readback) = encode_texture_readback(
                &self.gpu.device,
                encoder,
                &image.texture,
                image.width,
                image.height,
                &label,
            ) {
                self.page_dump_readbacks.push(readback);
            }
        }
    }
    fn flush_page_dump_readbacks(&mut self) {
        for readback in std::mem::take(&mut self.page_dump_readbacks) {
            let (sender, receiver) = std::sync::mpsc::channel();
            readback
                .buffer
                .slice(..)
                .map_async(
                    wgpu::MapMode::Read,
                    move |result| {
                        let _ = sender.send(result);
                    },
                );
            let poll_result = self.gpu.device.poll(wgpu::PollType::Wait);
            match receiver.recv() {
                Err(_) => {
                    eprintln!(
                        "[DUMP-DIAG] readback map never completed: label={} poll={:?}",
                        readback.label, poll_result
                    );
                    continue;
                }
                Ok(Err(map_error)) => {
                    eprintln!(
                        "[DUMP-DIAG] readback map error: label={} err={:?}", readback
                        .label, map_error
                    );
                    continue;
                }
                Ok(Ok(())) => {}
            }
            {
                let data = readback.buffer.slice(..).get_mapped_range();
                dump_readback_png(&readback, &data);
            }
            readback.buffer.unmap();
        }
    }
    pub fn draw_frame(&mut self) {
        let frame_id = self.next_frame_id;
        self.next_frame_id = self.next_frame_id.wrapping_add(1).max(1);
        let _ = self.gpu.device.poll(wgpu::PollType::Poll);
        if let Some(profiler) = self.profiler.as_mut() {
            profiler.collect_gpu_frames();
        }
        let profile_collect_started = self.profiler.as_ref().map(|_| Instant::now());
        let operation_profile = self
            .profiler
            .as_ref()
            .map(|_| profile_page_operations(&self.pending_page_ops))
            .unwrap_or_default();
        let op_pages = match self.profiler.as_ref() {
            Some(_) if self.pending_page_ops.len() >= 32 => {
                profile_op_pages(&self.pending_page_ops)
            }
            _ => Vec::new(),
        };
        let frame_marks: Vec<MarkEvent> = std::mem::take(&mut self.pending_marks)
            .into_iter()
            .map(|mark| MarkEvent {
                name: mark.name,
                detail: mark.detail,
            })
            .collect();
        let draw_modes = self
            .profiler
            .as_ref()
            .map(|_| profile_draw_modes(&self.quads, self.display_image.is_some()))
            .unwrap_or_default();
        let mut profile_collect_us = profile_collect_started
            .map(|started| started.elapsed().as_micros())
            .unwrap_or(0);
        let mut gpu_profile = self
            .profiler
            .as_mut()
            .and_then(|profiler| profiler.begin_gpu_frame(frame_id));
        let draw_started = self.profiler.as_ref().map(|_| Instant::now());
        let surface_started = self.profiler.as_ref().map(|_| Instant::now());
        let output = match self.surface.get_current_texture() {
            Ok(o) => o,
            Err(wgpu::SurfaceError::Lost) => {
                self.surface.configure(&self.gpu.device, &self.surface_config);
                return;
            }
            Err(wgpu::SurfaceError::OutOfMemory) => {
                panic!("GPU out of memory");
            }
            Err(_) => return,
        };
        let surface_acquire_us = surface_started
            .map(|started| started.elapsed().as_micros())
            .unwrap_or(0);
        let surface_view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .gpu
            .device
            .create_command_encoder(
                &wgpu::CommandEncoderDescriptor {
                    label: Some("frame encoder"),
                },
            );
        let frame_gpu_span = gpu_profile
            .as_mut()
            .and_then(|profile| profile.encoder_span(GpuSpanTag::stage("frame")));
        if let Some(span) = &frame_gpu_span {
            span.write_begin(&mut encoder);
        }
        let page_ops_gpu_span = gpu_profile
            .as_mut()
            .and_then(|profile| profile.encoder_span(GpuSpanTag::stage("page_ops")));
        if let Some(span) = &page_ops_gpu_span {
            span.write_begin(&mut encoder);
        }
        let page_ops_started = self.profiler.as_ref().map(|_| Instant::now());
        self.apply_page_ops(&mut encoder, &mut gpu_profile);
        let page_ops_encode_us = page_ops_started
            .map(|started| started.elapsed().as_micros())
            .unwrap_or(0);
        if let Some(span) = &page_ops_gpu_span {
            span.write_end(&mut encoder);
        }
        let composition_gpu_span = gpu_profile
            .as_mut()
            .and_then(|profile| profile.encoder_span(GpuSpanTag::stage("composition")));
        if let Some(span) = &composition_gpu_span {
            span.write_begin(&mut encoder);
        }
        let composition_started = self.profiler.as_ref().map(|_| Instant::now());
        let mut all = Vec::with_capacity(self.quads.len() + 1);
        if let Some(img) = &self.display_image {
            all.push(Quad {
                page: None,
                image: img.clone(),
                x: -(self.viewport_offset.0 as f32),
                y: -(self.viewport_offset.1 as f32),
                w: self.internal_w as f32,
                h: self.internal_h as f32,
                u0: 0.0,
                v0: 0.0,
                u1: 1.0,
                v1: 1.0,
                alpha: 1.0,
                rotation_degrees: 0.0,
                scale_x: 1.0,
                scale_y: 1.0,
                source_has_alpha: false,
                draw_mode: 0,
            });
        }
        all.extend(self.quads.iter().cloned());
        let mut composition_initialized = false;
        let mut cursor = 0usize;
        while cursor < all.len() {
            let quad = &all[cursor];
            let opacity = (quad.alpha.clamp(0.0, 1.0) * 255.0).round() as u32;
            if matches!(quad.draw_mode, 3 | 4) {
                if opacity == 0 {
                    cursor += 1;
                    continue;
                }
                if !composition_initialized {
                    self.clear_composition(&mut encoder, "initialize frame composition");
                    composition_initialized = true;
                }
                self.update_profile_counters(|counters| {
                    counters.texture_copy_bytes
                        += u64::from(self.internal_w) * u64::from(self.internal_h)
                            * u64::from(self.presentation_scale)
                            * u64::from(self.presentation_scale) * 4;
                });
                encoder
                    .copy_texture_to_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture: &self.composition_texture,
                            mip_level: 0,
                            origin: wgpu::Origin3d::ZERO,
                            aspect: wgpu::TextureAspect::All,
                        },
                        wgpu::TexelCopyTextureInfo {
                            texture: &self.destination_snapshot,
                            mip_level: 0,
                            origin: wgpu::Origin3d::ZERO,
                            aspect: wgpu::TextureAspect::All,
                        },
                        wgpu::Extent3d {
                            width: self.internal_w * self.presentation_scale,
                            height: self.internal_h * self.presentation_scale,
                            depth_or_array_layers: 1,
                        },
                    );
                let vertices = quad_vertices(quad);
                let (vertex_buffer, bind_group) = self
                    .make_draw_resources(
                        &vertices,
                        &quad.image.view,
                        &self.destination_snapshot_view,
                        &self.gpu.sampler,
                        QuadUniform {
                            native_transparency: 255 - opacity,
                            source_has_alpha: u32::from(quad.source_has_alpha),
                            draw_mode: quad.draw_mode,
                            _padding: 0,
                            color_a: 0,
                            color_b: 0,
                            parameter: 0,
                            _padding_b: 0,
                        },
                    );
                let detail_tag = self.quad_gpu_tag(quad, "quad.nonlinear");
                let detail_span = gpu_profile
                    .as_mut()
                    .and_then(|profile| profile.detail_span(detail_tag));
                let mut pass = self
                    .begin_composition_pass(
                        &mut encoder,
                        "draw mode 3/4 pass",
                        wgpu::LoadOp::Load,
                    );
                pass.set_pipeline(&self.gpu.nonlinear_pipeline);
                pass.set_bind_group(0, &self.frame_bind_group, &[]);
                pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                pass.set_bind_group(1, &bind_group, &[]);
                if let Some(span) = &detail_span {
                    span.write_begin_in_pass(&mut pass);
                }
                pass.draw(0..6, 0..1);
                if let Some(span) = &detail_span {
                    span.write_end_in_pass(&mut pass);
                }
                drop(pass);
                cursor += 1;
                continue;
            }
            let load = if composition_initialized {
                wgpu::LoadOp::Load
            } else {
                composition_initialized = true;
                wgpu::LoadOp::Clear(wgpu::Color::BLACK)
            };
            let mut pass = self.begin_composition_pass(&mut encoder, "quad pass", load);
            pass.set_pipeline(&self.gpu.composition_pipeline);
            pass.set_bind_group(0, &self.frame_bind_group, &[]);
            while cursor < all.len() {
                let quad = &all[cursor];
                if matches!(quad.draw_mode, 3 | 4) {
                    break;
                }
                let opacity = (quad.alpha.clamp(0.0, 1.0) * 255.0).round() as u32;
                let detail_tag = self.quad_gpu_tag(quad, "quad.ordinary");
                let detail_span = gpu_profile
                    .as_mut()
                    .and_then(|profile| profile.detail_span(detail_tag));
                let vertices = quad_vertices(quad);
                let (vertex_buffer, bind_group) = self
                    .make_draw_resources(
                        &vertices,
                        &quad.image.view,
                        &self.destination_snapshot_view,
                        &self.gpu.sampler,
                        QuadUniform {
                            native_transparency: 255 - opacity,
                            source_has_alpha: u32::from(quad.source_has_alpha),
                            draw_mode: quad.draw_mode,
                            _padding: 0,
                            color_a: 0,
                            color_b: 0,
                            parameter: 0,
                            _padding_b: 0,
                        },
                    );
                pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                pass.set_bind_group(1, &bind_group, &[]);
                if let Some(span) = &detail_span {
                    span.write_begin_in_pass(&mut pass);
                }
                pass.draw(0..6, 0..1);
                if let Some(span) = &detail_span {
                    span.write_end_in_pass(&mut pass);
                }
                cursor += 1;
            }
            drop(pass);
        }
        if !composition_initialized {
            self.clear_composition(&mut encoder, "empty frame composition");
        }
        if let Some(movie) = &self.movie_overlay {
            let geometry_image = match &movie.frame {
                MovieGpuFrame::Bgra8(image) => image.clone(),
                MovieGpuFrame::Nv12 { luma, .. } => luma.clone(),
            };
            let movie_quad = Quad {
                page: None,
                image: geometry_image,
                x: movie.x,
                y: movie.y,
                w: movie.w,
                h: movie.h,
                u0: 0.0,
                v0: 0.0,
                u1: 1.0,
                v1: 1.0,
                alpha: 1.0,
                rotation_degrees: 0.0,
                scale_x: 1.0,
                scale_y: 1.0,
                source_has_alpha: false,
                draw_mode: 0,
            };
            let detail_tag = self.quad_gpu_tag(&movie_quad, "quad.movie");
            let detail_span = gpu_profile
                .as_mut()
                .and_then(|profile| profile.detail_span(detail_tag));
            let vertices = quad_vertices(&movie_quad);
            match &movie.frame {
                MovieGpuFrame::Bgra8(image) => {
                    let (vertex_buffer, bind_group) = self
                        .make_draw_resources(
                            &vertices,
                            &image.view,
                            &self.destination_snapshot_view,
                            &self.linear_sampler,
                            QuadUniform {
                                native_transparency: 0,
                                source_has_alpha: 0,
                                draw_mode: 0,
                                _padding: 0,
                                color_a: 0,
                                color_b: 0,
                                parameter: 0,
                                _padding_b: 0,
                            },
                        );
                    let mut pass = self
                        .begin_composition_pass(
                            &mut encoder,
                            "BGRA movie overlay pass",
                            wgpu::LoadOp::Load,
                        );
                    pass.set_pipeline(&self.gpu.composition_pipeline);
                    pass.set_bind_group(0, &self.frame_bind_group, &[]);
                    pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                    pass.set_bind_group(1, &bind_group, &[]);
                    if let Some(span) = &detail_span {
                        span.write_begin_in_pass(&mut pass);
                    }
                    pass.draw(0..6, 0..1);
                    if let Some(span) = &detail_span {
                        span.write_end_in_pass(&mut pass);
                    }
                }
                MovieGpuFrame::Nv12 { bind_group, .. } => {
                    self.update_profile_counters(|counters| {
                        counters.buffers_created += 1;
                        counters.draw_calls += 1;
                    });
                    let vertex_buffer = self
                        .gpu
                        .device
                        .create_buffer_init(
                            &wgpu::util::BufferInitDescriptor {
                                label: Some("NV12 movie verts"),
                                contents: bytemuck::cast_slice(&vertices),
                                usage: wgpu::BufferUsages::VERTEX,
                            },
                        );
                    let mut pass = self
                        .begin_composition_pass(
                            &mut encoder,
                            "NV12 movie overlay pass",
                            wgpu::LoadOp::Load,
                        );
                    pass.set_pipeline(&self.gpu.movie_pipeline);
                    pass.set_bind_group(0, &self.frame_bind_group, &[]);
                    pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                    pass.set_bind_group(1, bind_group.as_ref(), &[]);
                    if let Some(span) = &detail_span {
                        span.write_begin_in_pass(&mut pass);
                    }
                    pass.draw(0..6, 0..1);
                    if let Some(span) = &detail_span {
                        span.write_end_in_pass(&mut pass);
                    }
                }
            }
        }
        let composition_encode_us = composition_started
            .map(|started| started.elapsed().as_micros())
            .unwrap_or(0);
        if let Some(span) = &composition_gpu_span {
            span.write_end(&mut encoder);
        }
        let present_vertices = fullscreen_vertices(self.internal_w, self.internal_h);
        let (present_vertex_buffer, present_bind_group) = self
            .make_draw_resources(
                &present_vertices,
                &self.composition_view,
                &self.destination_snapshot_view,
                &self.linear_sampler,
                QuadUniform {
                    native_transparency: 0,
                    source_has_alpha: 0,
                    draw_mode: 0,
                    _padding: 0,
                    color_a: 0,
                    color_b: 0,
                    parameter: 0,
                    _padding_b: 0,
                },
            );
        let present_span = gpu_profile
            .as_mut()
            .and_then(|profile| profile.pass_span(GpuSpanTag::stage("present")));
        let timestamp_writes = present_span.as_ref().map(|span| span.pass_writes());
        self.update_profile_counters(|counters| counters.render_passes += 1);
        let mut present_pass = encoder
            .begin_render_pass(
                &wgpu::RenderPassDescriptor {
                    label: Some("present pass"),
                    color_attachments: &[
                        Some(wgpu::RenderPassColorAttachment {
                            view: &surface_view,
                            resolve_target: None,
                            depth_slice: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                store: wgpu::StoreOp::Store,
                            },
                        }),
                    ],
                    depth_stencil_attachment: None,
                    timestamp_writes,
                    occlusion_query_set: None,
                },
            );
        present_pass.set_pipeline(&self.gpu.present_pipeline);
        present_pass.set_bind_group(0, &self.frame_bind_group, &[]);
        present_pass.set_vertex_buffer(0, present_vertex_buffer.slice(..));
        present_pass.set_bind_group(1, &present_bind_group, &[]);
        present_pass.draw(0..6, 0..1);
        drop(present_pass);
        if page_dump_dir().is_some() {
            let scale = self.presentation_scale;
            if let Some(readback) = encode_texture_readback(
                &self.gpu.device,
                &mut encoder,
                &self.composition_texture,
                self.internal_w * scale,
                self.internal_h * scale,
                "scene",
            ) {
                self.page_dump_readbacks.push(readback);
            }
        }
        if let Some(span) = &frame_gpu_span {
            span.write_end(&mut encoder);
        }
        if let (Some(profiler), Some(active)) = (
            self.profiler.as_mut(),
            gpu_profile.as_ref(),
        ) {
            profiler.resolve_gpu_frame(active, &mut encoder);
        }
        let queue_started = self.profiler.as_ref().map(|_| Instant::now());
        self.gpu.queue.submit(std::iter::once(encoder.finish()));
        self.flush_page_dump_readbacks();
        let queue_submit_us = queue_started
            .map(|started| started.elapsed().as_micros())
            .unwrap_or(0);
        if let (Some(profiler), Some(active)) = (
            self.profiler.as_mut(),
            gpu_profile.take(),
        ) {
            profiler.finish_gpu_frame(frame_id, active);
        }
        let present_started = self.profiler.as_ref().map(|_| Instant::now());
        output.present();
        let present_us = present_started
            .map(|started| started.elapsed().as_micros())
            .unwrap_or(0);
        if let Some(draw_started) = draw_started {
            let total_us = draw_started.elapsed().as_micros();
            let profile_tail_started = Instant::now();
            let memory = self.texture_memory_snapshot();
            let resources = self.profile_counters.replace(ResourceCounters::default());
            profile_collect_us += profile_tail_started.elapsed().as_micros();
            let elapsed_ms = self
                .profiler
                .as_ref()
                .map(RenderProfiler::elapsed_ms)
                .unwrap_or(0);
            let frame = CpuFrameProfile {
                frame_id,
                elapsed_ms,
                runtime: std::mem::take(&mut self.runtime_cpu),
                submit: self.last_submit_profile.clone(),
                draw: DrawCpuProfile {
                    total_us,
                    surface_acquire_us,
                    page_ops_encode_us,
                    composition_encode_us,
                    queue_submit_us,
                    present_us,
                    profile_collect_us,
                },
                operations: operation_profile,
                draw_modes,
                resources,
                memory,
                marks: frame_marks,
                op_pages,
            };
            if let Some(profiler) = self.profiler.as_mut() {
                profiler.record_cpu_frame(frame);
            }
        }
    }
}
impl Drop for Renderer {
    fn drop(&mut self) {
        if let Some(profiler) = self.profiler.as_mut() {
            profiler.finish(&self.gpu.device);
        }
    }
}
fn rect_pixels(rect: [i32; 4]) -> u64 {
    u64::try_from(rect[2].max(0)).unwrap_or(0)
        * u64::try_from(rect[3].max(0)).unwrap_or(0)
}
static PAGE_DUMP_DIR: std::sync::OnceLock<Option<std::path::PathBuf>> = std::sync::OnceLock::new();
static PAGE_DUMP_COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(
    0,
);
fn page_dump_dir() -> Option<&'static std::path::PathBuf> {
    PAGE_DUMP_DIR.get_or_init(|| crate::diag::env_knob_path("OSTB_DUMP_PAGES")).as_ref()
}
fn dump_page_debug_gated(
    tag: &str,
    handle: u32,
    revision: u64,
    w: u32,
    h: u32,
    rgba: &[u8],
    force: bool,
) {
    let Some(dir) = page_dump_dir() else { return };
    if w < 256 || h < 256 {
        return;
    }
    if !force && !rgba_has_green(rgba, w as usize) {
        return;
    }
    if PAGE_DUMP_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed) > 1000 {
        return;
    }
    let name = format!(
        "{}/page{:05}_r{}_{}x{}_{}.png", dir.display(), handle, revision, w, h, tag
    );
    let Ok(file) = std::fs::File::create(&name) else {
        return;
    };
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    encoder.set_color(png::ColorType::Rgba);
    if let Ok(mut writer) = encoder.write_header() {
        let _ = writer.write_image_data(rgba);
    }
}
fn rgba_has_green(rgba: &[u8], width: usize) -> bool {
    let mut green = 0u64;
    let stride = 3usize;
    let mut y = 0usize;
    while y * width * 4 < rgba.len() {
        for x in (0..width).step_by(stride) {
            let o = (y * width + x) * 4;
            if o + 3 >= rgba.len() {
                break;
            }
            let (r, g, b) = (rgba[o] as i32, rgba[o + 1] as i32, rgba[o + 2] as i32);
            if g > r + 80 && g > b + 80 {
                green += 1;
            }
        }
        y += stride;
    }
    green >= 30
}
struct PageDumpReadback {
    label: String,
    buffer: wgpu::Buffer,
    width: u32,
    height: u32,
    bytes_per_row: u32,
}
fn encode_texture_readback(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
    label: &str,
) -> Option<PageDumpReadback> {
    if width < 256 || height < 256 {
        return None;
    }
    let bytes_per_row = (width * 4).next_multiple_of(256);
    let buffer_size = u64::from(bytes_per_row) * u64::from(height);
    if buffer_size > 128 * 1024 * 1024 {
        return None;
    }
    let buffer = device
        .create_buffer(
            &wgpu::BufferDescriptor {
                label: Some("page_dump_readback"),
                size: buffer_size,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            },
        );
    encoder
        .copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    Some(PageDumpReadback {
        label: label.to_owned(),
        buffer,
        width,
        height,
        bytes_per_row,
    })
}
fn dump_readback_png(readback: &PageDumpReadback, data: &[u8]) {
    let Some(dir) = page_dump_dir() else { return };
    let width = readback.width as usize;
    let height = readback.height as usize;
    let mut tight = vec![0u8; width * height * 4];
    for row in 0..height {
        let src = row * readback.bytes_per_row as usize;
        let dst = row * width * 4;
        if src + width * 4 > data.len() {
            break;
        }
        tight[dst..dst + width * 4].copy_from_slice(&data[src..src + width * 4]);
    }
    let mut green = 0u64;
    let mut y = 0usize;
    'rows: while y < height {
        for x in (0..width).step_by(3) {
            let o = (y * width + x) * 4;
            if o + 3 >= tight.len() {
                break 'rows;
            }
            let (r, g, b) = (tight[o] as i32, tight[o + 1] as i32, tight[o + 2] as i32);
            if g > r + 80 && g > b + 80 {
                green += 1;
            }
        }
        y += 3;
    }
    let is_post_page = readback.label.starts_with("post_page");
    if !is_post_page {
        static SCENE_DUMP_SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(
            0,
        );
        if !SCENE_DUMP_SEQ
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            .is_multiple_of(30)
        {
            return;
        }
    }
    if PAGE_DUMP_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed) > 600 {
        return;
    }
    let name = format!("{}/{}_{}.png", dir.display(), readback.label, green);
    let Ok(file) = std::fs::File::create(&name) else {
        return;
    };
    let mut encoder = png::Encoder::new(
        std::io::BufWriter::new(file),
        readback.width,
        readback.height,
    );
    encoder.set_color(png::ColorType::Rgba);
    if let Ok(mut writer) = encoder.write_header() {
        let _ = writer.write_image_data(&tight);
    }
}
fn gpu_image_bytes(image: &GpuImage) -> u64 {
    u64::from(image.width) * u64::from(image.height) * 4
}
fn profile_op_pages(operations: &[PageOp]) -> Vec<OpPageStat> {
    const MAX_ENTRIES: usize = 16;
    let mut stats: BTreeMap<(String, u32), (u64, u64)> = BTreeMap::new();
    for operation in operations {
        let (kind, page, pixels) = match operation {
            PageOp::Points { destination, points } => {
                ("points", *destination, points.len() as u64)
            }
            PageOp::Fill { destination, rect, .. } => {
                ("fill", *destination, rect_pixels(*rect))
            }
            PageOp::Copy { destination, destination_rect, .. } => {
                ("copy", *destination, rect_pixels(*destination_rect))
            }
            PageOp::TransformCopy { destination, destination_rect, .. } => {
                ("transform_copy", *destination, rect_pixels(*destination_rect))
            }
            PageOp::Swap { destination, source_rect, .. } => {
                ("swap", *destination, rect_pixels(*source_rect).saturating_mul(2))
            }
            PageOp::Color { destination, rect, .. } => {
                ("color", *destination, rect_pixels(*rect))
            }
            PageOp::PresentationOverlay { destination, width, height, .. } => {
                (
                    "presentation_overlay",
                    *destination,
                    u64::from(*width) * u64::from(*height),
                )
            }
            PageOp::UploadLogical { destination, width, height, .. } => {
                ("upload_logical", *destination, u64::from(*width) * u64::from(*height))
            }
            PageOp::UploadLogicalRect { destination, rect, .. } => {
                ("upload_logical_rect", *destination, rect_pixels(*rect))
            }
        };
        let entry = stats.entry((kind.to_owned(), page)).or_insert((0, 0));
        entry.0 += 1;
        entry.1 = entry.1.saturating_add(pixels);
    }
    let mut entries: Vec<OpPageStat> = stats
        .into_iter()
        .map(|((op, page), (count, pixels))| OpPageStat {
            op,
            page,
            count,
            pixels,
            max_op_pixels: pixels.checked_div(count.max(1)).unwrap_or(0),
        })
        .collect();
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.pixels));
    entries.truncate(MAX_ENTRIES);
    entries
}
fn profile_page_operations(operations: &[PageOp]) -> OperationProfile {
    let mut profile = OperationProfile::default();
    for operation in operations {
        let (name, pixels) = match operation {
            PageOp::Points { points, .. } => ("points", points.len() as u64),
            PageOp::Fill { rect, .. } => ("fill", rect_pixels(*rect)),
            PageOp::Copy { destination_rect, .. } => {
                ("copy", rect_pixels(*destination_rect))
            }
            PageOp::TransformCopy { destination_rect, .. } => {
                ("transform_copy", rect_pixels(*destination_rect))
            }
            PageOp::Swap { source_rect, .. } => ("swap", rect_pixels(*source_rect) * 2),
            PageOp::Color { rect, .. } => ("color", rect_pixels(*rect)),
            PageOp::PresentationOverlay { width, height, .. } => {
                ("presentation_overlay", u64::from(*width) * u64::from(*height))
            }
            PageOp::UploadLogical { width, height, .. } => {
                ("upload_logical", u64::from(*width) * u64::from(*height))
            }
            PageOp::UploadLogicalRect { rect, .. } => {
                ("upload_logical_rect", rect_pixels(*rect))
            }
        };
        *profile.counts.entry(name.to_owned()).or_default() += 1;
        profile.affected_pixels = profile.affected_pixels.saturating_add(pixels);
        profile.estimated_rw_bytes = profile
            .estimated_rw_bytes
            .saturating_add(pixels.saturating_mul(8));
    }
    profile
}
fn profile_draw_modes(quads: &[Quad], has_display_image: bool) -> BTreeMap<u32, u64> {
    let mut modes = BTreeMap::new();
    if has_display_image {
        *modes.entry(0).or_default() += 1;
    }
    for quad in quads {
        *modes.entry(quad.draw_mode).or_default() += 1;
    }
    modes
}
fn create_composition_texture(
    device: &wgpu::Device,
    label: &str,
    scale: u32,
    internal_w: u32,
    internal_h: u32,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device
        .create_texture(
            &wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: internal_w * scale,
                    height: internal_h * scale,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
        );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}
fn page_op_vertices(
    destination: [i32; 4],
    source: [i32; 4],
    source_width: u32,
    source_height: u32,
) -> [Vertex; 6] {
    let [x, y, width, height] = destination.map(|value| value as f32);
    let [source_x, source_y, source_width_rect, source_height_rect] = source
        .map(|value| value as f32);
    let u0 = source_x / source_width.max(1) as f32;
    let v0 = source_y / source_height.max(1) as f32;
    let u1 = (source_x + source_width_rect) / source_width.max(1) as f32;
    let v1 = (source_y + source_height_rect) / source_height.max(1) as f32;
    [
        Vertex {
            pos: [x, y],
            uv: [u0, v0],
        },
        Vertex {
            pos: [x + width, y],
            uv: [u1, v0],
        },
        Vertex {
            pos: [x, y + height],
            uv: [u0, v1],
        },
        Vertex {
            pos: [x + width, y],
            uv: [u1, v0],
        },
        Vertex {
            pos: [x + width, y + height],
            uv: [u1, v1],
        },
        Vertex {
            pos: [x, y + height],
            uv: [u0, v1],
        },
    ]
}
fn page_op_physical_vertices(
    physical_x: i32,
    physical_y: i32,
    width: u32,
    height: u32,
    scale: u32,
) -> [Vertex; 6] {
    let scale = scale.max(1) as f32;
    let x = physical_x as f32 / scale;
    let y = physical_y as f32 / scale;
    let width = width as f32 / scale;
    let height = height as f32 / scale;
    [
        Vertex {
            pos: [x, y],
            uv: [0.0, 0.0],
        },
        Vertex {
            pos: [x + width, y],
            uv: [1.0, 0.0],
        },
        Vertex {
            pos: [x, y + height],
            uv: [0.0, 1.0],
        },
        Vertex {
            pos: [x + width, y],
            uv: [1.0, 0.0],
        },
        Vertex {
            pos: [x + width, y + height],
            uv: [1.0, 1.0],
        },
        Vertex {
            pos: [x, y + height],
            uv: [0.0, 1.0],
        },
    ]
}
#[allow(clippy::too_many_arguments)]
fn transformed_page_op_vertices(
    destination: [i32; 4],
    source: [i32; 4],
    source_width: u32,
    source_height: u32,
    angle_degrees: f32,
    scale_x: f32,
    scale_y: f32,
) -> [Vertex; 6] {
    let [x, y, width, height] = destination.map(|value| value as f32);
    let [source_x, source_y, source_rect_width, source_rect_height] = source
        .map(|value| value as f32);
    let u0 = source_x / source_width.max(1) as f32;
    let v0 = source_y / source_height.max(1) as f32;
    let u1 = (source_x + source_rect_width) / source_width.max(1) as f32;
    let v1 = (source_y + source_rect_height) / source_height.max(1) as f32;
    let [top_left, top_right, bottom_left, bottom_right] = transformed_quad_positions(
        x,
        y,
        width,
        height,
        angle_degrees,
        scale_x,
        scale_y,
    );
    [
        Vertex {
            pos: top_left,
            uv: [u0, v0],
        },
        Vertex {
            pos: top_right,
            uv: [u1, v0],
        },
        Vertex {
            pos: bottom_left,
            uv: [u0, v1],
        },
        Vertex {
            pos: top_right,
            uv: [u1, v0],
        },
        Vertex {
            pos: bottom_right,
            uv: [u1, v1],
        },
        Vertex {
            pos: bottom_left,
            uv: [u0, v1],
        },
    ]
}
fn extract_logical_rect(
    pixels: &[u8],
    page_width: u32,
    page_height: u32,
    rect: [i32; 4],
) -> Option<([i32; 4], Vec<u8>)> {
    let x0 = rect[0].max(0).min(page_width as i32);
    let y0 = rect[1].max(0).min(page_height as i32);
    let x1 = rect[0].saturating_add(rect[2]).max(0).min(page_width as i32);
    let y1 = rect[1].saturating_add(rect[3]).max(0).min(page_height as i32);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let width = (x1 - x0) as usize;
    let height = (y1 - y0) as usize;
    let mut output = Vec::with_capacity(width * height * 4);
    let stride = page_width as usize * 4;
    for y in y0 as usize..y1 as usize {
        let start = y * stride + x0 as usize * 4;
        output.extend_from_slice(&pixels[start..start + width * 4]);
    }
    Some(([x0, y0, width as i32, height as i32], output))
}
fn normalize_scissor_rect(
    rect: [i32; 4],
    logical_width: u32,
    logical_height: u32,
    presentation_scale: u32,
) -> Option<[u32; 4]> {
    let x0 = rect[0].max(0).min(logical_width as i32);
    let y0 = rect[1].max(0).min(logical_height as i32);
    let x1 = rect[0].saturating_add(rect[2]).max(0).min(logical_width as i32);
    let y1 = rect[1].saturating_add(rect[3]).max(0).min(logical_height as i32);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let scale = presentation_scale.max(1);
    Some([
        x0 as u32 * scale,
        y0 as u32 * scale,
        (x1 - x0) as u32 * scale,
        (y1 - y0) as u32 * scale,
    ])
}
fn normalize_swap_rect(
    source: [i32; 4],
    destination_x: i32,
    destination_y: i32,
    source_width: u32,
    source_height: u32,
    destination_width: u32,
    destination_height: u32,
) -> Option<(u32, u32, u32, u32, u32, u32)> {
    let (mut sx, mut sy, mut dx, mut dy) = (
        source[0] as i64,
        source[1] as i64,
        destination_x as i64,
        destination_y as i64,
    );
    let (mut width, mut height) = (source[2] as i64, source[3] as i64);
    if sx < 0 {
        dx -= sx;
        width += sx;
        sx = 0;
    }
    if sy < 0 {
        dy -= sy;
        height += sy;
        sy = 0;
    }
    if dx < 0 {
        sx -= dx;
        width += dx;
        dx = 0;
    }
    if dy < 0 {
        sy -= dy;
        height += dy;
        dy = 0;
    }
    width = width.min(source_width as i64 - sx).min(destination_width as i64 - dx);
    height = height.min(source_height as i64 - sy).min(destination_height as i64 - dy);
    (width > 0 && height > 0)
        .then_some((
            sx as u32,
            sy as u32,
            dx as u32,
            dy as u32,
            width as u32,
            height as u32,
        ))
}
fn normalize_copy_rects(
    source: [i32; 4],
    destination: [i32; 4],
    source_width: u32,
    source_height: u32,
    destination_width: u32,
    destination_height: u32,
) -> Option<([i32; 4], [i32; 4])> {
    if source[2] != destination[2] || source[3] != destination[3] {
        return (source[2] > 0 && source[3] > 0 && destination[2] > 0
            && destination[3] > 0)
            .then_some((source, destination));
    }
    let (mut sx, mut sy, mut dx, mut dy) = (
        i64::from(source[0]),
        i64::from(source[1]),
        i64::from(destination[0]),
        i64::from(destination[1]),
    );
    let (mut width, mut height) = (
        i64::from(source[2]).min(i64::from(destination[2])),
        i64::from(source[3]).min(i64::from(destination[3])),
    );
    if sx < 0 {
        let clipped = -sx;
        sx = 0;
        dx += clipped;
        width -= clipped;
    }
    if sy < 0 {
        let clipped = -sy;
        sy = 0;
        dy += clipped;
        height -= clipped;
    }
    if dx < 0 {
        let clipped = -dx;
        dx = 0;
        sx += clipped;
        width -= clipped;
    }
    if dy < 0 {
        let clipped = -dy;
        dy = 0;
        sy += clipped;
        height -= clipped;
    }
    width = width
        .min(i64::from(source_width) - sx)
        .min(i64::from(destination_width) - dx);
    height = height
        .min(i64::from(source_height) - sy)
        .min(i64::from(destination_height) - dy);
    (width > 0 && height > 0)
        .then_some((
            [sx as i32, sy as i32, width as i32, height as i32],
            [dx as i32, dy as i32, width as i32, height as i32],
        ))
}
fn texture_region(
    texture: &wgpu::Texture,
    x: u32,
    y: u32,
) -> wgpu::TexelCopyTextureInfo<'_> {
    wgpu::TexelCopyTextureInfo {
        texture,
        mip_level: 0,
        origin: wgpu::Origin3d { x, y, z: 0 },
        aspect: wgpu::TextureAspect::All,
    }
}
fn fullscreen_vertices(internal_w: u32, internal_h: u32) -> [Vertex; 6] {
    let w = internal_w as f32;
    let h = internal_h as f32;
    [
        Vertex {
            pos: [0.0, 0.0],
            uv: [0.0, 0.0],
        },
        Vertex {
            pos: [w, 0.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            pos: [0.0, h],
            uv: [0.0, 1.0],
        },
        Vertex {
            pos: [w, 0.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            pos: [w, h],
            uv: [1.0, 1.0],
        },
        Vertex {
            pos: [0.0, h],
            uv: [0.0, 1.0],
        },
    ]
}
fn quad_vertices(q: &Quad) -> [Vertex; 6] {
    let [top_left, top_right, bottom_left, bottom_right] = transformed_quad_positions(
        q.x,
        q.y,
        q.w,
        q.h,
        q.rotation_degrees,
        q.scale_x,
        q.scale_y,
    );
    [
        Vertex {
            pos: top_left,
            uv: [q.u0, q.v0],
        },
        Vertex {
            pos: top_right,
            uv: [q.u1, q.v0],
        },
        Vertex {
            pos: bottom_left,
            uv: [q.u0, q.v1],
        },
        Vertex {
            pos: top_right,
            uv: [q.u1, q.v0],
        },
        Vertex {
            pos: bottom_right,
            uv: [q.u1, q.v1],
        },
        Vertex {
            pos: bottom_left,
            uv: [q.u0, q.v1],
        },
    ]
}
fn transformed_quad_positions(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    rotation_degrees: f32,
    scale_x: f32,
    scale_y: f32,
) -> [[f32; 2]; 4] {
    let centre_x = x + width * 0.5;
    let centre_y = y + height * 0.5;
    let half_w = width * scale_x * 0.5;
    let half_h = height * scale_y * 0.5;
    let radians = rotation_degrees.to_radians();
    let (sin, cos) = radians.sin_cos();
    let transform = |x: f32, y: f32| [
        centre_x + cos * x - sin * y,
        centre_y + sin * x + cos * y,
    ];
    [
        transform(-half_w, -half_h),
        transform(half_w, -half_h),
        transform(-half_w, half_h),
        transform(half_w, half_h),
    ]
}
fn padded_row_bytes(n: u32) -> u32 {
    (n + 255) & !255
}
fn clamp_surface_size(w: u32, h: u32, max_dim: u32) -> (u32, u32) {
    (w.max(1).min(max_dim), h.max(1).min(max_dim))
}

