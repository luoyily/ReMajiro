use super::{
    Page, Sprite, SpriteAlphaAnimation, SpriteFrameAnimation, SpritePositionAnimation,
    SpriteRotationAnimation, SpriteScaleAnimation, SpriteSource,
};
#[allow(clippy::approx_constant)]
pub(super) fn advance_position_animation(
    position: &mut Option<(i32, i32)>,
    animation: &mut SpritePositionAnimation,
    now_ms: i32,
) -> bool {
    if animation.state != 1 {
        return false;
    }
    let duration = animation.duration_ms;
    let mut elapsed = now_ms.wrapping_sub(animation.started_ms);
    if elapsed >= duration {
        elapsed = duration;
        animation.state = 2;
    }
    let axis = |start: i32, delta: i32, ease_out: i32, ease_in: i32| -> i32 {
        if duration == 0 {
            return start.wrapping_add(delta);
        }
        let duration_f = f64::from(duration);
        let elapsed_f = f64::from(elapsed);
        let delta_f = f64::from(delta);
        if animation.easing_flags & ease_out != 0 {
            let phase = (duration_f - elapsed_f) * 3.1415926535 / duration_f * 0.5;
            (f64::from(start) + delta_f - phase.sin() * delta_f) as i32
        } else if animation.easing_flags & ease_in != 0 {
            let phase = elapsed_f * 3.1415926535 / duration_f * 0.5;
            (f64::from(start) + phase.sin() * delta_f) as i32
        } else {
            (delta_f * elapsed_f / duration_f + f64::from(start)) as i32
        }
    };
    *position = Some((
        axis(animation.start.0, animation.delta.0, 0x4000_0000, 0x2000_0000),
        axis(animation.start.1, animation.delta.1, 0x1000_0000, 0x0800_0000),
    ));
    animation.state == 1
}
pub(super) fn native_8bit_sample(page: &Page, x: usize, y: usize) -> u8 {
    page.indexed_samples
        .as_ref()
        .and_then(|samples| samples.get(y * page.width as usize + x))
        .copied()
        .unwrap_or(page.pixels[(y * page.width as usize + x) * 4])
}
pub(super) fn sprite_source_rect(
    sprite: &Sprite,
    page: &Page,
) -> Option<(f32, f32, f32, f32)> {
    let Some(source) = sprite.source else {
        return Some((0.0, 0.0, page.width as f32, page.height as f32));
    };
    let x0 = source.x.clamp(0, page.width as i32);
    let y0 = source.y.clamp(0, page.height as i32);
    let x1 = source.x.saturating_add(source.width).clamp(0, page.width as i32);
    let y1 = source.y.saturating_add(source.height).clamp(0, page.height as i32);
    (x1 > x0 && y1 > y0)
        .then(|| (x0 as f32, y0 as f32, (x1 - x0) as f32, (y1 - y0) as f32))
}
pub(super) fn advance_alpha_animation(
    animation: &mut SpriteAlphaAnimation,
    now_ms: i32,
) -> (u8, bool) {
    if animation.keyframes.is_empty() {
        return (animation.previous_transparency, true);
    }
    let mut elapsed = now_ms.wrapping_sub(animation.started_ms).wrapping_abs();
    if elapsed >= animation.segment_duration_ms {
        for _ in 0..100 {
            elapsed = elapsed.wrapping_sub(animation.segment_duration_ms);
            if animation.segment_index >= 0 {
                animation.previous_transparency = animation
                    .keyframes[animation.segment_index as usize]
                    .0;
            }
            animation.segment_index += 1;
            if animation.segment_index as usize >= animation.keyframes.len() {
                animation.segment_index = 0;
            }
            animation.segment_duration_ms = animation
                .keyframes[animation.segment_index as usize]
                .1;
            if animation.segment_duration_ms < 0 {
                let target = animation.keyframes[animation.segment_index as usize].0;
                animation.started_ms = now_ms;
                return (target, true);
            }
            if elapsed < animation.segment_duration_ms {
                break;
            }
        }
        animation.started_ms = now_ms.wrapping_sub(elapsed);
    }
    let (target, duration) = animation.keyframes[animation.segment_index as usize];
    if duration <= 0 {
        return (target, false);
    }
    let value = elapsed
        .wrapping_mul(i32::from(target))
        .wrapping_add(
            duration
                .wrapping_sub(elapsed)
                .wrapping_mul(i32::from(animation.previous_transparency)),
        ) / duration;
    (value.clamp(0, 255) as u8, false)
}
pub(super) fn advance_frame_animation(
    animation: &mut SpriteFrameAnimation,
    now_ms: i32,
) -> (Option<i32>, bool) {
    if animation.state == 0 || animation.keyframes.is_empty() {
        return (None, false);
    }
    let mut elapsed = now_ms.wrapping_sub(animation.started_ms).wrapping_abs();
    if animation.segment_duration_ms > elapsed {
        return (None, true);
    }
    for _ in 0..100 {
        elapsed = elapsed.wrapping_sub(animation.segment_duration_ms);
        animation.segment_index += 1;
        if animation.segment_index as usize >= animation.keyframes.len() {
            animation.segment_index = 0;
            animation.state = 2;
        }
        animation.segment_duration_ms = animation
            .keyframes[animation.segment_index as usize]
            .1;
        if animation.segment_duration_ms < 0 {
            animation.state = 0;
            elapsed = 0;
            break;
        }
        if elapsed < animation.segment_duration_ms {
            break;
        }
    }
    animation.started_ms = now_ms.wrapping_sub(elapsed);
    (Some(animation.keyframes[animation.segment_index as usize].0), animation.state != 0)
}
pub(super) fn advance_rotation_animation(
    animation: &mut SpriteRotationAnimation,
    now_ms: i32,
) -> bool {
    if animation.state == 0 {
        return false;
    }
    let mut elapsed = now_ms.wrapping_sub(animation.started_ms).wrapping_abs();
    if animation.state == 1 && elapsed >= animation.duration_ms {
        elapsed = animation.duration_ms;
        animation.state = 0;
    }
    animation.current = (((f64::from(animation.target) - f64::from(animation.previous))
        / f64::from(animation.duration_ms)) * f64::from(elapsed)
        + f64::from(animation.previous)) as f32;
    animation.state != 0
}
pub(super) fn advance_scale_animation(
    animation: &mut SpriteScaleAnimation,
    now_ms: i32,
) -> bool {
    if animation.state == 0 || animation.keyframes.is_empty() {
        return false;
    }
    let mut elapsed = now_ms.wrapping_sub(animation.started_ms).wrapping_abs();
    if elapsed >= animation.segment_duration_ms {
        for _ in 0..100 {
            elapsed = elapsed.wrapping_sub(animation.segment_duration_ms);
            if animation.segment_index >= 0 {
                animation.previous = animation
                    .keyframes[animation.segment_index as usize]
                    .0;
            }
            animation.segment_index += 1;
            if animation.segment_index as usize >= animation.keyframes.len() {
                animation.segment_index = 0;
                animation.state = 2;
            }
            let raw_duration = animation.keyframes[animation.segment_index as usize].1;
            if raw_duration < 0 {
                elapsed = 0;
                animation.state = 0;
                break;
            }
            animation.segment_duration_ms = (raw_duration as u32 & 0x84FF_FFFF) as i32;
            animation.easing_flags = (raw_duration as u32 & 0x7800_0000) as i32;
            if elapsed < animation.segment_duration_ms {
                break;
            }
        }
        animation.started_ms = now_ms.wrapping_sub(elapsed);
    }
    let (target, raw_duration) = animation.keyframes[animation.segment_index as usize];
    animation.current = if raw_duration <= 0 || animation.segment_duration_ms == 0 {
        target
    } else {
        let duration = animation.segment_duration_ms;
        let elapsed = elapsed.clamp(0, duration);
        let delta = f64::from(target) - f64::from(animation.previous);
        if animation.easing_flags & animation.ease_out_mask != 0 {
            let phase = f64::from(duration.wrapping_sub(elapsed)) * std::f64::consts::PI
                / f64::from(duration) * 0.5;
            (f64::from(animation.previous) + delta - phase.sin() * delta) as f32
        } else if animation.easing_flags & animation.ease_in_mask != 0 {
            let phase = f64::from(elapsed) * std::f64::consts::PI / f64::from(duration)
                * 0.5;
            (f64::from(animation.previous) + phase.sin() * delta) as f32
        } else {
            ((f64::from(duration.wrapping_sub(elapsed)) * f64::from(animation.previous)
                + f64::from(elapsed) * f64::from(target)) / f64::from(duration)) as f32
        }
    };
    animation.state != 0
}
pub(super) fn scale_set_animation(
    current: f32,
    target: f32,
    duration_ms: i32,
    now_ms: i32,
    ease_out_mask: i32,
    ease_in_mask: i32,
) -> SpriteScaleAnimation {
    SpriteScaleAnimation {
        state: i32::from(duration_ms != 0),
        current: if duration_ms == 0 { target } else { current },
        started_ms: now_ms,
        segment_duration_ms: 0,
        segment_index: -1,
        previous: current,
        easing_flags: 0,
        ease_out_mask,
        ease_in_mask,
        keyframes: vec![(target, duration_ms), (target, - 1)],
    }
}
pub(super) fn source_for_animation_frame(
    sprite: &Sprite,
    page: &Page,
    frame: i32,
) -> SpriteSource {
    let mut source = sprite
        .source
        .unwrap_or(SpriteSource {
            x: sprite.original_source_position.0,
            y: sprite.original_source_position.1,
            width: page.width as i32,
            height: page.height as i32,
        });
    if frame == -2 {
        source.x = sprite.original_source_position.0;
        source.y = sprite.original_source_position.1;
        return source;
    }
    let columns = if source.width > 0 {
        (page.width as i32 / source.width).max(1)
    } else {
        1
    };
    source.x = source.width.wrapping_mul(frame % columns);
    source.y = source.height.wrapping_mul(frame / columns);
    source
}
