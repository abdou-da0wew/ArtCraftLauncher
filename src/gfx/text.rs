//! Text: the four ArtCraft typefaces, shaped with swash and rasterised into a
//! glyph cache.
//!
//! Font roles map to the site's bridges:
//! * `Display` → `--font-display: var(--font-archivo)`, rendered at
//!   `font-weight: 620; font-stretch: 118%` (`.font-display` in globals.css)
//! * `Body`    → `--font-body: var(--font-inter)`
//! * `Mono`    → `--font-geist-mono`, weights 400/500/700
//! * `Serif`   → `--font-instrument` (400 + italic)
//!
//! Everything is drawn through one method (`draw_text`) that destructures the
//! engine into disjoint field borrows, so the glyph cache can be mutated while
//! glyphs are being blitted.

use std::collections::HashMap;

use swash::scale::{Render, ScaleContext, Source};
use swash::zeno::{Format, Vector};
use swash::{FontRef, GlyphId, Setting};

use crate::gfx::canvas::Canvas;
use crate::theme::Rgba;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Role {
    /// Archivo, weight 620, stretch 118%.
    Display,
    /// Inter, weight 400.
    Body,
    /// Inter, weight 500.
    BodyMedium,
    /// Inter, weight 600.
    BodySemi,
    /// Geist Mono 400.
    Mono,
    /// Geist Mono 500 — what `.hud-label` uses.
    MonoMedium,
    /// Geist Mono 700 — what buttons use.
    MonoBold,
    /// Instrument Serif 400.
    Serif,
    /// Instrument Serif 400 italic.
    SerifItalic,
}

impl Role {
    pub fn data(self) -> &'static [u8] {
        match self {
            Role::Display => include_bytes!("../../assets/fonts/Archivo.ttf"),
            Role::Body | Role::BodyMedium | Role::BodySemi => {
                include_bytes!("../../assets/fonts/Inter.ttf")
            }
            Role::Mono => include_bytes!("../../assets/fonts/GeistMono-Regular.ttf"),
            Role::MonoMedium => include_bytes!("../../assets/fonts/GeistMono-Medium.ttf"),
            Role::MonoBold => include_bytes!("../../assets/fonts/GeistMono-Bold.ttf"),
            Role::Serif => include_bytes!("../../assets/fonts/InstrumentSerif-Regular.ttf"),
            Role::SerifItalic => include_bytes!("../../assets/fonts/InstrumentSerif-Italic.ttf"),
        }
    }

    /// Variation settings applied when building the scaler (user-space values).
    fn variations(self) -> &'static [(&'static [u8; 4], f32)] {
        match self {
            Role::Display => &[(b"wght", 620.0), (b"wdth", 118.0)],
            Role::BodyMedium => &[(b"opsz", 14.0), (b"wght", 500.0)],
            Role::BodySemi => &[(b"opsz", 14.0), (b"wght", 600.0)],
            Role::Body => &[(b"opsz", 14.0), (b"wght", 400.0)],
            _ => &[],
        }
    }

    /// Ascender as a fraction of em, used to place the first baseline.
    pub fn ascent(self) -> f32 {
        match self {
            Role::Display => 0.90,
            Role::Body | Role::BodyMedium | Role::BodySemi => 0.90,
            Role::Mono | Role::MonoMedium | Role::MonoBold => 0.92,
            Role::Serif | Role::SerifItalic => 0.94,
        }
    }

    /// Font family index used to look the role up in `AppFonts`.
    pub fn family(self) -> usize {
        match self {
            Role::Display => 0,
            Role::Body | Role::BodyMedium | Role::BodySemi => 1,
            Role::Mono | Role::MonoMedium | Role::MonoBold => 2,
            Role::Serif | Role::SerifItalic => 3,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct GlyphKey {
    role: Role,
    glyph_id: u16,
    size_q: u16,
}

#[derive(Clone)]
pub struct Glyph {
    pub mask: Vec<u8>,
    pub w: usize,
    pub h: usize,
    pub left: i32,
    /// Baseline → mask top (negative = above baseline).
    pub top: i32,
    pub advance: f32,
}

pub struct TextEngine {
    ctx: ScaleContext,
    fonts: HashMap<Role, FontRef<'static>>,
    coords: HashMap<Role, Vec<i16>>,
    map: HashMap<GlyphKey, Option<Glyph>>,
    mask_bytes: usize,
}

/// Total glyph mask bytes we're willing to keep.
const CACHE_BUDGET: usize = 16 * 1024 * 1024;

impl TextEngine {
    pub fn new() -> Self {
        let mut fonts = HashMap::new();
        let mut coords = HashMap::new();
        for r in [
            Role::Display,
            Role::Body,
            Role::BodyMedium,
            Role::BodySemi,
            Role::Mono,
            Role::MonoMedium,
            Role::MonoBold,
            Role::Serif,
            Role::SerifItalic,
        ] {
            let data: &'static [u8] = Box::leak(r.data().into());
            let f = FontRef::from_index(data, 0).expect("embedded font is valid");
            let mut settings: Vec<Setting<f32>> = Vec::new();
            for (tag, value) in r.variations() {
                let tag = swash::tag_from_bytes(tag);
                if f.variations().find_by_tag(tag).is_some() {
                    settings.push(Setting { tag, value: *value });
                }
            }
            let cs: Vec<i16> = f.variations().normalized_coords(settings).collect();
            fonts.insert(r, f);
            coords.insert(r, cs);
        }
        Self {
            ctx: ScaleContext::new(),
            fonts,
            coords,
            map: HashMap::new(),
            mask_bytes: 0,
        }
    }

    fn font(&self, role: Role) -> FontRef<'static> {
        *self.fonts.get(&role).expect("role loaded")
    }

    /// Horizontal advance of one character at `size`, in px.
    pub fn advance(&self, role: Role, ch: char, size: f32) -> f32 {
        let font = self.font(role);
        let gid = font.charmap().map(ch);
        let cs = self.coords.get(&role).map(|v| v.as_slice()).unwrap_or(&[]);
        font.glyph_metrics(cs).scale(size).advance_width(gid)
    }

    /// Width of a single line, including tracking.
    pub fn measure(&self, text: &str, role: Role, size: f32, tracking: f32) -> f32 {
        let mut total = 0.0;
        for (i, ch) in text.chars().enumerate() {
            if i > 0 {
                total += tracking;
            }
            total += self.advance(role, ch, size);
        }
        total
    }

    /// Largest prefix of `text` that fits in `max_w`.
    pub fn fit(&self, text: &str, role: Role, size: f32, tracking: f32, max_w: f32) -> usize {
        let mut w = 0.0;
        let mut n = 0;
        for (i, ch) in text.chars().enumerate() {
            let aw = self.advance(role, ch, size) + if i > 0 { tracking } else { 0.0 };
            if w + aw > max_w && n > 0 {
                break;
            }
            w += aw;
            n += 1;
        }
        n
    }

    /// Draw one line. `(x, y)` is the top-left of the em box.
    /// Returns the ink width.
    pub fn draw_text(
        &mut self,
        canvas: &mut Canvas,
        text: &str,
        role: Role,
        size: f32,
        tracking: f32,
        x: f32,
        y: f32,
        color: Rgba,
        alpha: f32,
    ) -> f32 {
        if text.is_empty() || alpha <= 0.0 || color.a <= 0.0 {
            return 0.0;
        }
        let TextEngine {
            ctx,
            fonts,
            coords,
            map,
            mask_bytes,
        } = self;
        let font = *fonts.get(&role).expect("role loaded");
        let cs: Vec<i16> = coords.get(&role).cloned().unwrap_or_default();
        let baseline = y + role.ascent() * size;

        let mut pen = 0.0f32;
        let mut plan: Vec<(i32, i32, GlyphKey)> = Vec::with_capacity(text.len());
        for (i, ch) in text.chars().enumerate() {
            if i > 0 {
                pen += tracking;
            }
            let gid: GlyphId = font.charmap().map(ch);
            let key = GlyphKey {
                role,
                glyph_id: gid,
                size_q: (size.max(0.0) * 64.0) as u16,
            };
            let adv = font.glyph_metrics(&cs).scale(size).advance_width(gid);
            if !map.contains_key(&key) {
                let g = rasterize(ctx, font, &cs, gid, size, adv);
                if let Some(g) = &g {
                    *mask_bytes += g.mask.len();
                }
                map.insert(key, g);
                if *mask_bytes > CACHE_BUDGET {
                    map.clear();
                    *mask_bytes = 0;
                }
            }
            if let Some(g) = map.get(&key).and_then(|g| g.as_ref()) {
                plan.push((
                    (x + pen + g.left as f32) as i32,
                    (baseline - g.top as f32) as i32,
                    key,
                ));
            }
            pen += adv;
        }
        let c = color.with_alpha(color.a * alpha);
        for (gx, gy, key) in plan {
            if let Some(g) = map.get(&key).and_then(|g| g.as_ref()) {
                canvas.glyph(&g.mask, g.w, g.h, gx, gy, c);
            }
        }
        pen
    }

    pub fn draw_text_right(
        &mut self,
        canvas: &mut Canvas,
        text: &str,
        role: Role,
        size: f32,
        tracking: f32,
        x_right: f32,
        y: f32,
        color: Rgba,
        alpha: f32,
    ) -> f32 {
        let w = self.measure(text, role, size, tracking);
        self.draw_text(canvas, text, role, size, tracking, x_right - w, y, color, alpha)
    }

    pub fn draw_text_centered(
        &mut self,
        canvas: &mut Canvas,
        text: &str,
        role: Role,
        size: f32,
        tracking: f32,
        cx: f32,
        y: f32,
        color: Rgba,
        alpha: f32,
    ) -> f32 {
        let w = self.measure(text, role, size, tracking);
        self.draw_text(canvas, text, role, size, tracking, cx - w * 0.5, y, color, alpha)
    }

    /// Word-wrap `text` to `max_w`, returning the lines and total height.
    pub fn wrap(&self, text: &str, role: Role, size: f32, tracking: f32, max_w: f32) -> Vec<String> {
        let mut lines = Vec::new();
        let mut cur = String::new();
        let mut cur_w = 0.0;
        for word in text.split(' ') {
            let ww = self.measure(word, role, size, tracking);
            let space = if cur.is_empty() { 0.0 } else { self.advance(role, ' ', size) };
            if cur_w + space + ww > max_w && !cur.is_empty() {
                lines.push(std::mem::take(&mut cur));
                cur_w = 0.0;
                cur.push_str(word);
                cur_w += ww;
            } else {
                if !cur.is_empty() {
                    cur.push(' ');
                    cur_w += space;
                }
                cur.push_str(word);
                cur_w += ww;
            }
        }
        if !cur.is_empty() {
            lines.push(cur);
        }
        lines
    }

    /// Draw wrapped text; returns the number of lines drawn.
    pub fn draw_paragraph(
        &mut self,
        canvas: &mut Canvas,
        text: &str,
        role: Role,
        size: f32,
        tracking: f32,
        x: f32,
        y: f32,
        max_w: f32,
        line_h: f32,
        color: Rgba,
        alpha: f32,
    ) -> usize {
        let lines = self.wrap(text, role, size, tracking, max_w);
        for (i, l) in lines.iter().enumerate() {
            self.draw_text(
                canvas, l, role, size, tracking, x, y + i as f32 * line_h, color, alpha,
            );
        }
        lines.len()
    }

    /// Uppercase-with-tracking variant used by `.hud-label`, which in CSS also
    /// applies `letter-spacing: .15em` to the *untransformed* text.
    pub fn draw_hud(
        &mut self,
        canvas: &mut Canvas,
        text: &str,
        size: f32,
        x: f32,
        y: f32,
        color: Rgba,
        alpha: f32,
    ) -> f32 {
        self.draw_text(canvas, text, Role::MonoMedium, size, size * 0.15, x, y, color, alpha)
    }
}

fn rasterize(
    ctx: &mut ScaleContext,
    font: FontRef<'_>,
    coords: &[i16],
    gid: GlyphId,
    size: f32,
    advance: f32,
) -> Option<Glyph> {
    let mut scaler = ctx
        .builder(font)
        .size(size)
        .hint(false)
        .normalized_coords(coords.iter().copied())
        .build();
    let image = Render::new(&[Source::Outline])
        .format(Format::Alpha)
        .offset(Vector::new(0.0, 0.0))
        .render(&mut scaler, gid)?;
    if image.data.is_empty() || image.placement.width == 0 || image.placement.height == 0 {
        return None;
    }
    // Swash delivers coverage row-major top-down, which is exactly what the
    // canvas wants — no flip. (An earlier version flipped it here, which
    // rendered every glyph upside-down.)
    let w = image.placement.width as usize;
    let h = image.placement.height as usize;
    let mask = &image.data[..w * h];
    Some(Glyph {
        mask: mask.to_vec(),
        w,
        h,
        left: image.placement.left,
        top: image.placement.top,
        advance,
    })
}
