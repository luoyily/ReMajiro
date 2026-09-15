use wgpu::util::DeviceExt;
pub const INTERNAL_W: u32 = 1024;
pub const INTERNAL_H: u32 = 576;
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
}
impl Vertex {
    const ATTRS: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![
        0 => Float32x2, 1 => Float32x2
    ];
    pub const LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &Self::ATTRS,
    };
}
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct FrameUniform {
    mvp: [[f32; 4]; 4],
}
pub struct GpuContext {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub composition_pipeline: wgpu::RenderPipeline,
    pub nonlinear_pipeline: wgpu::RenderPipeline,
    pub page_op_pipeline: wgpu::RenderPipeline,
    pub movie_pipeline: wgpu::RenderPipeline,
    pub present_pipeline: wgpu::RenderPipeline,
    pub frame_bind_group_layout: wgpu::BindGroupLayout,
    pub texture_bind_group_layout: wgpu::BindGroupLayout,
    pub movie_texture_bind_group_layout: wgpu::BindGroupLayout,
    pub sampler: wgpu::Sampler,
    pub max_texture_dim: u32,
}
impl GpuContext {
    pub fn new(adapter: &wgpu::Adapter, surface_format: wgpu::TextureFormat) -> Self {
        Self::new_with_features(adapter, surface_format, wgpu::Features::empty())
    }
    pub fn new_with_features(
        adapter: &wgpu::Adapter,
        surface_format: wgpu::TextureFormat,
        required_features: wgpu::Features,
    ) -> Self {
        let (device, queue) = pollster::block_on(
                adapter
                    .request_device(&Self::device_descriptor(adapter, required_features)),
            )
            .expect("request_device failed");
        Self::from_device(device, queue, surface_format)
    }
    pub async fn request_with_features(
        adapter: &wgpu::Adapter,
        surface_format: wgpu::TextureFormat,
        required_features: wgpu::Features,
    ) -> Self {
        let (device, queue) = adapter
            .request_device(&Self::device_descriptor(adapter, required_features))
            .await
            .expect("request_device failed");
        Self::from_device(device, queue, surface_format)
    }
    fn device_descriptor(
        adapter: &wgpu::Adapter,
        required_features: wgpu::Features,
    ) -> wgpu::DeviceDescriptor<'static> {
        let adapter_limits = adapter.limits();
        wgpu::DeviceDescriptor {
            label: Some("engine device"),
            required_features,
            required_limits: wgpu::Limits {
                max_texture_dimension_2d: adapter_limits.max_texture_dimension_2d,
                ..wgpu::Limits::default()
            },
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::Off,
        }
    }
    fn from_device(
        device: wgpu::Device,
        queue: wgpu::Queue,
        surface_format: wgpu::TextureFormat,
    ) -> Self {
        let shader = device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("quad shader"),
                source: wgpu::ShaderSource::Wgsl(SHADER_SRC.into()),
            });
        let movie_shader = device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("NV12 movie shader"),
                source: wgpu::ShaderSource::Wgsl(MOVIE_SHADER_SRC.into()),
            });
        let frame_bind_group_layout = device
            .create_bind_group_layout(
                &wgpu::BindGroupLayoutDescriptor {
                    label: Some("frame bind group layout"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::VERTEX,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                    ],
                },
            );
        let texture_bind_group_layout = device
            .create_bind_group_layout(
                &wgpu::BindGroupLayoutDescriptor {
                    label: Some("texture bind group layout"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float {
                                    filterable: true,
                                },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(
                                wgpu::SamplerBindingType::Filtering,
                            ),
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 3,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float {
                                    filterable: false,
                                },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                    ],
                },
            );
        let movie_texture_bind_group_layout = device
            .create_bind_group_layout(
                &wgpu::BindGroupLayoutDescriptor {
                    label: Some("NV12 movie texture bind group layout"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float {
                                    filterable: true,
                                },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float {
                                    filterable: true,
                                },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(
                                wgpu::SamplerBindingType::Filtering,
                            ),
                            count: None,
                        },
                    ],
                },
            );
        let sampler = device
            .create_sampler(
                &wgpu::SamplerDescriptor {
                    label: Some("quad sampler"),
                    address_mode_u: wgpu::AddressMode::ClampToEdge,
                    address_mode_v: wgpu::AddressMode::ClampToEdge,
                    address_mode_w: wgpu::AddressMode::ClampToEdge,
                    mag_filter: wgpu::FilterMode::Nearest,
                    min_filter: wgpu::FilterMode::Nearest,
                    ..Default::default()
                },
            );
        let pipeline_layout = device
            .create_pipeline_layout(
                &wgpu::PipelineLayoutDescriptor {
                    label: Some("quad pipeline layout"),
                    bind_group_layouts: &[
                        &frame_bind_group_layout,
                        &texture_bind_group_layout,
                    ],
                    push_constant_ranges: &[],
                },
            );
        let movie_pipeline_layout = device
            .create_pipeline_layout(
                &wgpu::PipelineLayoutDescriptor {
                    label: Some("NV12 movie pipeline layout"),
                    bind_group_layouts: &[
                        &frame_bind_group_layout,
                        &movie_texture_bind_group_layout,
                    ],
                    push_constant_ranges: &[],
                },
            );
        let native_blend = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent::REPLACE,
        };
        let composition_pipeline = create_pipeline(
            &device,
            &pipeline_layout,
            &shader,
            "composition pipeline",
            "fs_main",
            wgpu::TextureFormat::Rgba8Unorm,
            Some(native_blend),
        );
        let nonlinear_pipeline = create_pipeline(
            &device,
            &pipeline_layout,
            &shader,
            "draw mode 3/4 pipeline",
            "fs_nonlinear",
            wgpu::TextureFormat::Rgba8Unorm,
            None,
        );
        let present_pipeline = create_pipeline(
            &device,
            &pipeline_layout,
            &shader,
            "present pipeline",
            "fs_present",
            surface_format,
            None,
        );
        let page_op_pipeline = create_pipeline(
            &device,
            &pipeline_layout,
            &shader,
            "presentation page operation pipeline",
            "fs_page_op",
            wgpu::TextureFormat::Rgba8Unorm,
            None,
        );
        let movie_pipeline = create_pipeline(
            &device,
            &movie_pipeline_layout,
            &movie_shader,
            "NV12 movie pipeline",
            "fs_movie",
            wgpu::TextureFormat::Rgba8Unorm,
            None,
        );
        Self {
            max_texture_dim: device.limits().max_texture_dimension_2d,
            device,
            queue,
            composition_pipeline,
            nonlinear_pipeline,
            page_op_pipeline,
            movie_pipeline,
            present_pipeline,
            frame_bind_group_layout,
            texture_bind_group_layout,
            movie_texture_bind_group_layout,
            sampler,
        }
    }
    pub fn make_frame_bind_group(
        &self,
        internal_w: u32,
        internal_h: u32,
    ) -> wgpu::BindGroup {
        self.make_logical_bind_group(internal_w, internal_h)
    }
    pub fn make_logical_bind_group(&self, width: u32, height: u32) -> wgpu::BindGroup {
        let mvp = ortho_top_left(0.0, width as f32, 0.0, height as f32);
        let buf = self
            .device
            .create_buffer_init(
                &wgpu::util::BufferInitDescriptor {
                    label: Some("frame uniform"),
                    contents: bytemuck::bytes_of(&FrameUniform { mvp }),
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                },
            );
        self.device
            .create_bind_group(
                &wgpu::BindGroupDescriptor {
                    label: Some("frame bind group"),
                    layout: &self.frame_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: buf.as_entire_binding(),
                        },
                    ],
                },
            )
    }
}
fn create_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    label: &str,
    fragment_entry: &str,
    format: wgpu::TextureFormat,
    blend: Option<wgpu::BlendState>,
) -> wgpu::RenderPipeline {
    device
        .create_render_pipeline(
            &wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[Vertex::LAYOUT],
                },
                fragment: Some(wgpu::FragmentState {
                    module: shader,
                    entry_point: Some(fragment_entry),
                    compilation_options: Default::default(),
                    targets: &[
                        Some(wgpu::ColorTargetState {
                            format,
                            blend,
                            write_mask: wgpu::ColorWrites::ALL,
                        }),
                    ],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            },
        )
}
fn ortho_top_left(left: f32, right: f32, top: f32, bottom: f32) -> [[f32; 4]; 4] {
    let sx = 2.0 / (right - left);
    let sy = -2.0 / (bottom - top);
    let tx = -(right + left) / (right - left);
    let ty = (bottom + top) / (bottom - top);
    [[sx, 0.0, 0.0, 0.0], [0.0, sy, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [tx, ty, 0.0, 1.0]]
}
const SHADER_SRC: &str = r#"
struct FrameUniform {
    mvp: mat4x4<f32>,
};
@group(0) @binding(0) var<uniform> frame: FrameUniform;

struct QuadUniform {
    native_transparency: u32,
    source_has_alpha: u32,
    draw_mode: u32,
    padding: u32,
    color_a: u32,
    color_b: u32,
    parameter: i32,
    padding_b: u32,
};
@group(1) @binding(2) var<uniform> quad: QuadUniform;

struct VsIn {
    @location(0) pos: vec2<f32>,
    @location(1) uv:  vec2<f32>,
};
struct VsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    // Positions are in the 1024x576 internal space; project to clip space.
    out.clip_pos = frame.mvp * vec4<f32>(in.pos, 0.0, 1.0);
    // No V flip here: the projection already maps y=0 → screen top (NDC +1),
    // and the top-left vertex has uv.y=0 which samples the image's first row
    // (its top). The two correspond 1:1, so UVs pass through unchanged.
    out.uv = in.uv;
    return out;
}

@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;
@group(1) @binding(3) var destination_tex: texture_2d<f32>;

// Native composites with `>> 8`, i.e. truncation. Fixed-function blending
// writes the unorm target with round-to-nearest, which biases every layer
// upward by half a level; across a deep draw list that reads as a steady
// brightening, and because only channels >= 128 round up it also skews hue.
// Subtracting half a level before the blend makes the target's rounding land
// on the same value native's truncation would produce
// (`round(x - 0.5) == floor(x)` away from exact half-integers).
const HALF_LSB: f32 = 0.5 / 255.0;

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, in.uv);
    if (quad.draw_mode == 1u) {
        if (quad.native_transparency == 255u) {
            return vec4<f32>(0.0);
        }
        if (quad.native_transparency == 0u) {
            // Native sub_459150 adds the untouched source byte; both terms are
            // integers, so no truncation bias applies.
            return vec4<f32>(c.rgb, 0.0);
        }
        let source_weight = f32(255u - quad.native_transparency);
        return vec4<f32>(c.rgb * (source_weight / 256.0) - HALF_LSB, 0.0);
    }
    let source_anti = round((1.0 - c.a) * 255.0);
    var blend_anti = f32(quad.native_transparency + 1u);
    if (quad.source_has_alpha != 0u) {
        blend_anti += source_anti;
        if (blend_anti >= 255.0) {
            return vec4<f32>(0.0);
        }
        if (blend_anti <= 2.0) {
            return vec4<f32>(c.rgb, 1.0);
        }
    } else if (quad.native_transparency == 0u) {
        return vec4<f32>(c.rgb, 1.0);
    } else if (quad.native_transparency >= 255u) {
        // A source with no anti-data at full transparency contributes nothing:
        // render_blend_rgb_trunc evaluates ((256-255)*src + 256*dst) >> 8, and the >> 8
        // truncates src/256 away, leaving dst untouched. Without this guard the
        // generic path below emits c.rgb/256, which the render target's
        // round-to-nearest turns into +1 on every channel >= 128 — so each
        // fully invisible full-screen layer silently brightens the frame.
        return vec4<f32>(0.0);
    }
    let source_weight = 257.0 - blend_anti;
    // The pipeline uses ONE / ONE_MINUS_SRC_ALPHA. RGB carries the native
    // source weight; alpha encodes the native destination weight.
    return vec4<f32>(c.rgb * (source_weight / 256.0) - HALF_LSB, 1.0 - blend_anti / 256.0);
}

fn mode3_channel(source: u32, destination: u32, transparency: u32) -> u32 {
    let destination_weight = min(transparency + 1u, 256u);
    let multiply_weight = 256u - destination_weight;
    let multiplied = (destination * (source + 1u)) >> 8u;
    return (destination_weight * destination + multiply_weight * multiplied) >> 8u;
}

fn mode4_channel(source: u32, destination: u32, transparency: u32) -> u32 {
    if (transparency == 255u) {
        return destination;
    }
    let maximum_weight = 256u - min(transparency, 256u);
    return (maximum_weight * max(source, destination) + transparency * destination) >> 8u;
}

@fragment
fn fs_nonlinear(in: VsOut) -> @location(0) vec4<f32> {
    let source = vec4<u32>(round(textureSample(tex, samp, in.uv) * 255.0));
    let pixel = vec2<i32>(in.clip_pos.xy);
    let destination = vec4<u32>(round(textureLoad(destination_tex, pixel, 0) * 255.0));
    let transparency = quad.native_transparency;
    var rgb = destination.rgb;
    if (quad.draw_mode == 3u) {
        rgb = vec3<u32>(
            mode3_channel(source.r, destination.r, transparency),
            mode3_channel(source.g, destination.g, transparency),
            mode3_channel(source.b, destination.b, transparency),
        );
    } else if (quad.draw_mode == 4u) {
        rgb = vec3<u32>(
            mode4_channel(source.r, destination.r, transparency),
            mode4_channel(source.g, destination.g, transparency),
            mode4_channel(source.b, destination.b, transparency),
        );
    }
    return vec4<f32>(vec4<u32>(rgb, destination.a)) / 255.0;
}

fn unpack_rgb(value: u32) -> vec3<u32> {
    return vec3<u32>(value & 255u, (value >> 8u) & 255u, (value >> 16u) & 255u);
}

@fragment
fn fs_page_op(in: VsOut) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(in.clip_pos.xy);
    let old = vec4<u32>(round(textureLoad(destination_tex, pixel, 0) * 255.0));
    let source = vec4<u32>(round(textureSample(tex, samp, in.uv) * 255.0));
    let mode = quad.draw_mode;
    let a = unpack_rgb(quad.color_a);
    let b = unpack_rgb(quad.color_b);
    let destination_has_alpha = (quad.color_a & 1u) != 0u;
    let source_alpha_view = (quad.color_a & 2u) != 0u;
    let destination_alpha_view = (quad.color_a & 4u) != 0u;
    let source_native_alpha = select(source.r, 255u - source.a, source_alpha_view);
    var rgb = old.rgb;
    var alpha = old.a;
    if (mode == 1u) {
        if (quad.parameter != 0) {
            alpha = 255u - (quad.color_a & 255u);
        } else {
            rgb = a;
            alpha = 255u;
        }
    } else if (mode == 2u) {
        let old_weight = u32(clamp(quad.parameter + 1, 1, 256));
        let new_weight = 257u - old_weight;
        rgb = (a * new_weight + old.rgb * old_weight) >> vec3<u32>(8u);
    } else if (mode == 3u) {
        rgb = (old.rgb * (a + vec3<u32>(1u))) >> vec3<u32>(8u);
    } else if (mode == 4u) {
        for (var channel = 0u; channel < 3u; channel++) {
            let value = old[channel];
            if (value < 128u) {
                rgb[channel] = 2u * a[channel] * value / 255u;
            } else {
                rgb[channel] = (2u * (a[channel] + value - a[channel] * value / 255u) + 1u) & 255u;
            }
        }
    } else if (mode == 5u) {
        rgb = vec3<u32>(0u);
        alpha = 0u;
    } else if (mode == 6u) {
        alpha = 255u - (quad.color_a & 255u);
    } else if (mode == 7u) {
        let old_weight = u32(clamp(quad.parameter + 1, 1, 256));
        let new_weight = 257u - old_weight;
        let old_anti = 255u - old.a;
        let new_anti = ((quad.color_a & 255u) * new_weight + old_anti * old_weight) >> 8u;
        alpha = 255u - new_anti;
    } else if (mode == 8u) {
        // Colour-plane-only fill (native grp_boxfill on a page with attached
        // anti-data): the alpha channel mirrors that plane and must keep its
        // current value instead of being forced opaque.
        rgb = a;
    } else if (mode == 10u) {
        if (source_alpha_view && destination_alpha_view) {
            alpha = source.a;
        } else if (source_alpha_view) {
            rgb = vec3<u32>(255u - source.a);
            alpha = 255u;
        } else if (destination_alpha_view) {
            alpha = 255u - source.r;
        } else {
            // Native 24-bit colour copies do not copy the port-only RGBA
            // mirror. Attached anti-data is replayed as a separate alpha-view
            // operation.
            rgb = source.rgb;
        }
    } else if (mode == 11u) {
        let global_anti = u32(clamp(quad.parameter, 0, 255));
        let source_anti = 255u - source.a;
        var blend_anti = global_anti + 1u;
        if (quad.source_has_alpha != 0u) {
            blend_anti = global_anti + source_anti + 1u;
        }
        let color_visible = quad.source_has_alpha == 0u || blend_anti < 255u;
        if (color_visible) {
            if (quad.source_has_alpha != 0u && blend_anti <= 2u) {
                rgb = source.rgb;
            } else {
                let source_weight = 257u - blend_anti;
                rgb = (blend_anti * old.rgb + source_weight * source.rgb) >> vec3<u32>(8u);
            }
        }
        if (destination_has_alpha) {
            let destination_anti = 255u - old.a;
            var output_anti = 0u;
            if (quad.source_has_alpha != 0u) {
                output_anti = (destination_anti * (source_anti + 1u)) >> 8u;
            } else if (global_anti != 0u) {
                output_anti = (destination_anti * global_anti) >> 8u;
            }
            alpha = 255u - output_anti;
        } else if (color_visible) {
            alpha = 255u;
        }
    } else if (mode == 12u) {
        rgb = (old.rgb * (source.rgb + vec3<u32>(1u))) >> vec3<u32>(8u);
    } else if (mode == 13u) {
        rgb = vec3<u32>(255u) - (((vec3<u32>(256u) - source.rgb) * (vec3<u32>(255u) - old.rgb)) >> vec3<u32>(8u));
    } else if (mode == 14u) {
        alpha = 255u - source_native_alpha;
    } else if (mode == 15u) {
        let anti = ((255u - old.a) * (source_native_alpha + 1u)) >> 8u;
        alpha = 255u - anti;
    } else if (mode == 16u) {
        let anti = 255u - (((256u - source_native_alpha) * old.a) >> 8u);
        alpha = 255u - anti;
    } else if (mode == 17u) {
        // Presentation glyph buffers are premultiplied by `blend_mask`.
        // Composite them with the same /255 coverage rule as the CPU text
        // renderer; treating them as straight RGBA would multiply coverage a
        // second time and darken every antialiased edge.
        let inverse_coverage = 255u - source.a;
        rgb = min(
            source.rgb + (old.rgb * inverse_coverage) / vec3<u32>(255u),
            vec3<u32>(255u),
        );
        alpha = min(source.a + (old.a * inverse_coverage) / 255u, 255u);
    } else if (mode == 18u) {
        // Renderer-internal nearest-neighbour expansion. Unlike the native
        // colour-copy modes, this initializes all four stored channels.
        rgb = source.rgb;
        alpha = source.a;
    } else if (mode == 20u) {
        rgb = vec3<u32>(255u) - old.rgb;
    } else if (mode == 21u) {
        let luma = i32(old.b + 2u * (old.r + 2u * old.g) + 1u);
        for (var channel = 0u; channel < 3u; channel++) {
            let sepia = (i32(a[channel]) * (2049 - luma) + i32(b[channel]) * luma) / 2048;
            if (quad.parameter >= 0) {
                let old_weight = quad.parameter + 1;
                rgb[channel] = u32((old_weight * i32(old[channel]) + (256 - quad.parameter) * sepia) / 256);
            } else {
                rgb[channel] = u32(sepia);
            }
        }
    } else if (mode == 30u) {
        // sprite_paste draw mode 1: saturated additive RGB. Native treats
        // opacity 255 specially instead of applying the usual >> 8 scale.
        let opacity = u32(clamp(quad.parameter, 0, 255));
        let contribution = select(
            (source.rgb * vec3<u32>(opacity)) >> vec3<u32>(8u),
            source.rgb,
            opacity == 255u,
        );
        rgb = min(old.rgb + contribution, vec3<u32>(255u));
    } else if (mode == 31u) {
        // sprite_paste draw mode 3: interpolate destination toward its
        // per-channel multiply with the source.
        let opacity = u32(clamp(quad.parameter, 0, 255));
        let destination_weight = 256u - opacity;
        let multiplied = (old.rgb * (source.rgb + vec3<u32>(1u))) >> vec3<u32>(8u);
        rgb = (destination_weight * old.rgb + opacity * multiplied) >> vec3<u32>(8u);
    } else if (mode == 32u) {
        // sprite_paste draw mode 4: weighted lighten.
        let opacity = u32(clamp(quad.parameter, 0, 255));
        let maximum_weight = opacity + 1u;
        let destination_weight = 255u - opacity;
        rgb = (
            maximum_weight * max(source.rgb, old.rgb)
            + destination_weight * old.rgb
        ) >> vec3<u32>(8u);
    } else if (mode == 33u) {
        // grp_modify_copy is a transformed replacement, not sprite alpha
        // composition. Native copies antidata only when both pages own it;
        // every other written pixel is materialized opaque by the port.
        rgb = source.rgb;
        alpha = select(
            255u,
            source.a,
            quad.source_has_alpha != 0u && destination_has_alpha,
        );
    }
    return vec4<f32>(vec4<u32>(rgb, alpha)) / 255.0;
}

@fragment
fn fs_present(in: VsOut) -> @location(0) vec4<f32> {
    return textureSample(tex, samp, in.uv);
}
"#;
const MOVIE_SHADER_SRC: &str = r#"
struct FrameUniform {
    mvp: mat4x4<f32>,
};
@group(0) @binding(0) var<uniform> frame: FrameUniform;

struct VsIn {
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
};
struct VsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    out.clip_pos = frame.mvp * vec4<f32>(in.pos, 0.0, 1.0);
    out.uv = in.uv;
    return out;
}

@group(1) @binding(0) var luma_tex: texture_2d<f32>;
@group(1) @binding(1) var chroma_tex: texture_2d<f32>;
@group(1) @binding(2) var movie_sampler: sampler;

@fragment
fn fs_movie(in: VsOut) -> @location(0) vec4<f32> {
    // Media Foundation's ordinary NV12 output uses studio-range Y'CbCr.
    // HD movie assets use BT.709 coefficients. Chroma is sampled linearly at
    // half resolution while luma retains the source frame's full resolution.
    let y = (textureSample(luma_tex, movie_sampler, in.uv).r - 16.0 / 255.0)
        * (255.0 / 219.0);
    let uv = (textureSample(chroma_tex, movie_sampler, in.uv).rg - vec2<f32>(128.0 / 255.0))
        * (255.0 / 224.0);
    let rgb = vec3<f32>(
        y + 1.5748 * uv.y,
        y - 0.1873 * uv.x - 0.4681 * uv.y,
        y + 1.8556 * uv.x,
    );
    return vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
"#;

