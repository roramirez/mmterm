use super::text::{blend, dim_color};

fn blit_pixel(buf: &mut [u32], buf_width: u32, sx: u32, sy: u32, clip: [u32; 4], color: u32) {
    let [rx, ry, rw, rh] = clip;
    if sx >= rx + rw || sy >= ry + rh {
        return;
    }
    let idx = (sy * buf_width + sx) as usize;
    if idx < buf.len() {
        buf[idx] = color;
    }
}

fn blend_pixel(buf: &mut [u32], bw: u32, bh: u32, sx: u32, sy: u32, color: u32, alpha: u8) {
    if sx >= bw || sy >= bh {
        return;
    }
    let idx = (sy * bw + sx) as usize;
    if idx < buf.len() {
        buf[idx] = blend(buf[idx], color, alpha);
    }
}

fn compose_color_pixel(
    bitmap: &[u8],
    base: usize,
    pane_is_active: bool,
    dim_factor: f32,
    bg32: u32,
    a: u8,
) -> u32 {
    let r = bitmap[base] as u32;
    let g = bitmap[base + 1] as u32;
    let b = bitmap[base + 2] as u32;
    let px = (0xff_u32 << 24) | (r << 16) | (g << 8) | b;
    let px = if pane_is_active {
        px
    } else {
        dim_color(px, dim_factor)
    };
    blend(bg32, px, a)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn blit_color_glyph(
    buf: &mut [u32],
    buf_width: u32,
    bitmap: &[u8],
    gw: u32,
    gh: u32,
    x_base: u32,
    cell_y: u32,
    y_offset: u32,
    bg32: u32,
    pane_is_active: bool,
    dim_factor: f32,
    clip: [u32; 4],
) {
    for gy in 0..gh {
        for gx in 0..gw {
            let base = ((gy * gw + gx) * 4) as usize;
            let a = bitmap[base + 3];
            if a == 0 {
                continue;
            }
            let sx = x_base + gx;
            let sy = cell_y + y_offset + gy;
            let color = compose_color_pixel(bitmap, base, pane_is_active, dim_factor, bg32, a);
            blit_pixel(buf, buf_width, sx, sy, clip, color);
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn blit_gray_glyph(
    buf: &mut [u32],
    buf_width: u32,
    bitmap: &[u8],
    gw: u32,
    gh: u32,
    x_base: u32,
    cell_y: u32,
    y_offset: u32,
    bg32: u32,
    fg32: u32,
    clip: [u32; 4],
) {
    for gy in 0..gh {
        for gx in 0..gw {
            let alpha = bitmap[(gy * gw + gx) as usize];
            if alpha == 0 {
                continue;
            }
            let sx = x_base + gx;
            let sy = cell_y + y_offset + gy;
            blit_pixel(buf, buf_width, sx, sy, clip, blend(bg32, fg32, alpha));
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn blit_glyph_pixels(
    buf: &mut [u32],
    bw: u32,
    bh: u32,
    ox: u32,
    oy: u32,
    gw: u32,
    gh: u32,
    bitmap: &[u8],
    color: u32,
) {
    for gy in 0..gh {
        for gx in 0..gw {
            let alpha = bitmap[(gy * gw + gx) as usize];
            if alpha == 0 {
                continue;
            }
            let sx = ox + gx;
            let sy = oy + gy;
            blend_pixel(buf, bw, bh, sx, sy, color, alpha);
        }
    }
}

#[cfg(test)]
#[path = "blit_test.rs"]
mod tests;
