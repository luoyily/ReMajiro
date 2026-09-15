use super::{native_8bit_sample, Page, Sprite};
use std::collections::HashMap;
use std::sync::Arc;
pub(super) fn replace_red_colorkey(dst: &mut [u8], source: &[u8]) -> usize {
    debug_assert_eq!(dst.len(), source.len());
    let mut replaced = 0;
    for (target, base) in dst.chunks_exact_mut(4).zip(source.chunks_exact(4)) {
        if target[..3] == [0xFF, 0, 0] {
            target[..3].copy_from_slice(&base[..3]);
            replaced += 1;
        }
    }
    replaced
}
pub(super) fn fill_page_rect(
    page: &mut Page,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    color: u32,
) -> bool {
    let x0 = x.max(0).min(page.width as i32);
    let y0 = y.max(0).min(page.height as i32);
    let x1 = x.saturating_add(width).max(0).min(page.width as i32);
    let y1 = y.saturating_add(height).max(0).min(page.height as i32);
    if x1 <= x0 || y1 <= y0 {
        return false;
    }
    if let Some(samples) = page.indexed_samples.as_mut() {
        let sample = color as u8;
        let samples = Arc::make_mut(samples);
        for row in y0 as usize..y1 as usize {
            let start = row * page.width as usize + x0 as usize;
            let end = row * page.width as usize + x1 as usize;
            samples[start..end].fill(sample);
        }
    }
    let rgba = if page.indexed_samples.is_some() {
        [color as u8, color as u8, color as u8, 0xFF]
    } else {
        [
            (color & 0xFF) as u8,
            ((color >> 8) & 0xFF) as u8,
            ((color >> 16) & 0xFF) as u8,
            0xFF,
        ]
    };
    let stride = page.width as usize * 4;
    let pixels = Arc::make_mut(&mut page.pixels);
    for row in y0 as usize..y1 as usize {
        let start = row * stride + x0 as usize * 4;
        let end = row * stride + x1 as usize * 4;
        for pixel in pixels[start..end].chunks_exact_mut(4) {
            pixel.copy_from_slice(&rgba);
        }
    }
    true
}
pub(super) fn fill_page_alpha_rect(
    page: &mut Page,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    alpha: u8,
) -> bool {
    let x0 = x.max(0).min(page.width as i32);
    let y0 = y.max(0).min(page.height as i32);
    let x1 = x.saturating_add(width).max(0).min(page.width as i32);
    let y1 = y.saturating_add(height).max(0).min(page.height as i32);
    if x1 <= x0 || y1 <= y0 {
        return false;
    }
    let stride = page.width as usize * 4;
    let pixels = Arc::make_mut(&mut page.pixels);
    for row in y0 as usize..y1 as usize {
        let start = row * stride + x0 as usize * 4;
        let end = row * stride + x1 as usize * 4;
        for pixel in pixels[start..end].chunks_exact_mut(4) {
            pixel[3] = 255 - alpha;
        }
    }
    true
}
pub(super) fn fill_antidata_rect(
    page: &mut Page,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    transparency: u8,
) -> bool {
    let x0 = x.max(0).min(page.width as i32);
    let y0 = y.max(0).min(page.height as i32);
    let x1 = x.saturating_add(width).max(0).min(page.width as i32);
    let y1 = y.saturating_add(height).max(0).min(page.height as i32);
    if x1 <= x0 || y1 <= y0 {
        return false;
    }
    if let Some(samples) = page.indexed_samples.as_mut() {
        let samples = Arc::make_mut(samples);
        for row in y0 as usize..y1 as usize {
            let start = row * page.width as usize + x0 as usize;
            let end = row * page.width as usize + x1 as usize;
            samples[start..end].fill(transparency);
        }
    }
    let stride = page.width as usize * 4;
    let pixels = Arc::make_mut(&mut page.pixels);
    for row in y0 as usize..y1 as usize {
        let start = row * stride + x0 as usize * 4;
        let end = row * stride + x1 as usize * 4;
        for pixel in pixels[start..end].chunks_exact_mut(4) {
            pixel.copy_from_slice(&[transparency, transparency, transparency, 0xFF]);
        }
    }
    true
}
pub(super) fn clipped_rect(
    page: &Page,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> Option<(usize, usize, usize, usize)> {
    let x0 = x.max(0).min(page.width as i32);
    let y0 = y.max(0).min(page.height as i32);
    let x1 = x.saturating_add(width).max(0).min(page.width as i32);
    let y1 = y.saturating_add(height).max(0).min(page.height as i32);
    (x1 > x0 && y1 > y0).then_some((x0 as usize, y0 as usize, x1 as usize, y1 as usize))
}
#[allow(clippy::too_many_arguments)]
pub(super) fn clipped_copy_rects(
    src: &Page,
    dst: &Page,
    src_x: i32,
    src_y: i32,
    width: i32,
    height: i32,
    dst_x: i32,
    dst_y: i32,
) -> Option<(usize, usize, usize, usize, usize, usize)> {
    if width <= 0 || height <= 0 {
        return None;
    }
    let (mut sx, mut sy, mut dx, mut dy) = (
        src_x as i64,
        src_y as i64,
        dst_x as i64,
        dst_y as i64,
    );
    let (mut w, mut h) = (width as i64, height as i64);
    if sx < 0 {
        let n = -sx;
        sx = 0;
        dx += n;
        w -= n;
    }
    if sy < 0 {
        let n = -sy;
        sy = 0;
        dy += n;
        h -= n;
    }
    if dx < 0 {
        let n = -dx;
        dx = 0;
        sx += n;
        w -= n;
    }
    if dy < 0 {
        let n = -dy;
        dy = 0;
        sy += n;
        h -= n;
    }
    w = w.min(src.width as i64 - sx).min(dst.width as i64 - dx);
    h = h.min(src.height as i64 - sy).min(dst.height as i64 - dy);
    (w > 0 && h > 0)
        .then_some((
            sx as usize,
            sy as usize,
            dx as usize,
            dy as usize,
            w as usize,
            h as usize,
        ))
}
#[allow(clippy::too_many_arguments)]
pub(super) fn copy_page_region(
    src: &Page,
    dst: &mut Page,
    src_x: i32,
    src_y: i32,
    width: i32,
    height: i32,
    dst_x: i32,
    dst_y: i32,
) -> bool {
    if width <= 0 || height <= 0 {
        return false;
    }
    let (mut sx, mut sy, mut dx, mut dy) = (
        src_x as i64,
        src_y as i64,
        dst_x as i64,
        dst_y as i64,
    );
    let (mut w, mut h) = (width as i64, height as i64);
    if sx < 0 {
        let clipped = -sx;
        sx = 0;
        dx += clipped;
        w -= clipped;
    }
    if sy < 0 {
        let clipped = -sy;
        sy = 0;
        dy += clipped;
        h -= clipped;
    }
    if dx < 0 {
        let clipped = -dx;
        dx = 0;
        sx += clipped;
        w -= clipped;
    }
    if dy < 0 {
        let clipped = -dy;
        dy = 0;
        sy += clipped;
        h -= clipped;
    }
    w = w.min(src.width as i64 - sx).min(dst.width as i64 - dx);
    h = h.min(src.height as i64 - sy).min(dst.height as i64 - dy);
    if w <= 0 || h <= 0 {
        return false;
    }
    let src_stride = src.width as usize * 4;
    let dst_stride = dst.width as usize * 4;
    let row_bytes = w as usize * 4;
    let src_pixels = src.pixels.clone();
    let dst_pixels = Arc::make_mut(&mut dst.pixels);
    for row in 0..h as usize {
        let src_start = (sy as usize + row) * src_stride + sx as usize * 4;
        let dst_start = (dy as usize + row) * dst_stride + dx as usize * 4;
        dst_pixels[dst_start..dst_start + row_bytes]
            .copy_from_slice(&src_pixels[src_start..src_start + row_bytes]);
    }
    if let (Some(src_samples), Some(dst_samples)) = (
        src.indexed_samples.as_ref(),
        dst.indexed_samples.as_mut(),
    ) {
        let src_samples = src_samples.clone();
        let dst_samples = Arc::make_mut(dst_samples);
        for row in 0..h as usize {
            let src_start = (sy as usize + row) * src.width as usize + sx as usize;
            let dst_start = (dy as usize + row) * dst.width as usize + dx as usize;
            dst_samples[dst_start..dst_start + w as usize]
                .copy_from_slice(&src_samples[src_start..src_start + w as usize]);
        }
    }
    true
}
#[allow(clippy::too_many_arguments)]
pub(super) fn copy_page_color_region(
    src: &Page,
    dst: &mut Page,
    src_x: i32,
    src_y: i32,
    width: i32,
    height: i32,
    dst_x: i32,
    dst_y: i32,
) -> bool {
    let Some((sx, sy, dx, dy, width, height)) = clipped_copy_rects(
        src,
        dst,
        src_x,
        src_y,
        width,
        height,
        dst_x,
        dst_y,
    ) else {
        return false;
    };
    let src_stride = src.width as usize * 4;
    let dst_stride = dst.width as usize * 4;
    let source_pixels = src.pixels.clone();
    let destination_pixels = Arc::make_mut(&mut dst.pixels);
    for row in 0..height {
        for column in 0..width {
            let source = (sy + row) * src_stride + (sx + column) * 4;
            let destination = (dy + row) * dst_stride + (dx + column) * 4;
            destination_pixels[destination..destination + 3]
                .copy_from_slice(&source_pixels[source..source + 3]);
        }
    }
    if let (Some(source_samples), Some(destination_samples)) = (
        src.indexed_samples.as_ref(),
        dst.indexed_samples.as_mut(),
    ) {
        let source_samples = source_samples.clone();
        let destination_samples = Arc::make_mut(destination_samples);
        for row in 0..height {
            let source = (sy + row) * src.width as usize + sx;
            let destination = (dy + row) * dst.width as usize + dx;
            destination_samples[destination..destination + width]
                .copy_from_slice(&source_samples[source..source + width]);
        }
    }
    true
}
#[allow(clippy::too_many_arguments)]
pub(super) fn copy_page_region_within(
    page: &mut Page,
    src_x: i32,
    src_y: i32,
    width: i32,
    height: i32,
    dst_x: i32,
    dst_y: i32,
) -> bool {
    let Some((sx, sy, dx, dy, width, height)) = clipped_copy_rects(
        page,
        page,
        src_x,
        src_y,
        width,
        height,
        dst_x,
        dst_y,
    ) else {
        return false;
    };
    let stride = page.width as usize * 4;
    let row_bytes = width * 4;
    let pixels = Arc::make_mut(&mut page.pixels);
    if dy > sy {
        for row in (0..height).rev() {
            let source = (sy + row) * stride + sx * 4;
            let destination = (dy + row) * stride + dx * 4;
            pixels.copy_within(source..source + row_bytes, destination);
        }
    } else {
        for row in 0..height {
            let source = (sy + row) * stride + sx * 4;
            let destination = (dy + row) * stride + dx * 4;
            pixels.copy_within(source..source + row_bytes, destination);
        }
    }
    if let Some(samples) = page.indexed_samples.as_mut() {
        let stride = page.width as usize;
        let samples = Arc::make_mut(samples);
        if dy > sy {
            for row in (0..height).rev() {
                let source = (sy + row) * stride + sx;
                let destination = (dy + row) * stride + dx;
                samples.copy_within(source..source + width, destination);
            }
        } else {
            for row in 0..height {
                let source = (sy + row) * stride + sx;
                let destination = (dy + row) * stride + dx;
                samples.copy_within(source..source + width, destination);
            }
        }
    }
    true
}
#[allow(clippy::too_many_arguments)]
pub(super) fn copy_antidata_region(
    src: &Page,
    dst: &mut Page,
    src_x: i32,
    src_y: i32,
    width: i32,
    height: i32,
    dst_x: i32,
    dst_y: i32,
) -> bool {
    let Some((sx, sy, dx, dy, width, height)) = clipped_copy_rects(
        src,
        dst,
        src_x,
        src_y,
        width,
        height,
        dst_x,
        dst_y,
    ) else {
        return false;
    };
    let dst_stride = dst.width as usize * 4;
    let dst_sample_stride = dst.width as usize;
    let mut copied_samples = dst
        .indexed_samples
        .as_ref()
        .map(|samples| samples.as_ref().clone());
    let dst_pixels = Arc::make_mut(&mut dst.pixels);
    for row in 0..height {
        for column in 0..width {
            let sample = native_8bit_sample(src, sx + column, sy + row);
            if let Some(samples) = copied_samples.as_mut() {
                samples[(dy + row) * dst_sample_stride + dx + column] = sample;
            }
            let target = (dy + row) * dst_stride + (dx + column) * 4;
            dst_pixels[target..target + 3].fill(sample);
            dst_pixels[target + 3] = 0xFF;
        }
    }
    if let Some(samples) = copied_samples {
        dst.indexed_samples = Some(Arc::new(samples));
    }
    true
}
#[allow(clippy::too_many_arguments)]
pub(super) fn blend_indexed_page_region(
    src: &Page,
    dst: &mut Page,
    src_x: i32,
    src_y: i32,
    width: i32,
    height: i32,
    dst_x: i32,
    dst_y: i32,
    transparency: u8,
) -> bool {
    let Some((sx, sy, dx, dy, width, height)) = clipped_copy_rects(
        src,
        dst,
        src_x,
        src_y,
        width,
        height,
        dst_x,
        dst_y,
    ) else {
        return false;
    };
    let Some(dst_samples) = dst.indexed_samples.as_mut() else {
        return false;
    };
    let source_weight = 256 - u16::from(transparency);
    let destination_weight = u16::from(transparency) + 1;
    let destination_stride = dst.width as usize;
    let samples = Arc::make_mut(dst_samples);
    for row in 0..height {
        for column in 0..width {
            let source = native_8bit_sample(src, sx + column, sy + row);
            let destination = &mut samples[(dy + row) * destination_stride + dx
                + column];
            *destination = ((source_weight * u16::from(source)
                + destination_weight * u16::from(*destination)) >> 8) as u8;
        }
    }
    true
}
#[allow(clippy::too_many_arguments)]
pub(super) fn scale_copy_page_region(
    src: &Page,
    dst: &mut Page,
    src_x: i32,
    src_y: i32,
    src_width: i32,
    src_height: i32,
    dst_x: i32,
    dst_y: i32,
    dst_width: i32,
    dst_height: i32,
) -> bool {
    scale_copy_page_region_channels(
        src,
        dst,
        src_x,
        src_y,
        src_width,
        src_height,
        dst_x,
        dst_y,
        dst_width,
        dst_height,
        true,
        false,
    )
}
#[allow(clippy::too_many_arguments)]
pub(super) fn scale_copy_page_region_channels(
    src: &Page,
    dst: &mut Page,
    src_x: i32,
    src_y: i32,
    src_width: i32,
    src_height: i32,
    dst_x: i32,
    dst_y: i32,
    dst_width: i32,
    dst_height: i32,
    copy_alpha: bool,
    preserve_destination_alpha: bool,
) -> bool {
    if src_width <= 0 || src_height <= 0 || dst_width <= 0 || dst_height <= 0 {
        return false;
    }
    if src_width == dst_width && src_height == dst_height && copy_alpha {
        return copy_page_region(
            src,
            dst,
            src_x,
            src_y,
            src_width,
            src_height,
            dst_x,
            dst_y,
        );
    }
    let x0 = dst_x.max(0).min(dst.width as i32);
    let y0 = dst_y.max(0).min(dst.height as i32);
    let x1 = dst_x.saturating_add(dst_width).max(0).min(dst.width as i32);
    let y1 = dst_y.saturating_add(dst_height).max(0).min(dst.height as i32);
    if x1 <= x0 || y1 <= y0 {
        return false;
    }
    let source_pixels = &src.pixels;
    let destination_pixels = Arc::make_mut(&mut dst.pixels);
    let src_stride = src.width as usize * 4;
    let dst_stride = dst.width as usize * 4;
    let mut changed = false;
    let x_samples = native_scale_samples(src_width as usize, dst_width as usize);
    let y_samples = native_scale_samples(src_height as usize, dst_height as usize);
    for y in y0..y1 {
        let sy = i64::from(src_y) + y_samples[(y - dst_y) as usize] as i64;
        if sy < 0 || sy >= i64::from(src.height) {
            continue;
        }
        for x in x0..x1 {
            let sx = i64::from(src_x) + x_samples[(x - dst_x) as usize] as i64;
            if sx < 0 || sx >= i64::from(src.width) {
                continue;
            }
            let source = sy as usize * src_stride + sx as usize * 4;
            let destination = y as usize * dst_stride + x as usize * 4;
            destination_pixels[destination..destination + 3]
                .copy_from_slice(&source_pixels[source..source + 3]);
            if copy_alpha {
                destination_pixels[destination + 3] = source_pixels[source + 3];
            } else if !preserve_destination_alpha {
                destination_pixels[destination + 3] = 255;
            }
            changed = true;
        }
    }
    changed
}
#[allow(clippy::too_many_arguments)]
pub(super) fn hq_scale_copy_page_region(
    src: &Page,
    dst: &mut Page,
    src_x: i32,
    src_y: i32,
    src_width: i32,
    src_height: i32,
    dst_x: i32,
    dst_y: i32,
    dst_width: i32,
    dst_height: i32,
    scale_x: f32,
    scale_y: f32,
) -> bool {
    const ONE: i64 = 0x4000;
    const HALF: i64 = 0x2000;
    if src_width <= 0 || src_height <= 0 || dst_width <= 0 || dst_height <= 0
        || scale_x <= 0.0 || scale_y <= 0.0
        || src.indexed_samples.is_some() != dst.indexed_samples.is_some()
    {
        return false;
    }
    let source_left = i64::from(src_x).max(0);
    let source_top = i64::from(src_y).max(0);
    let source_right = i64::from(src_x)
        .saturating_add(i64::from(src_width))
        .min(i64::from(src.width));
    let source_bottom = i64::from(src_y)
        .saturating_add(i64::from(src_height))
        .min(i64::from(src.height));
    let mut destination_left = i64::from(dst_x).max(0);
    let mut destination_top = i64::from(dst_y).max(0);
    let destination_right = i64::from(dst_x)
        .saturating_add(i64::from(dst_width))
        .min(i64::from(dst.width));
    let destination_bottom = i64::from(dst_y)
        .saturating_add(i64::from(dst_height))
        .min(i64::from(dst.height));
    if source_right <= source_left || source_bottom <= source_top
        || destination_right <= destination_left || destination_bottom <= destination_top
    {
        return false;
    }
    let source_centre_x = i64::from(src_x.wrapping_add(src_width / 2));
    let source_centre_y = i64::from(src_y.wrapping_add(src_height / 2));
    let destination_centre_x = i64::from(dst_x.wrapping_add(dst_width / 2));
    let destination_centre_y = i64::from(dst_y.wrapping_add(dst_height / 2));
    let step_x = (16384.0 / f64::from(scale_x)).trunc() as i64;
    let step_y = (16384.0 / f64::from(scale_y)).trunc() as i64;
    let mut source_fixed_x = source_centre_x * ONE
        - step_x * (destination_centre_x - destination_left) + step_x / 2 - HALF;
    let mut source_fixed_y = source_centre_y * ONE
        - step_y * (destination_centre_y - destination_top) + step_y / 2 - HALF;
    let mut output_width = destination_right - destination_left;
    let mut output_height = destination_bottom - destination_top;
    while output_width > 0 && source_fixed_x < source_left * ONE {
        destination_left += 1;
        output_width -= 1;
        source_fixed_x += step_x;
    }
    let source_x_limit = source_right * ONE - ONE;
    while output_width > 0
        && source_fixed_x + step_x * (output_width - 1) >= source_x_limit
    {
        output_width -= 1;
    }
    while output_height > 0 && source_fixed_y < source_top * ONE {
        destination_top += 1;
        output_height -= 1;
        source_fixed_y += step_y;
    }
    let source_y_limit = source_bottom * ONE - ONE;
    while output_height > 0
        && source_fixed_y + step_y * (output_height - 1) >= source_y_limit
    {
        output_height -= 1;
    }
    if output_width <= 0 || output_height <= 0 {
        return false;
    }
    #[inline]
    fn horizontal(left: u8, right: u8, fixed: i64, unit_step: bool) -> u8 {
        if unit_step {
            return left;
        }
        let fraction = (fixed & 0x3fff) as u32;
        (((0x4000 - fraction) * u32::from(left) + fraction * u32::from(right)) >> 14)
            as u8
    }
    #[inline]
    fn vertical(top: u8, bottom: u8, fixed: i64) -> u8 {
        let weight = ((fixed >> 6) & 0xff) as u32;
        (((256 - weight) * u32::from(top) + (weight + 1) * u32::from(bottom)) >> 8) as u8
    }
    let unit_x = step_x == ONE;
    if src.indexed_samples.is_some() {
        let mut output = Vec::with_capacity((output_width * output_height) as usize);
        let mut fixed_y = source_fixed_y;
        for row in 0..output_height {
            let top_y = (fixed_y / ONE) as usize;
            let bottom_y = top_y + 1;
            let mut fixed_x = source_fixed_x;
            for column in 0..output_width {
                let left_x = (fixed_x / ONE) as usize;
                let right_x = left_x + 1;
                let top = horizontal(
                    native_8bit_sample(src, left_x, top_y),
                    native_8bit_sample(src, right_x, top_y),
                    fixed_x,
                    unit_x,
                );
                let bottom = horizontal(
                    native_8bit_sample(src, left_x, bottom_y),
                    native_8bit_sample(src, right_x, bottom_y),
                    fixed_x,
                    unit_x,
                );
                output
                    .push((
                        (destination_top + row) as usize * dst.width as usize
                            + (destination_left + column) as usize,
                        vertical(top, bottom, fixed_y),
                    ));
                fixed_x += step_x;
            }
            fixed_y += step_y;
        }
        let palette = dst.indexed_palette.clone();
        let samples = Arc::make_mut(
            dst.indexed_samples.as_mut().expect("indexed destination"),
        );
        let pixels = Arc::make_mut(&mut dst.pixels);
        for (index, sample) in output {
            samples[index] = sample;
            let rgba = palette
                .as_deref()
                .map_or(
                    [sample, sample, sample, 0xff],
                    |entries| { entries[sample as usize] },
                );
            pixels[index * 4..index * 4 + 4].copy_from_slice(&rgba);
        }
        return true;
    }
    let source_stride = src.width as usize * 4;
    let destination_stride = dst.width as usize * 4;
    let preserve_alpha = dst.alpha_masked;
    let pixels = Arc::make_mut(&mut dst.pixels);
    let mut fixed_y = source_fixed_y;
    for row in 0..output_height {
        let top_y = (fixed_y / ONE) as usize;
        let bottom_y = top_y + 1;
        let mut fixed_x = source_fixed_x;
        for column in 0..output_width {
            let left_x = (fixed_x / ONE) as usize;
            let right_x = left_x + 1;
            let target = (destination_top + row) as usize * destination_stride
                + (destination_left + column) as usize * 4;
            for channel in 0..3 {
                let top_left = src.pixels[top_y * source_stride + left_x * 4 + channel];
                let top_right = src
                    .pixels[top_y * source_stride + right_x * 4 + channel];
                let bottom_left = src
                    .pixels[bottom_y * source_stride + left_x * 4 + channel];
                let bottom_right = src
                    .pixels[bottom_y * source_stride + right_x * 4 + channel];
                let top = horizontal(top_left, top_right, fixed_x, unit_x);
                let bottom = horizontal(bottom_left, bottom_right, fixed_x, unit_x);
                pixels[target + channel] = vertical(top, bottom, fixed_y);
            }
            if !preserve_alpha {
                pixels[target + 3] = 0xff;
            }
            fixed_x += step_x;
        }
        fixed_y += step_y;
    }
    true
}
#[inline]
fn fixed_16_16_to_int(value: i32) -> i32 {
    value.wrapping_add(if value < 0 { 0xffff } else { 0 }) >> 16
}
#[allow(clippy::too_many_arguments)]
pub(super) fn transform_copy_page_region(
    src: &Page,
    dst: &mut Page,
    src_x: i32,
    src_y: i32,
    src_width: i32,
    src_height: i32,
    dst_x: i32,
    dst_y: i32,
    dst_width: i32,
    dst_height: i32,
    angle_degrees: f32,
    scale_x: f32,
    scale_y: f32,
    source_has_alpha: bool,
    destination_has_alpha: bool,
) -> bool {
    if f64::from(scale_x).abs() <= 0.002 || f64::from(scale_y).abs() <= 0.002
        || src_width <= 0 || src_height <= 0 || dst_width <= 0 || dst_height <= 0
    {
        return false;
    }
    let source_left = src_x.max(0);
    let source_top = src_y.max(0);
    let source_right = src_x
        .saturating_add(src_width)
        .min(src.width as i32)
        .max(source_left);
    let source_bottom = src_y
        .saturating_add(src_height)
        .min(src.height as i32)
        .max(source_top);
    let destination_left = dst_x.max(0);
    let destination_top = dst_y.max(0);
    let destination_right = dst_x
        .saturating_add(dst_width)
        .min(dst.width as i32)
        .max(destination_left);
    let destination_bottom = dst_y
        .saturating_add(dst_height)
        .min(dst.height as i32)
        .max(destination_top);
    if source_right <= source_left || source_bottom <= source_top
        || destination_right <= destination_left || destination_bottom <= destination_top
    {
        return false;
    }
    let source_centre_x = src_x.wrapping_add(src_width / 2);
    let source_centre_y = src_y.wrapping_add(src_height / 2);
    let destination_centre_x = dst_x.wrapping_add(dst_width / 2);
    let destination_centre_y = dst_y.wrapping_add(dst_height / 2);
    let radians = -f64::from(angle_degrees) * std::f64::consts::PI / 180.0;
    let (sin, cos) = radians.sin_cos();
    let coefficient_xx = (cos * 65536.0 / f64::from(scale_x)).trunc() as i32;
    let coefficient_xy = (sin * 65536.0 / f64::from(scale_y)).trunc() as i32;
    let coefficient_yx = (-sin * 65536.0 / f64::from(scale_x)).trunc() as i32;
    let coefficient_yy = (cos * 65536.0 / f64::from(scale_y)).trunc() as i32;
    let source_min_x = source_left.wrapping_shl(16);
    let source_min_y = source_top.wrapping_shl(16);
    let source_max_x = source_right.wrapping_shl(16);
    let source_max_y = source_bottom.wrapping_shl(16);
    if let (Some(_), Some(dst_samples)) = (
        &src.indexed_samples,
        &mut dst.indexed_samples,
    ) {
        let palette = dst.indexed_palette.clone();
        let dst_stride = dst.width as usize;
        let samples = Arc::make_mut(dst_samples);
        let pixels = Arc::make_mut(&mut dst.pixels);
        let mut changed = false;
        for y in destination_top..destination_bottom {
            let relative_y = y.wrapping_sub(destination_centre_y);
            for x in destination_left..destination_right {
                let relative_x = x.wrapping_sub(destination_centre_x);
                let source_fixed_x = source_centre_x
                    .wrapping_shl(16)
                    .wrapping_add(relative_x.wrapping_mul(coefficient_xx))
                    .wrapping_add(relative_y.wrapping_mul(coefficient_yx));
                let source_fixed_y = source_centre_y
                    .wrapping_shl(16)
                    .wrapping_add(relative_x.wrapping_mul(coefficient_xy))
                    .wrapping_add(relative_y.wrapping_mul(coefficient_yy));
                if source_fixed_x < source_min_x || source_fixed_y < source_min_y
                    || source_fixed_x >= source_max_x || source_fixed_y >= source_max_y
                {
                    continue;
                }
                let sx = fixed_16_16_to_int(source_fixed_x) as usize;
                let sy = fixed_16_16_to_int(source_fixed_y) as usize;
                let sample = native_8bit_sample(src, sx, sy);
                let target = y as usize * dst_stride + x as usize;
                samples[target] = sample;
                let rgba = palette
                    .as_deref()
                    .map_or(
                        [sample, sample, sample, 0xff],
                        |entries| { entries[sample as usize] },
                    );
                pixels[target * 4..target * 4 + 4].copy_from_slice(&rgba);
                changed = true;
            }
        }
        return changed;
    }
    let source_pixels = &src.pixels;
    let destination_pixels = Arc::make_mut(&mut dst.pixels);
    let src_stride = src.width as usize * 4;
    let dst_stride = dst.width as usize * 4;
    let mut changed = false;
    for y in destination_top..destination_bottom {
        let relative_y = y.wrapping_sub(destination_centre_y);
        for x in destination_left..destination_right {
            let relative_x = x.wrapping_sub(destination_centre_x);
            let source_fixed_x = source_centre_x
                .wrapping_shl(16)
                .wrapping_add(relative_x.wrapping_mul(coefficient_xx))
                .wrapping_add(relative_y.wrapping_mul(coefficient_yx));
            let source_fixed_y = source_centre_y
                .wrapping_shl(16)
                .wrapping_add(relative_x.wrapping_mul(coefficient_xy))
                .wrapping_add(relative_y.wrapping_mul(coefficient_yy));
            if source_fixed_x < source_min_x || source_fixed_y < source_min_y
                || source_fixed_x >= source_max_x || source_fixed_y >= source_max_y
            {
                continue;
            }
            let sx = fixed_16_16_to_int(source_fixed_x) as usize;
            let sy = fixed_16_16_to_int(source_fixed_y) as usize;
            let source = sy * src_stride + sx * 4;
            let target = y as usize * dst_stride + x as usize * 4;
            destination_pixels[target..target + 3]
                .copy_from_slice(&source_pixels[source..source + 3]);
            destination_pixels[target + 3] = if destination_has_alpha && source_has_alpha
            {
                source_pixels[source + 3]
            } else {
                255
            };
            changed = true;
        }
    }
    changed
}
#[inline]
fn msvc_rand(state: &mut u32) -> i32 {
    *state = state.wrapping_mul(214_013).wrapping_add(2_531_011);
    ((*state >> 16) & 0x7fff) as i32
}
#[allow(clippy::too_many_arguments)]
pub(super) fn mosaic_page_region(
    src: &Page,
    dst: &mut Page,
    src_x: i32,
    src_y: i32,
    width: i32,
    height: i32,
    dst_x: i32,
    dst_y: i32,
    block_size: i32,
    rng_state: &mut u32,
) -> bool {
    if block_size <= 0 || src.indexed_samples.is_some() || dst.indexed_samples.is_some()
    {
        return false;
    }
    let Some((sx, sy, dx, dy, width, height)) = clipped_copy_rects(
        src,
        dst,
        src_x,
        src_y,
        width.max(1),
        height.max(1),
        dst_x,
        dst_y,
    ) else {
        return false;
    };
    let (sx, sy, dx, dy, width, height) = (
        sx as i32,
        sy as i32,
        dx as i32,
        dy as i32,
        width as i32,
        height as i32,
    );
    let source_pixels = &src.pixels;
    let preserve_materialized_alpha = dst.alpha_masked;
    let destination_pixels = Arc::make_mut(&mut dst.pixels);
    let src_stride = src.width as usize * 4;
    let dst_stride = dst.width as usize * 4;
    let mut block_x = dx + (width % block_size) / 2 - block_size;
    let mut changed = false;
    while block_x < dx + width {
        let mut block_y = dy + (height % block_size) / 2 - block_size;
        while block_y < dy + height {
            let clipped_x = block_x.max(dx);
            let clipped_y = block_y.max(dy);
            let clipped_w = block_x
                .wrapping_add(block_size)
                .min(dx + width)
                .wrapping_sub(clipped_x);
            let clipped_h = block_y
                .wrapping_add(block_size)
                .min(dy + height)
                .wrapping_sub(clipped_y);
            if clipped_w > 0 && clipped_h > 0 {
                let sample_base_x = sx + clipped_x - dx;
                let sample_base_y = sy + clipped_y - dy;
                let mut sums = [0_u32; 3];
                for _ in 0..3 {
                    let sample_y = sample_base_y + msvc_rand(rng_state) % clipped_h;
                    let sample_x = sample_base_x + msvc_rand(rng_state) % clipped_w;
                    let index = sample_y as usize * src_stride + sample_x as usize * 4;
                    sums[0] += u32::from(source_pixels[index]);
                    sums[1] += u32::from(source_pixels[index + 1]);
                    sums[2] += u32::from(source_pixels[index + 2]);
                }
                let colour = [
                    (sums[0] / 3) as u8,
                    (sums[1] / 3) as u8,
                    (sums[2] / 3) as u8,
                ];
                for y in clipped_y..clipped_y + clipped_h {
                    let start = y as usize * dst_stride + clipped_x as usize * 4;
                    let end = start + clipped_w as usize * 4;
                    if preserve_materialized_alpha {
                        for pixel in destination_pixels[start..end].chunks_exact_mut(4) {
                            pixel[..3].copy_from_slice(&colour);
                        }
                    } else {
                        let rgba = [colour[0], colour[1], colour[2], 0xFF];
                        for pixel in destination_pixels[start..end].chunks_exact_mut(4) {
                            pixel.copy_from_slice(&rgba);
                        }
                    }
                }
                changed = true;
            }
            block_y = block_y.wrapping_add(block_size);
        }
        block_x = block_x.wrapping_add(block_size);
    }
    changed
}
fn native_scale_samples(source: usize, destination: usize) -> Vec<usize> {
    debug_assert!(source > 0 && destination > 0);
    if source == destination {
        return (0..source).collect();
    }
    let mut samples = Vec::with_capacity(destination);
    if destination < source {
        let mut source_index = -1_i64;
        let mut error = (source / 2) as i64;
        for _ in 0..destination {
            loop {
                source_index += 1;
                error -= destination as i64;
                if error < 0 {
                    break;
                }
            }
            samples.push(source_index as usize);
            error += source as i64;
        }
    } else {
        let mut source_index = 0_usize;
        let mut error = destination as i64 - 1;
        samples.push(source_index);
        for _ in 1..destination {
            error -= source as i64;
            if error < 0 {
                error += destination as i64;
                source_index += 1;
            }
            samples.push(source_index.min(source - 1));
        }
    }
    samples
}

pub(super) fn blit_decoded_page(
    src: &Page,
    dst: &mut Page,
    dst_x: i32,
    dst_y: i32,
) -> bool {
    let changed = if src.alpha_masked {
        blend_page_region_over(
            src,
            dst,
            0,
            0,
            src.width as i32,
            src.height as i32,
            dst_x,
            dst_y,
            255,
            true,
            dst.alpha_masked,
        )
    } else {
        copy_page_region(
            src,
            dst,
            0,
            0,
            src.width as i32,
            src.height as i32,
            dst_x,
            dst_y,
        )
    };
    if !changed {
        return false;
    }
    if !src.alpha_masked && dst.indexed_samples.is_some() {
        if let (Some(source_samples), Some((sx, sy, dx, dy, width, height))) = (
            src.indexed_samples.as_deref(),
            clipped_copy_rects(
                src,
                dst,
                0,
                0,
                src.width as i32,
                src.height as i32,
                dst_x,
                dst_y,
            ),
        ) {
            let mut destination_samples = dst
                .indexed_samples
                .as_deref()
                .cloned()
                .unwrap();
            for row in 0..height {
                let source_start = (sy + row) * src.width as usize + sx;
                let destination_start = (dy + row) * dst.width as usize + dx;
                destination_samples[destination_start..destination_start + width]
                    .copy_from_slice(
                        &source_samples[source_start..source_start + width],
                    );
            }
            dst.indexed_samples = Some(Arc::new(destination_samples));
        }
    }
    true
}
#[allow(clippy::too_many_arguments)]
pub(super) fn blend_page_region_over(
    src: &Page,
    dst: &mut Page,
    src_x: i32,
    src_y: i32,
    width: i32,
    height: i32,
    dst_x: i32,
    dst_y: i32,
    opacity: u8,
    use_source_alpha: bool,
    destination_has_alpha: bool,
) -> bool {
    if width <= 0 || height <= 0 {
        return false;
    }
    let (mut sx, mut sy, mut dx, mut dy) = (
        src_x as i64,
        src_y as i64,
        dst_x as i64,
        dst_y as i64,
    );
    let (mut w, mut h) = (width as i64, height as i64);
    if sx < 0 {
        let clipped = -sx;
        sx = 0;
        dx += clipped;
        w -= clipped;
    }
    if sy < 0 {
        let clipped = -sy;
        sy = 0;
        dy += clipped;
        h -= clipped;
    }
    if dx < 0 {
        let clipped = -dx;
        dx = 0;
        sx += clipped;
        w -= clipped;
    }
    if dy < 0 {
        let clipped = -dy;
        dy = 0;
        sy += clipped;
        h -= clipped;
    }
    w = w.min(src.width as i64 - sx).min(dst.width as i64 - dx);
    h = h.min(src.height as i64 - sy).min(dst.height as i64 - dy);
    if w <= 0 || h <= 0 {
        return false;
    }
    let src_stride = src.width as usize * 4;
    let dst_stride = dst.width as usize * 4;
    let dst_pixels = Arc::make_mut(&mut dst.pixels);
    let mut changed = false;
    for row in 0..h as usize {
        for column in 0..w as usize {
            let source = (sy as usize + row) * src_stride + (sx as usize + column) * 4;
            let target = (dy as usize + row) * dst_stride + (dx as usize + column) * 4;
            let global_anti = 255_u16 - u16::from(opacity);
            let source_anti = 255_u16 - u16::from(src.pixels[source + 3]);
            let blend_anti = if use_source_alpha {
                global_anti + source_anti + 1
            } else if global_anti == 0 {
                1
            } else {
                global_anti + 1
            };
            let color_visible = !use_source_alpha || blend_anti < 255;
            if color_visible {
                for channel in 0..3 {
                    dst_pixels[target + channel] = if use_source_alpha && blend_anti <= 2
                    {
                        src.pixels[source + channel]
                    } else {
                        let source_weight = 257 - blend_anti;
                        ((blend_anti * u16::from(dst_pixels[target + channel])
                            + source_weight * u16::from(src.pixels[source + channel]))
                            >> 8) as u8
                    };
                }
            }
            if destination_has_alpha {
                let destination_anti = 255_u16 - u16::from(dst_pixels[target + 3]);
                let output_anti = if use_source_alpha {
                    (destination_anti * (source_anti + 1)) >> 8
                } else if global_anti == 0 {
                    0
                } else {
                    (destination_anti * global_anti) >> 8
                };
                let output_alpha = (255 - output_anti) as u8;
                changed |= output_alpha != dst_pixels[target + 3];
                dst_pixels[target + 3] = output_alpha;
            } else if color_visible {
                dst_pixels[target + 3] = 255;
            }
            changed |= color_visible;
        }
    }
    changed
}
#[allow(clippy::too_many_arguments)]
pub(super) fn blend_page_region_additive(
    src: &Page,
    dst: &mut Page,
    src_x: i32,
    src_y: i32,
    width: i32,
    height: i32,
    dst_x: i32,
    dst_y: i32,
    opacity: u8,
) -> bool {
    if width <= 0 || height <= 0 || opacity == 0 {
        return false;
    }
    let (mut sx, mut sy, mut dx, mut dy) = (
        src_x as i64,
        src_y as i64,
        dst_x as i64,
        dst_y as i64,
    );
    let (mut w, mut h) = (width as i64, height as i64);
    if sx < 0 {
        let clipped = -sx;
        sx = 0;
        dx += clipped;
        w -= clipped;
    }
    if sy < 0 {
        let clipped = -sy;
        sy = 0;
        dy += clipped;
        h -= clipped;
    }
    if dx < 0 {
        let clipped = -dx;
        dx = 0;
        sx += clipped;
        w -= clipped;
    }
    if dy < 0 {
        let clipped = -dy;
        dy = 0;
        sy += clipped;
        h -= clipped;
    }
    w = w.min(src.width as i64 - sx).min(dst.width as i64 - dx);
    h = h.min(src.height as i64 - sy).min(dst.height as i64 - dy);
    if w <= 0 || h <= 0 {
        return false;
    }
    let src_stride = src.width as usize * 4;
    let dst_stride = dst.width as usize * 4;
    let dst_pixels = Arc::make_mut(&mut dst.pixels);
    for row in 0..h as usize {
        for column in 0..w as usize {
            let source = (sy as usize + row) * src_stride + (sx as usize + column) * 4;
            let target = (dy as usize + row) * dst_stride + (dx as usize + column) * 4;
            for channel in 0..3 {
                let contribution = if opacity == 255 {
                    u16::from(src.pixels[source + channel])
                } else {
                    (u16::from(src.pixels[source + channel]) * u16::from(opacity)) >> 8
                };
                dst_pixels[target + channel] = (u16::from(dst_pixels[target + channel])
                    + contribution)
                    .min(255) as u8;
            }
        }
    }
    true
}
#[allow(clippy::too_many_arguments)]
pub(super) fn blend_page_region_multiply(
    src: &Page,
    dst: &mut Page,
    src_x: i32,
    src_y: i32,
    width: i32,
    height: i32,
    dst_x: i32,
    dst_y: i32,
    opacity: u8,
) -> bool {
    if width <= 0 || height <= 0 {
        return false;
    }
    let (mut sx, mut sy, mut dx, mut dy) = (
        src_x as i64,
        src_y as i64,
        dst_x as i64,
        dst_y as i64,
    );
    let (mut w, mut h) = (width as i64, height as i64);
    if sx < 0 {
        let clipped = -sx;
        sx = 0;
        dx += clipped;
        w -= clipped;
    }
    if sy < 0 {
        let clipped = -sy;
        sy = 0;
        dy += clipped;
        h -= clipped;
    }
    if dx < 0 {
        let clipped = -dx;
        dx = 0;
        sx += clipped;
        w -= clipped;
    }
    if dy < 0 {
        let clipped = -dy;
        dy = 0;
        sy += clipped;
        h -= clipped;
    }
    w = w.min(src.width as i64 - sx).min(dst.width as i64 - dx);
    h = h.min(src.height as i64 - sy).min(dst.height as i64 - dy);
    if w <= 0 || h <= 0 {
        return false;
    }
    let destination_weight = 256_u32 - u32::from(opacity);
    let multiply_weight = u32::from(opacity);
    let src_stride = src.width as usize * 4;
    let dst_stride = dst.width as usize * 4;
    let dst_pixels = Arc::make_mut(&mut dst.pixels);
    for row in 0..h as usize {
        for column in 0..w as usize {
            let source = (sy as usize + row) * src_stride + (sx as usize + column) * 4;
            let target = (dy as usize + row) * dst_stride + (dx as usize + column) * 4;
            for channel in 0..3 {
                let source_value = u32::from(src.pixels[source + channel]);
                let destination_value = u32::from(dst_pixels[target + channel]);
                let multiplied = (destination_value * (source_value + 1)) >> 8;
                dst_pixels[target + channel] = ((destination_weight * destination_value
                    + multiply_weight * multiplied) >> 8) as u8;
            }
        }
    }
    true
}
#[allow(clippy::too_many_arguments)]
pub(super) fn blend_page_region_lighten(
    src: &Page,
    dst: &mut Page,
    src_x: i32,
    src_y: i32,
    width: i32,
    height: i32,
    dst_x: i32,
    dst_y: i32,
    opacity: u8,
) -> bool {
    if width <= 0 || height <= 0 || opacity == 0 {
        return false;
    }
    let (mut sx, mut sy, mut dx, mut dy) = (
        src_x as i64,
        src_y as i64,
        dst_x as i64,
        dst_y as i64,
    );
    let (mut w, mut h) = (width as i64, height as i64);
    if sx < 0 {
        let clipped = -sx;
        sx = 0;
        dx += clipped;
        w -= clipped;
    }
    if sy < 0 {
        let clipped = -sy;
        sy = 0;
        dy += clipped;
        h -= clipped;
    }
    if dx < 0 {
        let clipped = -dx;
        dx = 0;
        sx += clipped;
        w -= clipped;
    }
    if dy < 0 {
        let clipped = -dy;
        dy = 0;
        sy += clipped;
        h -= clipped;
    }
    w = w.min(src.width as i64 - sx).min(dst.width as i64 - dx);
    h = h.min(src.height as i64 - sy).min(dst.height as i64 - dy);
    if w <= 0 || h <= 0 {
        return false;
    }
    let maximum_weight = u32::from(opacity) + 1;
    let destination_weight = 255_u32 - u32::from(opacity);
    let src_stride = src.width as usize * 4;
    let dst_stride = dst.width as usize * 4;
    let dst_pixels = Arc::make_mut(&mut dst.pixels);
    for row in 0..h as usize {
        for column in 0..w as usize {
            let source = (sy as usize + row) * src_stride + (sx as usize + column) * 4;
            let target = (dy as usize + row) * dst_stride + (dx as usize + column) * 4;
            for channel in 0..3 {
                let source_value = u32::from(src.pixels[source + channel]);
                let destination_value = u32::from(dst_pixels[target + channel]);
                let maximum = source_value.max(destination_value);
                dst_pixels[target + channel] = ((maximum_weight * maximum
                    + destination_weight * destination_value) >> 8) as u8;
            }
        }
    }
    true
}
#[allow(clippy::too_many_arguments)]
pub(super) fn copy_page_region_with_alpha_view(
    src: &Page,
    dst: &mut Page,
    src_x: i32,
    src_y: i32,
    width: i32,
    height: i32,
    dst_x: i32,
    dst_y: i32,
    src_alpha: bool,
    dst_alpha: bool,
) -> bool {
    if width <= 0 || height <= 0 {
        return false;
    }
    let (mut sx, mut sy, mut dx, mut dy) = (
        src_x as i64,
        src_y as i64,
        dst_x as i64,
        dst_y as i64,
    );
    let (mut w, mut h) = (width as i64, height as i64);
    if sx < 0 {
        let clipped = -sx;
        sx = 0;
        dx += clipped;
        w -= clipped;
    }
    if sy < 0 {
        let clipped = -sy;
        sy = 0;
        dy += clipped;
        h -= clipped;
    }
    if dx < 0 {
        let clipped = -dx;
        dx = 0;
        sx += clipped;
        w -= clipped;
    }
    if dy < 0 {
        let clipped = -dy;
        dy = 0;
        sy += clipped;
        h -= clipped;
    }
    w = w.min(src.width as i64 - sx).min(dst.width as i64 - dx);
    h = h.min(src.height as i64 - sy).min(dst.height as i64 - dy);
    if w <= 0 || h <= 0 {
        return false;
    }
    let src_stride = src.width as usize * 4;
    let dst_stride = dst.width as usize * 4;
    let src_pixels = src.pixels.clone();
    let dst_pixels = Arc::make_mut(&mut dst.pixels);
    for row in 0..h as usize {
        for column in 0..w as usize {
            let src_offset = (sy as usize + row) * src_stride
                + (sx as usize + column) * 4;
            let dst_offset = (dy as usize + row) * dst_stride
                + (dx as usize + column) * 4;
            if src_alpha {
                let native_alpha = 255 - src_pixels[src_offset + 3];
                if dst_alpha {
                    dst_pixels[dst_offset + 3] = 255 - native_alpha;
                } else {
                    dst_pixels[dst_offset..dst_offset + 3].fill(native_alpha);
                    dst_pixels[dst_offset + 3] = 0xFF;
                }
            } else if dst_alpha {
                let native_alpha = native_8bit_sample(
                    src,
                    sx as usize + column,
                    sy as usize + row,
                );
                dst_pixels[dst_offset + 3] = 255 - native_alpha;
            }
        }
    }
    true
}
#[allow(clippy::too_many_arguments)]
pub(super) fn multiply_alpha_region(
    src: &Page,
    dst: &mut Page,
    src_x: i32,
    src_y: i32,
    width: i32,
    height: i32,
    dst_x: i32,
    dst_y: i32,
    src_is_rgba_view: bool,
    dst_is_rgba_view: bool,
) -> bool {
    if width <= 0 || height <= 0 {
        return false;
    }
    let (mut sx, mut sy, mut dx, mut dy) = (
        src_x as i64,
        src_y as i64,
        dst_x as i64,
        dst_y as i64,
    );
    let (mut w, mut h) = (width as i64, height as i64);
    if sx < 0 {
        let clipped = -sx;
        sx = 0;
        dx += clipped;
        w -= clipped;
    }
    if sy < 0 {
        let clipped = -sy;
        sy = 0;
        dy += clipped;
        h -= clipped;
    }
    if dx < 0 {
        let clipped = -dx;
        dx = 0;
        sx += clipped;
        w -= clipped;
    }
    if dy < 0 {
        let clipped = -dy;
        dy = 0;
        sy += clipped;
        h -= clipped;
    }
    w = w.min(src.width as i64 - sx).min(dst.width as i64 - dx);
    h = h.min(src.height as i64 - sy).min(dst.height as i64 - dy);
    if w <= 0 || h <= 0 {
        return false;
    }
    let src_stride = src.width as usize * 4;
    let dst_stride = dst.width as usize * 4;
    let src_pixels = src.pixels.clone();
    let mut dst_sample_values = dst.indexed_samples.as_deref().cloned();
    let dst_pixels = Arc::make_mut(&mut dst.pixels);
    for row in 0..h as usize {
        for column in 0..w as usize {
            let source = (sy as usize + row) * src_stride + (sx as usize + column) * 4;
            let target = (dy as usize + row) * dst_stride + (dx as usize + column) * 4;
            let src_alpha = if src_is_rgba_view {
                255 - src_pixels[source + 3]
            } else {
                native_8bit_sample(src, sx as usize + column, sy as usize + row)
            };
            let dst_alpha = if dst_is_rgba_view {
                255 - dst_pixels[target + 3]
            } else {
                dst_sample_values
                    .as_ref()
                    .map(|samples| {
                        samples[(dy as usize + row) * dst.width as usize + dx as usize
                            + column]
                    })
                    .unwrap_or(dst_pixels[target])
            };
            let multiplied = ((u16::from(dst_alpha) * (u16::from(src_alpha) + 1)) >> 8)
                as u8;
            if dst_is_rgba_view {
                dst_pixels[target + 3] = 255 - multiplied;
            } else {
                dst_pixels[target..target + 3].fill(multiplied);
                dst_pixels[target + 3] = 255;
                if let Some(samples) = dst_sample_values.as_mut() {
                    samples[(dy as usize + row) * dst.width as usize + dx as usize
                        + column] = multiplied;
                }
            }
        }
    }
    if let Some(samples) = dst_sample_values {
        dst.indexed_samples = Some(Arc::new(samples));
    }
    true
}
pub(super) fn reprioritize_sprite(
    sprites: &mut HashMap<u32, Sprite>,
    order: &mut Vec<u32>,
    sprite: u32,
    reference: Option<u32>,
    high: bool,
    grouped: bool,
) -> bool {
    let Some(sprite_index) = order.iter().position(|handle| *handle == sprite) else {
        return false;
    };
    if reference == Some(sprite) {
        return false;
    }
    let reference_group = match reference {
        Some(reference) => {
            if !order.contains(&reference) {
                return false;
            }
            Some(sprites.get(&reference).map_or(reference, |value| value.group_id))
        }
        None => None,
    };
    order.remove(sprite_index);
    let insertion = match reference {
        None if high => order.len(),
        None => 0,
        Some(reference) => {
            let reference_index = order
                .iter()
                .position(|handle| *handle == reference)
                .expect("reference validated before detach");
            let group = reference_group.expect("reference group exists");
            if high {
                let mut insertion = reference_index + 1;
                if grouped {
                    while insertion < order.len()
                        && sprites
                            .get(&order[insertion])
                            .is_some_and(|value| value.group_id == group)
                    {
                        insertion += 1;
                    }
                }
                insertion
            } else {
                let mut insertion = reference_index;
                if grouped {
                    while insertion > 0
                        && sprites
                            .get(&order[insertion - 1])
                            .is_some_and(|value| value.group_id == group)
                    {
                        insertion -= 1;
                    }
                }
                insertion
            }
        }
    };
    order.insert(insertion, sprite);
    if let (Some(group), Some(value)) = (reference_group, sprites.get_mut(&sprite)) {
        value.group_id = group;
    }
    true
}
