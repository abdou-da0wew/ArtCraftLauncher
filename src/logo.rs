//! The ArtCraft mark, rasterised from the site's own SVG.
//!
//! `assets/images/artcraft-icon.svg` is one path in a `0 0 116.34 97.5`
//! viewBox, so it rasterises once per size into an alpha mask and is then
//! tinted with `currentColor` — exactly what the site does with
//! `fill="currentColor"`.
//!
//! The path only uses `M L H V Z` in absolute and relative form, which the
//! tiny parser below handles. (tiny-skia itself does the rasterisation and
//! antialiasing.)

use std::collections::HashMap;
use std::sync::Mutex;

use tiny_skia::{FillRule, Paint, Path, PathBuilder, Pixmap, Transform};

/// The exact path from `assets/images/artcraft-icon.svg`.
const MARK_PATH: &str = "M104.28,49.49L81.55,0h-31.23l-3.17,4.76L14.75,53.63,0,75.85l21.55,21.55,63.79-36.94,16.99,37.04,14.01-21.74-12.06-26.27ZM32.89,65.66l32.42-48.87,10.91,23.77-43.32,25.09Z";

/// The mark's viewBox, so callers can derive its aspect ratio.
pub const MARK_W: f32 = 116.34;
pub const MARK_H: f32 = 97.5;

/// Parse an SVG path subset (`M m L l H h V v Z z`) into a tiny-skia path.
fn parse_svg(d: &str) -> Option<Path> {
    let mut pb = PathBuilder::new();
    let b = d.as_bytes();
    let mut i = 0usize;
    let mut cur = (0.0f32, 0.0f32);
    let mut start = (0.0f32, 0.0f32);
    let mut cmd = 0u8;

    fn skip_ws(b: &[u8], i: &mut usize) {
        while *i < b.len() && (b[*i].is_ascii_whitespace() || b[*i] == b',') {
            *i += 1;
        }
    }
    fn num(b: &[u8], i: &mut usize) -> Option<f32> {
        skip_ws(b, i);
        let s = *i;
        if *i < b.len() && (b[*i] == b'-' || b[*i] == b'+') {
            *i += 1;
        }
        while *i < b.len()
            && (b[*i].is_ascii_digit() || b[*i] == b'.' || b[*i] == b'e' || b[*i] == b'E')
        {
            *i += 1;
        }
        if s == *i {
            return None;
        }
        std::str::from_utf8(&b[s..*i]).ok()?.parse().ok()
    }

    while i < b.len() {
        skip_ws(b, &mut i);
        if i >= b.len() {
            break;
        }
        if b[i].is_ascii_alphabetic() {
            cmd = b[i];
            i += 1;
        }
        match cmd {
            b'M' | b'm' => {
                let (x, y) = (num(b, &mut i)?, num(b, &mut i)?);
                cur = if cmd == b'M' { (x, y) } else { (cur.0 + x, cur.1 + y) };
                start = cur;
                pb.move_to(cur.0, cur.1);
                cmd = if cmd == b'M' { b'L' } else { b'l' };
            }
            b'L' | b'l' | b't' | b'T' => {
                let (x, y) = (num(b, &mut i)?, num(b, &mut i)?);
                let p = if cmd.is_ascii_lowercase() {
                    (cur.0 + x, cur.1 + y)
                } else {
                    (x, y)
                };
                pb.line_to(p.0, p.1);
                cur = p;
            }
            b'H' | b'h' => {
                let x = num(b, &mut i)?;
                let p = if cmd == b'h' { (cur.0 + x, cur.1) } else { (x, cur.1) };
                pb.line_to(p.0, p.1);
                cur = p;
            }
            b'V' | b'v' => {
                let y = num(b, &mut i)?;
                let p = if cmd == b'v' { (cur.0, cur.1 + y) } else { (cur.0, y) };
                pb.line_to(p.0, p.1);
                cur = p;
            }
            b'Z' | b'z' => {
                pb.close();
                cur = start;
            }
            _ => return None,
        }
    }
    pb.finish()
}

#[derive(Clone)]
pub struct MarkMask {
    pub mask: Vec<u8>,
    pub w: usize,
    pub h: usize,
}

fn rasterize(size: u32) -> Option<MarkMask> {
    let w = size.max(1);
    let h = ((size as f32) * MARK_H / MARK_W).round().max(1.0) as u32;
    let mut pix = Pixmap::new(w, h)?;
    let path = parse_svg(MARK_PATH)?;
    let scale = w as f32 / MARK_W;
    pix.fill_path(
        &path,
        &Paint {
            shader: tiny_skia::Shader::SolidColor(tiny_skia::Color::WHITE),
            blend_mode: tiny_skia::BlendMode::Source,
            ..Default::default()
        },
        FillRule::Winding,
        Transform::from_scale(scale, scale),
        None,
    );
    let data = pix.data();
    let mut mask = vec![0u8; (w * h) as usize];
    for (i, px) in data.chunks_exact(4).enumerate() {
        mask[i] = px[3];
    }
    Some(MarkMask {
        mask,
        w: w as usize,
        h: h as usize,
    })
}

struct Cache {
    map: HashMap<u32, Option<MarkMask>>,
}

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

/// Get (and memoise) the mark as an alpha mask at `size` px wide.
pub fn mark(size: u32) -> Option<MarkMask> {
    let q = size.clamp(4, 1024);
    let mut guard = CACHE.lock().ok()?;
    let c = guard.get_or_insert_with(|| Cache {
        map: HashMap::new(),
    });
    if let Some(m) = c.map.get(&q) {
        return m.clone();
    }
    let m = rasterize(q);
    c.map.insert(q, m.clone());
    m
}

/// Blit the mark at `(x, y)` with width `size`, tinted with `c`.
pub fn blit(canvas: &mut crate::gfx::canvas::Canvas, x: f32, y: f32, size: f32, c: crate::theme::Rgba) {
    let q = (size.round() as u32).clamp(4, 1024);
    let Some(m) = mark(q) else { return };
    let h = m.h as f32 / m.w as f32 * size;
    let x0 = x as i32;
    let y0 = (y + (size * 0.5 - h * 0.5)) as i32;
    canvas.glyph(&m.mask, m.w, m.h, x0, y0, c);
}

/// Raw RGBA bitmap of the mark (brand blue `#4e7bfb`), for icon generation.
pub fn mark_rgba(size: u32) -> Option<(Vec<u8>, u32, u32)> {
    let m = mark(size)?;
    let mut out = Vec::with_capacity(m.mask.len() * 4);
    for a in &m.mask {
        out.extend_from_slice(&[0x4e, 0x7b, 0xfb, *a]);
    }
    Some((out, m.w as u32, m.h as u32))
}
