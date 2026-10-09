//! A tiny software canvas: RGBA8, premultiplied-in-place blending, and
//! subpixel-accurate hairlines.
//!
//! The ArtCraft design language is *almost entirely* 1px rules, `.tick`
//! crosshairs and `gap-px` hairline grids. Rendering those with a generic
//! 2D library produces mushy grey lines at fractional offsets, so this canvas
//! distributes coverage across the two neighbouring pixels instead — a 1px
//! border at x=100.5 draws 50/50 across both columns, which is exactly what
//! `border-top: 1px` means.

use crate::theme::Rgba;

#[derive(Clone)]
pub struct Canvas {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u8>, // RGBA8, straight alpha in, premultiplied out
}

impl Canvas {
    pub fn new(w: usize, h: usize) -> Self {
        Self {
            w,
            h,
            px: vec![0; w * h * 4],
        }
    }

    pub fn clear(&mut self, c: Rgba) {
        let [r, g, b, a] = c.to_bytes();
        for px in self.px.chunks_exact_mut(4) {
            px[0] = r;
            px[1] = g;
            px[2] = b;
            px[3] = a;
        }
    }

    pub fn idx(&self, x: usize, y: usize) -> usize {
        (y * self.w + x) * 4
    }

    /// Blend one source pixel (straight alpha) over the canvas.
    #[inline]
    pub fn blend_px(&mut self, x: usize, y: usize, c: Rgba) {
        if x >= self.w || y >= self.h {
            return;
        }
        if c.a <= 0.0 {
            return;
        }
        let i = self.idx(x, y);
        let sa = c.a.min(1.0);
        let da = self.px[i + 3] as f32 / 255.0;
        let out_a = sa + da * (1.0 - sa);
        if out_a <= 0.0 {
            return;
        }
        let mix = |s: f32, d: u8| -> u8 {
            let dv = d as f32 / 255.0;
            let v = (s * sa + dv * da * (1.0 - sa)) / out_a;
            (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
        };
        self.px[i] = mix(c.r, self.px[i]);
        self.px[i + 1] = mix(c.g, self.px[i + 1]);
        self.px[i + 2] = mix(c.b, self.px[i + 2]);
        self.px[i + 3] = (out_a.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    }

    /// Blend a pixel with an extra coverage multiplier in 0..1.
    #[inline]
    pub fn blend_cov(&mut self, x: usize, y: usize, c: Rgba, cov: f32) {
        if cov <= 0.0 {
            return;
        }
        self.blend_px(x, y, c.with_alpha(c.a * cov.min(1.0)));
    }

    pub fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, c: Rgba) {
        if w <= 0.0 || h <= 0.0 || c.a <= 0.0 {
            return;
        }
        let (x0f, x1f) = (x, x + w);
        let (y0f, y1f) = (y, y + h);
        let xs = (x0f.floor().max(0.0) as usize).min(self.w);
        let xe = (x1f.ceil().max(0.0) as usize).min(self.w);
        let ys = (y0f.floor().max(0.0) as usize).min(self.h);
        let ye = (y1f.ceil().max(0.0) as usize).min(self.h);

        // Row coverage (fractional top/bottom edges).
        let row_cov = |yy: usize| -> f32 {
            if yy == 0 && y0f > 0.0 {
                1.0 - (y0f - y0f.floor())
            } else if y1f.fract() > 0.0 && yy as f32 == y1f.floor() && yy as f32 > y0f.floor() {
                y1f.fract()
            } else {
                1.0
            }
        };
        for yy in ys..ye {
            let rc = row_cov(yy);
            if rc <= 0.0 {
                continue;
            }
            for xx in xs..xe {
                let cov = col_cov(xx, x0f, x1f) * rc;
                self.blend_cov(xx, yy, c, cov);
            }
        }
    }

    /// A 1px rule with subpixel positioning — returns true if it painted.
    pub fn hline(&mut self, x: f32, y: f32, w: f32, c: Rgba) {
        self.line(x, y, x + w, y, 1.0, c);
    }

    pub fn vline(&mut self, x: f32, y: f32, h: f32, c: Rgba) {
        self.line(x, y, x, y + h, 1.0, c);
    }

    /// Axis-aligned line of thickness `th` (in logical px) with AA on both
    /// fractional ends and on the perpendicular axis.
    pub fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, th: f32, c: Rgba) {
        if c.a <= 0.0 {
            return;
        }
        if (y1 - y0).abs() < 1e-4 {
            // horizontal
            let (a, b) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
            let ymid = y0.max(y1);
            let lo = (ymid - th * 0.5).max(0.0);
            let hi = (ymid + th * 0.5).min(self.h as f32);
            let ys = lo.floor() as usize;
            let ye = hi.ceil() as usize;
            let xs = a.floor().max(0.0) as usize;
            let xe = b.ceil().min(self.w as f32) as usize;
            for yy in ys..ye.min(self.h) {
                let cov = seg_cov(yy as f32, yy as f32 + 1.0, lo, hi);
                if cov <= 0.0 {
                    continue;
                }
                for xx in xs..xe.min(self.w) {
                    let cc = col_cov(xx, a, b) * cov;
                    self.blend_cov(xx, yy, c, cc);
                }
            }
            return;
        }
        if (x1 - x0).abs() < 1e-4 {
            // vertical
            let (a, b) = if y0 <= y1 { (y0, y1) } else { (y1, y0) };
            let xmid = x0.max(x1);
            let lo = (xmid - th * 0.5).max(0.0);
            let hi = (xmid + th * 0.5).min(self.w as f32);
            let xs = lo.floor() as usize;
            let xe = hi.ceil() as usize;
            let ys = a.floor().max(0.0) as usize;
            let ye = b.ceil().min(self.h as f32) as usize;
            for xx in xs..xe.min(self.w) {
                let cov = seg_cov(xx as f32, xx as f32 + 1.0, lo, hi);
                if cov <= 0.0 {
                    continue;
                }
                for yy in ys..ye.min(self.h) {
                    let cc = col_cov(yy, a, b) * cov;
                    self.blend_cov(xx, yy, c, cc);
                }
            }
            return;
        }
        // diagonal — fine for tick diagonals
        let steps = (((x1 - x0).abs()).max((y1 - y0).abs())).ceil() as usize + 1;
        for i in 0..=steps.max(1) {
            let t = i as f32 / steps.max(1) as f32;
            let x = x0 + (x1 - x0) * t;
            let y = y0 + (y1 - y0) * t;
            self.dot(x, y, th, c);
        }
    }

    pub fn dot(&mut self, x: f32, y: f32, r: f32, c: Rgba) {
        self.fill_rect(x - r * 0.5, y - r * 0.5, r, r, c);
    }

    /// The `.tick` crosshair: an 11x11 box with a full-height vertical arm at
    /// x+5 and a full-width horizontal arm at y+5, minus the upper arm when
    /// `.tick-half`.
    pub fn tick(&mut self, x: f32, y: f32, c: Rgba, half: bool) {
        let t = crate::theme::TICK;
        // vertical arm
        if half {
            self.vline(x + 5.0, y + 5.0, t - 5.0, c);
        } else {
            self.vline(x + 5.0, y, t, c);
        }
        // horizontal arm
        self.hline(x, y + 5.0, t, c);
    }

    pub fn stroke_rect(&mut self, x: f32, y: f32, w: f32, h: f32, c: Rgba, th: f32) {
        if th <= 1.0 {
            // crisp hairline: inset by half so the 1px border sits inside
            let t = th.max(0.0);
            self.hline(x, y + t * 0.5, w, c);
            self.hline(x, y + h - t * 0.5, w, c);
            self.vline(x + t * 0.5, y, h, c);
            self.vline(x + w - t * 0.5, y, h, c);
        } else {
            let t = th;
            let hh = h;
            self.fill_rect(x, y, w, t, c);
            self.fill_rect(x, y + hh - t, w, t, c);
            self.fill_rect(x, y, t, hh, c);
            self.fill_rect(x + w - t, y, t, hh, c);
        }
    }

    /// The `.plan-card` gradient: `linear-gradient(to bottom, var(--app) X%, transparent)`.
    pub fn gradient_v(&mut self, x: f32, y: f32, w: f32, h: f32, top: Rgba, bottom: Rgba) {
        self.gradient(x, y, w, h, top, bottom);
    }

    pub fn gradient(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        c0: Rgba,
        c1: Rgba,
    ) {
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let ys = y.floor().max(0.0) as usize;
        let ye = ((y + h).ceil().max(0.0) as usize).min(self.h);
        for yy in ys..ye {
            let t = (yy as f32 + 0.5 - y) / h;
            let t = t.clamp(0.0, 1.0);
            let c = c0.lerp(c1, t);
            self.fill_rect(x, yy as f32, w, 1.0, c);
        }
    }

    /// Radial `closest-side` gradient — the hero scrim.
    pub fn radial(
        &mut self,
        cx: f32,
        cy: f32,
        radius: f32,
        inner: Rgba,
        outer: Rgba,
    ) {
        if radius <= 0.0 {
            return;
        }
        let x0 = (cx - radius).floor().max(0.0) as usize;
        let x1 = ((cx + radius).ceil().max(0.0) as usize).min(self.w);
        let y0 = (cy - radius).floor().max(0.0) as usize;
        let y1 = ((cy + radius).ceil().max(0.0) as usize).min(self.h);
        for yy in y0..y1 {
            for xx in x0..x1 {
                let dx = xx as f32 + 0.5 - cx;
                let dy = yy as f32 + 0.5 - cy;
                let d = (dx * dx + dy * dy).sqrt() / radius;
                if d >= 1.0 {
                    continue;
                }
                // smoothstep for a soft closest-side falloff
                let t = d * d * (3.0 - 2.0 * d);
                let c = inner.lerp(outer, t);
                self.blend_px(xx, yy, c);
            }
        }
    }

    /// Blit an RGBA8 image with bilinear scaling.
    pub fn image(
        &mut self,
        src: &[u8],
        sw: usize,
        sh: usize,
        dx: f32,
        dy: f32,
        dw: f32,
        dh: f32,
        alpha: f32,
    ) {
        if dw <= 0.0 || dh <= 0.0 || sw == 0 || sh == 0 {
            return;
        }
        let x0 = dx.floor().max(0.0) as usize;
        let x1 = ((dx + dw).ceil().max(0.0) as usize).min(self.w);
        let y0 = dy.floor().max(0.0) as usize;
        let y1 = ((dy + dh).ceil().max(0.0) as usize).min(self.h);
        for yy in y0..y1 {
            let v = (yy as f32 + 0.5 - dy) / dh;
            if v < 0.0 || v > 1.0 {
                continue;
            }
            let sy = ((v * sh as f32 - 0.5).clamp(0.0, sh as f32 - 1.0)) as usize;
            for xx in x0..x1 {
                let u = (xx as f32 + 0.5 - dx) / dw;
                if u < 0.0 || u > 1.0 {
                    continue;
                }
                let sx = ((u * sw as f32 - 0.5).clamp(0.0, sw as f32 - 1.0)) as usize;
                let si = (sy * sw + sx) * 4;
                if si + 3 >= src.len() {
                    continue;
                }
                let c = Rgba::rgba(
                    src[si] as f32 / 255.0,
                    src[si + 1] as f32 / 255.0,
                    src[si + 2] as f32 / 255.0,
                    src[si + 3] as f32 / 255.0 * alpha.clamp(0.0, 1.0),
                );
                self.blend_px(xx, yy, c);
            }
        }
    }

    /// Blit a glyph alpha mask, tinted with `c`.
    pub fn glyph(
        &mut self,
        mask: &[u8],
        gw: usize,
        gh: usize,
        dx: i32,
        dy: i32,
        c: Rgba,
    ) {
        if c.a <= 0.0 || gw == 0 || gh == 0 {
            return;
        }
        for gy in 0..gh {
            let yy = dy + gy as i32;
            if yy < 0 || yy as usize >= self.h {
                continue;
            }
            for gx in 0..gw {
                let xx = dx + gx as i32;
                if xx < 0 || xx as usize >= self.w {
                    continue;
                }
                let a = mask[gy * gw + gx] as f32 / 255.0;
                if a <= 0.0 {
                    continue;
                }
                self.blend_cov(xx as usize, yy as usize, c, a);
            }
        }
    }

/// `[data-reveal]` blur so its cost is bounded by the revealing element.
pub fn box_blur(&mut self, x: i32, y: i32, w: usize, h: usize, radius: i32) {
    if radius <= 0 || w == 0 || h == 0 {
        return;
    }
    let r = radius as usize;
    if w <= 2 * r || h <= 2 * r {
        return;
    }
    let x0 = x.max(0) as usize;
    let y0 = y.max(0) as usize;
    let x1 = ((x as usize) + w).min(self.w);
    let y1 = ((y as usize) + h).min(self.h);
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let ww = x1 - x0;
    let hh = y1 - y0;
    let mut tmp = vec![0u8; ww * hh * 4];
    for row in 0..hh {
        let mut acc = [0f32; 4];
        for k in 0..=(2 * r) {
            let sx = (k as isize - r as isize).clamp(0, ww as isize - 1) as usize;
            let i = ((y0 + row) * self.w + (x0 + sx)) * 4;
            for c in 0..4 {
                acc[c] += self.px[i + c] as f32;
            }
        }
        let n = (2 * r + 1) as f32;
        for col in 0..ww {
            let ti = (row * ww + col) * 4;
            for c in 0..4 {
                tmp[ti + c] = (acc[c] / n + 0.5) as u8;
            }
            let out = (col as isize - r as isize).clamp(0, ww as isize - 1) as usize;
            let inn = (col as isize + r as isize + 1).clamp(0, ww as isize - 1) as usize;
            let io = ((y0 + row) * self.w + (x0 + out)) * 4;
            let ii = ((y0 + row) * self.w + (x0 + inn)) * 4;
            for c in 0..4 {
                acc[c] += self.px[ii + c] as f32 - self.px[io + c] as f32;
            }
        }
    }
    for col in 0..ww {
        let mut acc = [0f32; 4];
        for k in 0..=(2 * r) {
            let sy = (k as isize - r as isize).clamp(0, hh as isize - 1) as usize;
            let i = (sy * ww + col) * 4;
            for c in 0..4 {
                acc[c] += tmp[i + c] as f32;
            }
        }
        let n = (2 * r + 1) as f32;
        for row in 0..hh {
            let oi = ((y0 + row) * self.w + (x0 + col)) * 4;
            for c in 0..4 {
                self.px[oi + c] = (acc[c] / n + 0.5) as u8;
            }
            let out = (row as isize - r as isize).clamp(0, hh as isize - 1) as usize;
            let inn = (row as isize + r as isize + 1).clamp(0, hh as isize - 1) as usize;
            let io = (out * ww + col) * 4;
            let ii = (inn * ww + col) * 4;
            for c in 0..4 {
                acc[c] += tmp[ii + c] as f32 - tmp[io + c] as f32;
            }
        }
    }
    }
}

#[inline]
fn col_cov(px: usize, a: f32, b: f32) -> f32 {
    let lo = px as f32;
    let hi = lo + 1.0;
    if a <= lo && b >= hi {
        1.0
    } else {
        let s = a.max(lo);
        let e = b.min(hi);
        (e - s).max(0.0).min(1.0)
    }
}

#[inline]
fn seg_cov(lo: f32, hi: f32, a: f32, b: f32) -> f32 {
    let s = a.max(lo);
    let e = b.min(hi);
    ((e - s) / (hi - lo)).clamp(0.0, 1.0)
}

