use std::sync::Arc;
#[derive(Clone, Debug)]
pub struct PresentationImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Arc<Vec<u8>>,
}
impl PresentationImage {
    pub fn composited_over_seed(
        seed: &[u8],
        seed_width: u32,
        seed_height: u32,
        patch: &PresentationImage,
    ) -> PresentationImage {
        let scale = (patch.width / seed_width.max(1)).max(1) as usize;
        let mut out = patch.pixels.as_ref().clone();
        let pw = patch.width as usize;
        let seed_w = seed_width as usize;
        let seed_h = seed_height as usize;
        for py in 0..patch.height as usize {
            let seed_y = (py / scale).min(seed_h.saturating_sub(1));
            for px in 0..pw {
                let p = (py * pw + px) * 4;
                if out[p + 3] != 0 {
                    continue;
                }
                let seed_x = (px / scale).min(seed_w.saturating_sub(1));
                let s = (seed_y * seed_w + seed_x) * 4;
                out[p..p + 4].copy_from_slice(&seed[s..s + 4]);
            }
        }
        PresentationImage {
            width: patch.width,
            height: patch.height,
            pixels: Arc::new(out),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageFillMode {
    Replace,
    Clear,
    Blend,
    Multiply,
    Overlay,
    AlphaReplace,
    AlphaBlend,
    ColourOnly,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageCopyMode {
    Replace,
    Alpha,
    Multiply,
    ReverseMultiply,
    AlphaPlane,
    AlphaMultiply,
    AlphaReverseMultiply,
    Additive,
    SpriteMultiply,
    Lighten,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageColorMode {
    Invert,
    Sepia,
}
#[derive(Clone, Copy, Debug)]
pub struct PagePoint {
    pub x: i32,
    pub y: i32,
    pub color: u32,
    pub alpha_view: bool,
}
#[derive(Clone, Debug)]
pub enum PageOp {
    Points { destination: u32, points: Vec<PagePoint> },
    Fill {
        destination: u32,
        rect: [i32; 4],
        color: u32,
        mode: PageFillMode,
        parameter: i32,
    },
    Copy {
        source: u32,
        source_is_presented_scene: bool,
        destination: u32,
        source_rect: [i32; 4],
        destination_rect: [i32; 4],
        mode: PageCopyMode,
        parameter: i32,
        source_has_alpha: bool,
        destination_has_alpha: bool,
        source_alpha_view: bool,
        destination_alpha_view: bool,
    },
    TransformCopy {
        source: u32,
        source_is_presented_scene: bool,
        destination: u32,
        source_rect: [i32; 4],
        destination_rect: [i32; 4],
        clip_rect: [i32; 4],
        angle_degrees: f32,
        scale_x: f32,
        scale_y: f32,
        linear_filter: bool,
        mode: PageCopyMode,
        parameter: i32,
        source_has_alpha: bool,
        destination_has_alpha: bool,
    },
    Swap {
        source: u32,
        destination: u32,
        source_rect: [i32; 4],
        destination_x: i32,
        destination_y: i32,
        swap_alpha: bool,
    },
    Color {
        destination: u32,
        rect: [i32; 4],
        mode: PageColorMode,
        color_a: u32,
        color_b: u32,
        parameter: i32,
    },
    PresentationOverlay {
        destination: u32,
        physical_x: i32,
        physical_y: i32,
        width: u32,
        height: u32,
        pixels: Arc<Vec<u8>>,
        destination_has_alpha: bool,
    },
    UploadLogical { destination: u32, width: u32, height: u32, pixels: Arc<Vec<u8>> },
    UploadLogicalRect {
        destination: u32,
        page_width: u32,
        page_height: u32,
        rect: [i32; 4],
        pixels: Arc<Vec<u8>>,
    },
}
#[derive(Clone, Debug)]
pub struct RenderPage {
    pub handle: u32,
    pub debug_name: Option<Arc<str>>,
    pub revision: u64,
    pub width: u32,
    pub height: u32,
    pub pixels: Arc<Vec<u8>>,
    pub presentation_seed: Arc<Vec<u8>>,
    pub presentation: Option<Arc<PresentationImage>>,
}
#[derive(Clone, Debug)]
pub enum RenderMoviePixels {
    Bgra8(Arc<Vec<u8>>),
    Nv12 { luma: Arc<Vec<u8>>, chroma: Arc<Vec<u8>> },
}
#[derive(Clone, Debug)]
pub struct RenderMovieFrame {
    pub revision: u64,
    pub width: u32,
    pub height: u32,
    pub pixels: RenderMoviePixels,
    pub x: f32,
    pub y: f32,
    pub output_width: f32,
    pub output_height: f32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderQuad {
    pub page: u32,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub source_x: f32,
    pub source_y: f32,
    pub source_width: f32,
    pub source_height: f32,
    pub alpha: f32,
    pub rotation_degrees: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub source_has_alpha: bool,
    pub draw_mode: u32,
}
#[derive(Clone, Debug)]
pub struct FrameMark {
    pub name: String,
    pub detail: String,
}
#[derive(Clone, Debug, Default)]
pub struct RenderFrame {
    pub pages: Vec<RenderPage>,
    pub rehydrate_pages: Vec<RenderPage>,
    pub page_ops: Vec<PageOp>,
    pub released_pages: Vec<u32>,
    pub retained_pages: Vec<u32>,
    pub display_page: Option<u32>,
    pub display_epoch: u64,
    pub quads: Vec<RenderQuad>,
    pub movie: Option<RenderMovieFrame>,
    pub viewport_offset: (i32, i32),
    pub marks: Vec<FrameMark>,
}

