use crate::exec::WaitDeadline;
use crate::host::Host;
use crate::value::Value;
use super::inner::InnerOutcome;
pub(super) const HASHES: &[u32] = &[
    0x7A7B6ED4, 0x5E4E3063, 0x5EB974F7, 0x102B6437, 0xF6A05538, 0xEF4581DC, 0xC2C3C4F4,
    0xCD71FE7A, 0x7B0E970D, 0x4AB0CAEF, 0x2399D761, 0x58B31737, 0xF8FD08F6, 0x65F2E980,
    0x4C310B4B, 0xC50DFD06, 0xDAD96289, 0x539B07BC, 0xCE94E6CA, 0x70612DB5, 0xD01BE374,
    0x56BBBA3A, 0x30636D6E, 0xA361D9F7, 0x053FAC99, 0x4697D2DD, 0x61BAE53C, 0x7E277952,
    0x87113971, 0xFDDF6C40, 0xF9705332, 0xDFD5599E, 0x4A02D664, 0x5548EF5E, 0x69073588,
    0x40258A56, 0xC29B30E3, 0xDFDBA8E4, 0x63F660A2, 0xFEF981D4, 0x6B4A7873, 0x103EDAC0,
    0x68EF2987, 0xF6459905, 0xD7771B72, 0x35A89417, 0x63C341B1, 0x721FA5B7, 0x8838FD56,
    0x89B59D02, 0xFD128A61, 0x3CAD3884, 0x3F34D519, 0x9AC11F47, 0xAC796720, 0x01B4517C,
    0x661AFB43, 0x3B1D5537, 0x6F384A3D, 0xBA383010,
];
impl crate::exec::Vm {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn handle_inner_render<H: Host>(
        &mut self,
        hash: u32,
        count: usize,
        _has_retval: bool,
        host: &mut H,
        args: &[Value],
    ) -> Option<Result<InnerOutcome, crate::exec::VmError>> {
        let opcode_start_ip = self.current_ip().unwrap_or(0);
        self.handle_inner_render_at(
            hash,
            count,
            _has_retval,
            host,
            args,
            opcode_start_ip,
        )
    }
    pub(super) fn handle_inner_render_at<H: Host>(
        &mut self,
        hash: u32,
        count: usize,
        _has_retval: bool,
        host: &mut H,
        args: &[Value],
        opcode_start_ip: usize,
    ) -> Option<Result<InnerOutcome, crate::exec::VmError>> {
        match hash {
            0x01B4517C => {
                let sprite = arg_int_from_top(args, count, 0) as u32;
                let keyframes = decode_float_keyframes(args, count);
                host.sprite_xmodify_define(sprite, &keyframes);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x661AFB43 => {
                let sprite = arg_int_from_top(args, count, 0) as u32;
                let target = arg_float_bits_from_top(args, count, 1);
                let duration = if count >= 3 {
                    arg_int_from_top(args, count, 2)
                } else {
                    0
                };
                let extrapolate = count >= 4 && arg_int_from_top(args, count, 3) != 0;
                host.sprite_rotate(sprite, target, duration, extrapolate);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x3B1D5537 | 0xBA383010 => {
                let sprite = arg_int_from_top(args, count, 0) as u32;
                let target = arg_float_bits_from_top(args, count, 1);
                let duration = if count >= 3 {
                    arg_int_from_top(args, count, 2)
                } else {
                    0
                };
                if hash == 0x3B1D5537 {
                    host.sprite_xmodify_set(sprite, target, duration);
                } else {
                    host.sprite_ymodify_set(sprite, target, duration);
                }
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x6F384A3D => {
                let sprite = arg_int_from_top(args, count, 0) as u32;
                let keyframes = decode_float_keyframes(args, count);
                host.sprite_ymodify_define(sprite, &keyframes);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x63C341B1 | 0x721FA5B7 => {
                if count <= 1 {
                    let sprite = (count == 1)
                        .then(|| arg_int_from_top(args, count, 0) as u32);
                    if hash == 0x63C341B1 {
                        host.sprite_visibility_pop(sprite);
                    } else {
                        host.sprite_visibility_push(sprite);
                    }
                }
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x8838FD56 => {
                host.sprite_page_auto_release(arg_int_from_top(args, count, 0) as u32);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x89B59D02 => {
                let page = arg_int_from_top(args, count, 0) as u32;
                let x = arg_int_from_top(args, count, 1);
                let y = arg_int_from_top(args, count, 2);
                let w = arg_int_from_top(args, count, 3);
                let h = arg_int_from_top(args, count, 4);
                let color = arg_int_from_top(args, count, 5) as u32;
                host.grp_alphablend(page, x, y, w, h, color);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xFD128A61 => {
                let page = arg_int_from_top(args, count, 0) as u32;
                let x = arg_int_from_top(args, count, 1);
                let y = arg_int_from_top(args, count, 2);
                let w = arg_int_from_top(args, count, 3);
                let h = arg_int_from_top(args, count, 4);
                let dark = arg_int_from_top(args, count, 5) as u32;
                let light = arg_int_from_top(args, count, 6) as u32;
                let mix = (count >= 8).then(|| arg_int_from_top(args, count, 7));
                host.grp_sepia(page, x, y, w, h, dark, light, mix);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x3CAD3884 => {
                let source_page = arg_int_from_top(args, count, 0) as u32;
                let destination_page = arg_int_from_top(args, count, 1) as u32;
                let table_value = args.get(count.min(args.len()).saturating_sub(3));
                let cells = table_value
                    .and_then(|value| match value.data.as_deref() {
                        Some(crate::value::ValueData::IntArray { cells, .. })
                        | Some(crate::value::ValueData::StringArray { cells, .. }) => {
                            Some(cells.as_slice())
                        }
                        _ => None,
                    });
                if let Some(cells) = cells.filter(|cells| cells.len() >= 256) {
                    let mut table = [0u8; 256];
                    for (output, value) in table.iter_mut().zip(cells) {
                        *output = value.as_int().unwrap_or(0).clamp(0, 255) as u8;
                    }
                    host.make_alfa_table(source_page, destination_page, &table);
                }
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x3F34D519 => {
                host.sprite_set_smooth_animation(
                    arg_int_from_top(args, count, 0) as u32,
                );
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x7A7B6ED4 => {
                let key_str = args.first().and_then(|v| v.as_str_bytes()).unwrap_or(&[]);
                eprintln!(
                    "[GFX] generate_key key={:?}", String::from_utf8_lossy(key_str)
                );
                host.generate_key(key_str);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x5E4E3063 => {
                let page = arg_int_from_top(args, count, 0) as u32;
                let mode = arg_int_from_top(args, count, 1) as u32;
                host.page_set_draw_mode(page, mode);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x5EB974F7 => {
                let dst_page = args
                    .get(count.saturating_sub(1))
                    .and_then(|v| v.as_int())
                    .unwrap_or(0) as u32;
                let filename = args
                    .get(count.saturating_sub(2))
                    .and_then(|v| v.as_str_bytes())
                    .unwrap_or(&[]);
                let dst_x = if count >= 3 {
                    arg_int_from_top(args, count, 2)
                } else {
                    0
                };
                let dst_y = if count >= 4 {
                    arg_int_from_top(args, count, 3)
                } else {
                    0
                };
                eprintln!(
                    "[GFX] pic_unpack (hash=0x{:08X}) page={} at=({},{}) {:?}", hash,
                    dst_page, dst_x, dst_y, String::from_utf8_lossy(filename)
                );
                host.pic_unpack_into(dst_page, filename, dst_x, dst_y);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x102B6437 | 0xEF4581DC | 0xF6A05538 => {
                let handle = host.sprite_create_raw(count, args);
                if hash == 0xF6A05538 {
                    host.sprite_mark_overlay(handle);
                }
                eprintln!(
                    "[GFX] sprite_create (hash=0x{:08X}) → handle {}", hash, handle
                );
                self.stack.push(Value::int(handle as i32));
                Some(Ok(InnerOutcome::Normal))
            }
            0xC2C3C4F4 | 0xCD71FE7A => {
                let filename = args
                    .get(count.saturating_sub(1))
                    .and_then(|v| v.as_str_bytes())
                    .unwrap_or(&[]);
                let handle = host.sprite_create_file_raw(count, args);
                if hash == 0xCD71FE7A {
                    host.sprite_mark_overlay(handle);
                }
                eprintln!(
                    "[GFX] sprite_create_file {:?} → handle {}",
                    String::from_utf8_lossy(filename), handle
                );
                self.stack.push(Value::int(handle as i32));
                Some(Ok(InnerOutcome::Normal))
            }
            0x7B0E970D => {
                let sprite = arg_int_from_top(args, count, 0) as u32;
                let x = arg_int_from_top(args, count, 1);
                let y = arg_int_from_top(args, count, 2);
                let width = arg_int_from_top(args, count, 3);
                let height = arg_int_from_top(args, count, 4);
                host.sprite_set_clip(sprite, x, y, width, height);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x4AB0CAEF => {
                let sprite = args
                    .get(count.saturating_sub(1))
                    .and_then(|v| v.as_int())
                    .unwrap_or(0) as u32;
                let dst_page = args
                    .get(count.saturating_sub(2))
                    .and_then(|v| v.as_int())
                    .unwrap_or(0) as u32;
                let position = (count >= 4)
                    .then(|| {
                        let x = args
                            .get(count - 3)
                            .and_then(|v| v.as_int())
                            .unwrap_or(0);
                        let y = args
                            .get(count - 4)
                            .and_then(|v| v.as_int())
                            .unwrap_or(0);
                        (x, y)
                    });
                host.sprite_paste(sprite, dst_page, position);
                eprintln!(
                    "[GFX] sprite_paste sp={} -> page={} position={:?}", sprite,
                    dst_page, position
                );
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x2399D761 => {
                let page = arg_int_from_top(args, count, 0) as u32;
                let old = host.set_frontbuffer(page);
                eprintln!("[GFX] set_frontbuffer page={} (old={})", page, old);
                self.stack.push(Value::int(old as i32));
                Some(Ok(InnerOutcome::Normal))
            }
            0x58B31737 => {
                let sprite = args.first().and_then(|v| v.as_int()).unwrap_or(0) as u32;
                host.sprite_release(sprite);
                eprintln!("[GFX] sprite_release sp={}", sprite);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xC29B30E3 | 0xDFDBA8E4 => {
                let real = count.min(args.len());
                let sprite = args
                    .get(real.saturating_sub(1))
                    .and_then(Value::as_int)
                    .unwrap_or(0) as u32;
                let keyframes = decode_frame_keyframes(args, real);
                if hash == 0xC29B30E3 {
                    host.sprite_animate_define(sprite, &keyframes);
                } else {
                    host.sprite_animate_add(sprite, &keyframes);
                }
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xF8FD08F6 => {
                let page = args.first().and_then(|v| v.as_int()).unwrap_or(0) as u32;
                let w = host.page_get_width(page);
                eprintln!("[GFX] page_get_width page={} → {}", page, w);
                self.stack.push(Value::int(w));
                Some(Ok(InnerOutcome::Normal))
            }
            0x65F2E980 => {
                let page = args.first().and_then(|v| v.as_int()).unwrap_or(0) as u32;
                let h = host.page_get_height(page);
                eprintln!("[GFX] page_get_height page={} → {}", page, h);
                self.stack.push(Value::int(h));
                Some(Ok(InnerOutcome::Normal))
            }
            0x4C310B4B => {
                let page = arg_int_from_top(args, count, 0) as u32;
                let x = arg_int_from_top(args, count, 1);
                let y = arg_int_from_top(args, count, 2);
                self.stack.push(Value::int(host.page_get_pixel(page, x, y)));
                Some(Ok(InnerOutcome::Normal))
            }
            0xC50DFD06 => {
                let page = args.first().and_then(|v| v.as_int()).unwrap_or(0) as u32;
                host.page_release(page);
                eprintln!("[GFX] page_release page={}", page);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xDAD96289 => {
                let filename = args
                    .first()
                    .and_then(|v| v.as_str_bytes())
                    .unwrap_or(&[]);
                let page = host.page_create_file(filename);
                eprintln!(
                    "[GFX] page_create_file {:?} → page {}",
                    String::from_utf8_lossy(filename), page
                );
                self.stack.push(Value::int(page as i32));
                Some(Ok(InnerOutcome::Normal))
            }
            0x539B07BC => {
                let filename = args
                    .get(count.saturating_sub(1))
                    .and_then(|v| v.as_str_bytes())
                    .unwrap_or(&[]);
                let width = host.pic_get_width(filename);
                eprintln!(
                    "[GFX] pic_get_width {:?} -> {}", String::from_utf8_lossy(filename),
                    width
                );
                self.stack.push(Value::int(width));
                Some(Ok(InnerOutcome::Normal))
            }
            0xCE94E6CA => {
                let filename = args
                    .get(count.saturating_sub(1))
                    .and_then(|v| v.as_str_bytes())
                    .unwrap_or(&[]);
                let height = host.pic_get_height(filename);
                eprintln!(
                    "[GFX] pic_get_height {:?} -> {}", String::from_utf8_lossy(filename),
                    height
                );
                self.stack.push(Value::int(height));
                Some(Ok(InnerOutcome::Normal))
            }
            0x70612DB5 => {
                let page = args.first().and_then(|v| v.as_int()).unwrap_or(0) as u32;
                let alpha_page = host.page_get_alpha(page);
                eprintln!(
                    "[GFX] page_get_alpha page={} -> alpha_page={}", page, alpha_page
                );
                self.stack.push(Value::int(alpha_page as i32));
                Some(Ok(InnerOutcome::Normal))
            }
            0xD01BE374 => {
                let color = args.first().and_then(|v| v.as_int()).unwrap_or(0) as u32;
                let h = args.get(1).and_then(|v| v.as_int()).unwrap_or(0);
                let w = args.get(2).and_then(|v| v.as_int()).unwrap_or(0);
                let y = args.get(3).and_then(|v| v.as_int()).unwrap_or(0);
                let x = args.get(4).and_then(|v| v.as_int()).unwrap_or(0);
                let page = args.get(5).and_then(|v| v.as_int()).unwrap_or(0) as u32;
                host.grp_boxfill(page, x, y, w, h, color);
                eprintln!(
                    "[GFX] grp_boxfill page={} rect=({},{},{},{}) color=0x{:08X}", page,
                    x, y, w, h, color
                );
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x56BBBA3A => {
                let src_page = arg_int_from_top(args, count, 0) as u32;
                let src_x = arg_int_from_top(args, count, 1);
                let src_y = arg_int_from_top(args, count, 2);
                let w = arg_int_from_top(args, count, 3);
                let h = arg_int_from_top(args, count, 4);
                let dst_page = arg_int_from_top(args, count, 5) as u32;
                let dst_x = if count >= 7 {
                    arg_int_from_top(args, count, 6)
                } else {
                    src_x
                };
                let dst_y = if count >= 8 {
                    arg_int_from_top(args, count, 7)
                } else {
                    src_y
                };
                host.grp_copy(src_page, src_x, src_y, w, h, dst_page, dst_x, dst_y);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x30636D6E => {
                let src_page = arg_int_from_top(args, count, 0) as u32;
                let src_x = arg_int_from_top(args, count, 1);
                let src_y = arg_int_from_top(args, count, 2);
                let w = arg_int_from_top(args, count, 3);
                let h = arg_int_from_top(args, count, 4);
                let dst_page = arg_int_from_top(args, count, 5) as u32;
                let dst_x = if count >= 7 {
                    arg_int_from_top(args, count, 6)
                } else {
                    src_x
                };
                let dst_y = if count >= 8 {
                    arg_int_from_top(args, count, 7)
                } else {
                    src_y
                };
                let alpha = if count >= 9 {
                    arg_int_from_top(args, count, 8)
                } else {
                    0
                };
                host.grp_extcopy(
                    src_page,
                    src_x,
                    src_y,
                    w,
                    h,
                    dst_page,
                    dst_x,
                    dst_y,
                    alpha,
                );
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xA361D9F7 => {
                let src_page = arg_int_from_top(args, count, 0) as u32;
                let src_x = arg_int_from_top(args, count, 1);
                let src_y = arg_int_from_top(args, count, 2);
                let w = arg_int_from_top(args, count, 3);
                let h = arg_int_from_top(args, count, 4);
                let dst_page = arg_int_from_top(args, count, 5) as u32;
                let dst_x = if count >= 7 {
                    arg_int_from_top(args, count, 6)
                } else {
                    src_x
                };
                let dst_y = if count >= 8 {
                    arg_int_from_top(args, count, 7)
                } else {
                    src_y
                };
                host.grp_mulcopy(src_page, src_x, src_y, w, h, dst_page, dst_x, dst_y);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xAC796720 => {
                let dst_page = arg_int_from_top(args, count, 5) as u32;
                host.grp_modify_copy(
                    dst_page,
                    arg_int_from_top(args, count, 6),
                    arg_int_from_top(args, count, 7),
                    arg_int_from_top(args, count, 8),
                    arg_int_from_top(args, count, 9),
                    arg_int_from_top(args, count, 0) as u32,
                    arg_int_from_top(args, count, 1),
                    arg_int_from_top(args, count, 2),
                    arg_int_from_top(args, count, 3),
                    arg_int_from_top(args, count, 4),
                    arg_float_bits_from_top(args, count, 10),
                    arg_float_bits_from_top(args, count, 11),
                    arg_float_bits_from_top(args, count, 12),
                );
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x9AC11F47 => {
                self.rng_state = host
                    .grp_make_mosaic(
                        arg_int_from_top(args, count, 0) as u32,
                        arg_int_from_top(args, count, 1),
                        arg_int_from_top(args, count, 2),
                        arg_int_from_top(args, count, 3).max(1),
                        arg_int_from_top(args, count, 4).max(1),
                        arg_int_from_top(args, count, 5) as u32,
                        arg_int_from_top(args, count, 6),
                        arg_int_from_top(args, count, 7),
                        arg_int_from_top(args, count, 8),
                        self.rng_state,
                    );
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x7E277952 => {
                let page = arg_int_from_top(args, count, 0) as u32;
                let x = arg_int_from_top(args, count, 1);
                let y = arg_int_from_top(args, count, 2);
                let color = arg_int_from_top(args, count, 3) as u32;
                host.grp_point_set(page, x, y, color);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x053FAC99 => {
                let page = arg_int_from_top(args, count, 0) as u32;
                let x = arg_int_from_top(args, count, 1);
                let y = arg_int_from_top(args, count, 2);
                let w = arg_int_from_top(args, count, 3);
                let h = arg_int_from_top(args, count, 4);
                host.grp_reverse(page, x, y, w, h);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x61BAE53C | 0xFDDF6C40 => {
                let page = arg_int_from_top(args, count, 0) as u32;
                let x = arg_int_from_top(args, count, 1);
                let y = arg_int_from_top(args, count, 2);
                let w = arg_int_from_top(args, count, 3);
                let h = arg_int_from_top(args, count, 4);
                let color = arg_int_from_top(args, count, 5) as u32;
                if hash == 0x61BAE53C {
                    host.grp_mulboxfill(page, x, y, w, h, color);
                } else {
                    let alpha = arg_int_from_top(args, count, 6);
                    host.grp_extboxfill(page, x, y, w, h, color, alpha);
                }
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x4697D2DD | 0x87113971 => {
                let src_page = arg_int_from_top(args, count, 0) as u32;
                let src_x = arg_int_from_top(args, count, 1);
                let src_y = arg_int_from_top(args, count, 2);
                let w = arg_int_from_top(args, count, 3);
                let h = arg_int_from_top(args, count, 4);
                let dst_page = arg_int_from_top(args, count, 5) as u32;
                let dst_x = if count >= 7 {
                    arg_int_from_top(args, count, 6)
                } else {
                    src_x
                };
                let dst_y = if count >= 8 {
                    arg_int_from_top(args, count, 7)
                } else {
                    src_y
                };
                if hash == 0x4697D2DD {
                    host.grp_revmulcopy(
                        src_page,
                        src_x,
                        src_y,
                        w,
                        h,
                        dst_page,
                        dst_x,
                        dst_y,
                    );
                } else {
                    host.grp_swap(src_page, src_x, src_y, w, h, dst_page, dst_x, dst_y);
                }
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xF9705332 => {
                let page = arg_int_from_top(args, count, 0) as u32;
                let alpha_page = arg_int_from_top(args, count, 1) as u32;
                host.page_set_antidata(page, alpha_page);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xDFD5599E => {
                let page = arg_int_from_top(args, count, 0) as u32;
                let rect = (count >= 5)
                    .then(|| {
                        (
                            arg_int_from_top(args, count, 1),
                            arg_int_from_top(args, count, 2),
                            arg_int_from_top(args, count, 3),
                            arg_int_from_top(args, count, 4),
                        )
                    });
                host.invalidate_page(page, rect);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x4A02D664 => {
                let sprite = arg_int_from_top(args, count, 0) as u32;
                let reference = (count >= 2)
                    .then(|| arg_int_from_top(args, count, 1) as u32);
                host.sprite_priority_high_group(sprite, reference);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x5548EF5E => {
                let sprite = args
                    .get(count.saturating_sub(1))
                    .and_then(|v| v.as_int())
                    .unwrap_or(0) as u32;
                let reference = (count >= 2)
                    .then(|| arg_int_from_top(args, count, 1) as u32);
                host.sprite_priority_high_single(sprite, reference);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x69073588 => {
                let sprite = args
                    .get(count.saturating_sub(1))
                    .and_then(|v| v.as_int())
                    .unwrap_or(0) as u32;
                let reference = (count >= 2)
                    .then(|| arg_int_from_top(args, count, 1) as u32);
                host.sprite_priority_low_group(sprite, reference);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x63F660A2 => {
                let sprite = arg_int_from_top(args, count, 0) as u32;
                let width = host.sprite_width(sprite);
                self.stack.push(Value::int(width));
                Some(Ok(InnerOutcome::Normal))
            }
            0xFEF981D4 => {
                let sprite = arg_int_from_top(args, count, 0) as u32;
                let height = host.sprite_height(sprite);
                self.stack.push(Value::int(height));
                Some(Ok(InnerOutcome::Normal))
            }
            0x6B4A7873 => {
                let sprite = arg_int_from_top(args, count, 0) as u32;
                let x = host.sprite_pos_x(sprite);
                self.stack.push(Value::int(x));
                Some(Ok(InnerOutcome::Normal))
            }
            0x103EDAC0 => {
                let now = host.get_timestamp();
                let Some(frame) = self.frames.last_mut() else {
                    return Some(Err(crate::exec::VmError::Exit));
                };
                if let WaitDeadline::At(deadline) = frame.wait_deadline {
                    let wrapped_distance = now.wrapping_sub(deadline).unsigned_abs();
                    if now < deadline && wrapped_distance <= 0x0293_2E00 {
                        return Some(Ok(InnerOutcome::Waiting));
                    }
                }
                let sprite = arg_int_from_top(args, count, 0) as u32;
                if host.sprite_is_frame_animating(sprite) {
                    frame.wait_deadline = WaitDeadline::At(now.wrapping_add(1));
                    return Some(Ok(InnerOutcome::Waiting));
                }
                frame.wait_deadline = WaitDeadline::None;
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x68EF2987 => {
                let sprite = arg_int_from_top(args, count, 0) as u32;
                self.stack.push(Value::int(host.sprite_is_moving(sprite) as i32));
                Some(Ok(InnerOutcome::Normal))
            }
            0x40258A56 => {
                let now = host.get_timestamp();
                let Some(frame) = self.frames.last_mut() else {
                    return Some(Err(crate::exec::VmError::Exit));
                };
                if let WaitDeadline::At(deadline) = frame.wait_deadline {
                    let wrapped_distance = now.wrapping_sub(deadline).unsigned_abs();
                    if now < deadline && wrapped_distance <= 0x0293_2E00 {
                        return Some(Ok(InnerOutcome::Waiting));
                    }
                }
                let sprite = arg_int_from_top(args, count, 0) as u32;
                if host.sprite_is_moving(sprite) {
                    frame.wait_deadline = WaitDeadline::At(now.wrapping_add(1));
                    return Some(Ok(InnerOutcome::Waiting));
                }
                frame.wait_deadline = WaitDeadline::None;
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xF6459905 => {
                let sprite = arg_int_from_top(args, count, 0) as u32;
                let y = host.sprite_pos_y(sprite);
                self.stack.push(Value::int(y));
                Some(Ok(InnerOutcome::Normal))
            }
            0xD7771B72 => {
                let now = host.get_timestamp();
                let Some(frame) = self.frames.last_mut() else {
                    return Some(Err(crate::exec::VmError::Exit));
                };
                if let WaitDeadline::At(deadline) = frame.wait_deadline {
                    let wrapped_distance = now.wrapping_sub(deadline).unsigned_abs();
                    if now < deadline && wrapped_distance <= 0x0293_2E00 {
                        return Some(Ok(InnerOutcome::Waiting));
                    }
                }
                let sprite = arg_int_from_top(args, count, 0) as u32;
                if host.sprite_is_alpha_animating(sprite) {
                    frame.wait_deadline = WaitDeadline::At(now.wrapping_add(1));
                    return Some(Ok(InnerOutcome::Waiting));
                }
                frame.wait_deadline = WaitDeadline::None;
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0x35A89417 => {
                let real = count.min(args.len());
                let from_top = |n: usize| (n < real).then(|| &args[real - 1 - n]);
                let text = from_top(0).and_then(Value::as_str_bytes).unwrap_or(&[]);
                let width = (count >= 3)
                    .then(|| from_top(1).and_then(Value::as_int).unwrap_or(0));
                let height = (count >= 3)
                    .then(|| from_top(2).and_then(Value::as_int).unwrap_or(0));
                let alignment = (count >= 4)
                    .then(|| from_top(3).and_then(Value::as_int).unwrap_or(0));
                let site = self
                    .frames
                    .last()
                    .and_then(|frame| self.scripts.get(frame.script_idx))
                    .map(|script| crate::host::DisplayTextSite {
                        script_name: script.name.clone().unwrap_or_default(),
                        render_offset: opcode_start_ip,
                        code_crc32: script.code_crc32,
                    })
                    .unwrap_or(crate::host::DisplayTextSite {
                        script_name: Vec::new(),
                        render_offset: opcode_start_ip,
                        code_crc32: 0,
                    });
                host.font_render(
                    &site,
                    self.active_function_id,
                    text,
                    width,
                    height,
                    alignment,
                );
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            _ => None,
        }
    }
}
fn arg_int_from_top(args: &[Value], count: usize, n: usize) -> i32 {
    let real = count.min(args.len());
    (n < real).then(|| &args[real - 1 - n]).and_then(Value::as_int).unwrap_or(0)
}
fn arg_float_bits_from_top(args: &[Value], count: usize, n: usize) -> f32 {
    let real = count.min(args.len());
    (n < real)
        .then(|| &args[real - 1 - n])
        .map(|value| f32::from_bits(value.bits))
        .unwrap_or(0.0)
}
fn decode_float_keyframes(args: &[Value], count: usize) -> Vec<(f32, i32)> {
    let real = count.min(args.len());
    let mut keyframes = Vec::new();
    let mut remaining = real.saturating_sub(1);
    let mut cursor = real.saturating_sub(2);
    while remaining >= 2 {
        let value = args
            .get(cursor)
            .map(|value| f32::from_bits(value.bits))
            .unwrap_or(0.0);
        let duration = args
            .get(cursor.saturating_sub(1))
            .and_then(Value::as_int)
            .unwrap_or(0);
        keyframes.push((value, duration));
        remaining -= 2;
        cursor = cursor.saturating_sub(2);
    }
    if remaining == 1 {
        let value = args
            .get(cursor)
            .map(|value| f32::from_bits(value.bits))
            .unwrap_or(0.0);
        keyframes.push((value, -1));
    }
    keyframes
}
fn decode_frame_keyframes(args: &[Value], count: usize) -> Vec<(i32, i32)> {
    let real = count.min(args.len());
    let mut keyframes = Vec::new();
    let mut remaining = real.saturating_sub(1);
    let mut cursor = real.saturating_sub(2);
    while remaining >= 2 {
        let frame = args.get(cursor).and_then(Value::as_int).unwrap_or(0);
        let duration = args
            .get(cursor.saturating_sub(1))
            .and_then(Value::as_int)
            .unwrap_or(0);
        keyframes.push((frame, duration));
        remaining -= 2;
        cursor = cursor.saturating_sub(2);
    }
    if remaining == 1 {
        let frame = args.get(cursor).and_then(Value::as_int).unwrap_or(0);
        keyframes.push((frame, -1));
    }
    keyframes
}

