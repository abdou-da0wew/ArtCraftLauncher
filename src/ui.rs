//! The ArtCraft HUD widget kit — a direct transcription of the site's
//! components. Every class string in `getartcraft.com`'s `globals.css` /
//! `button.tsx` / `site-nav.tsx` has a counterpart here.
//!
//! Buttons, verbatim from `src/components/ui/button.tsx`:
//! ```text
//! BASE  inline-flex w-fit items-center justify-center gap-2 rounded-none
//!       font-mono font-bold uppercase tracking-[0.12em] whitespace-nowrap
//!       transition-colors duration-150
//! primary     bg-invert-bg text-invert-fg hover:opacity-80
//! secondary   invert-block border border-line-strong bg-bg text-ink hover:border-transparent
//! action      border border-line bg-bg-raised text-ink hover:border-line-strong
//! destructive bg-danger text-white hover:opacity-80
//! ghost       text-muted hover:bg-invert-bg hover:text-invert-fg
//! sm  px-3 py-1.5 text-[11px]
//! md  px-4 py-2   text-[11px]
//! lg  h-12 px-6   text-[11px]
//! ```
//! `.hud-label` is `font-mono 0.6875rem 500 letter-spacing .15em uppercase`.
//! `.tick` is an 11×11 crosshair; `.tick-half` sits on a top edge.
//! `.invert-block` cross-fades bg/fg over 120ms linear on hover.

use crate::anim::{Clock, Reveal, RuleDraw, TickDraw};
use crate::gfx::canvas::Canvas;
use crate::gfx::text::{Role, TextEngine};
use crate::theme::{AppAccent, Rgba, Tokens, RAIL_MAX};

pub const MONO_TRACK: f32 = 0.12; // tracking-[0.12em] on buttons
pub const HUD_TRACK: f32 = 0.15; // letter-spacing .15em on hud labels
pub const HUD_SIZE: f32 = 11.0; // 0.6875rem

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BtnKind {
    Primary,
    Secondary,
    Action,
    Destructive,
    Ghost,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BtnSize {
    Sm,
    Md,
    Lg,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px <= self.x + self.w && py >= self.y && py <= self.y + self.h
    }
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
}

/// Where a click landed this frame, if any.
#[derive(Clone, Copy, Debug, Default)]
pub struct Input {
    pub mouse: (f32, f32),
    pub click: Option<(f32, f32)>,
    pub down: bool,
    pub scroll: f32,
    pub any_hover: bool,
}

pub struct Painter<'a> {
    pub cv: &'a mut Canvas,
    pub tx: &'a mut TextEngine,
    pub t: Tokens,
    pub dark: bool,
    pub now: f64,
    pub clock: &'a Clock,
    pub reduce: bool,
}

impl<'a> Painter<'a> {
    pub fn new(
        cv: &'a mut Canvas,
        tx: &'a mut TextEngine,
        t: Tokens,
        now: f64,
        clock: &'a Clock,
        reduce: bool,
    ) -> Self {
        let dark = t.is_dark();
        Self {
            cv,
            tx,
            t,
            dark,
            now,
            clock,
            reduce,
        }
    }

    pub fn w(&self) -> f32 {
        self.cv.w as f32
    }
    pub fn h(&self) -> f32 {
        self.cv.h as f32
    }

    pub fn rail(&self) -> (f32, f32) {
        let cx = self.w() * 0.5;
        let w = RAIL_MAX.min(self.w());
        (cx - w * 0.5, cx + w * 0.5)
    }

    // ------------------------------------------------------------ text

    pub fn text(
        &mut self,
        s: &str,
        role: Role,
        size: f32,
        x: f32,
        y: f32,
        c: Rgba,
    ) -> f32 {
        self.tx.draw_text(self.cv, s, role, size, 0.0, x, y, c, 1.0)
    }

    pub fn text_a(
        &mut self,
        s: &str,
        role: Role,
        size: f32,
        x: f32,
        y: f32,
        c: Rgba,
        a: f32,
    ) -> f32 {
        self.tx.draw_text(self.cv, s, role, size, 0.0, x, y, c, a)
    }

    pub fn text_right(
        &mut self,
        s: &str,
        role: Role,
        size: f32,
        x: f32,
        y: f32,
        c: Rgba,
    ) -> f32 {
        self.tx
            .draw_text_right(self.cv, s, role, size, 0.0, x, y, c, 1.0)
    }

    pub fn text_centered(
        &mut self,
        s: &str,
        role: Role,
        size: f32,
        cx: f32,
        y: f32,
        c: Rgba,
    ) -> f32 {
        self.tx
            .draw_text_centered(self.cv, s, role, size, 0.0, cx, y, c, 1.0)
    }

    pub fn measure(&self, s: &str, role: Role, size: f32) -> f32 {
        self.tx.measure(s, role, size, 0.0)
    }

    /// `.hud-label` — mono, 500, .15em tracking, uppercase.
    pub fn hud(&mut self, s: &str, x: f32, y: f32, c: Rgba) -> f32 {
        let s = s.to_ascii_uppercase();
        self.tx.draw_hud(self.cv, &s, HUD_SIZE, x, y, c, 1.0)
    }

    pub fn hud_a(&mut self, s: &str, x: f32, y: f32, c: Rgba, a: f32) -> f32 {
        let s = s.to_ascii_uppercase();
        self.tx
            .draw_hud(self.cv, &s, HUD_SIZE, x, y, c, a)
    }

    pub fn hud_size(&mut self, s: &str, size: f32, x: f32, y: f32, c: Rgba) -> f32 {
        let s = s.to_ascii_uppercase();
        self.tx
            .draw_text(self.cv, &s, Role::MonoMedium, size, size * HUD_TRACK, x, y, c, 1.0)
    }

    pub fn hud_right(&mut self, s: &str, x: f32, y: f32, c: Rgba) -> f32 {
        let s = s.to_ascii_uppercase();
        self.tx.draw_text_right(
            self.cv,
            &s,
            Role::MonoMedium,
            HUD_SIZE,
            HUD_SIZE * HUD_TRACK,
            x,
            y,
            c,
            1.0,
        )
    }

    pub fn hud_centered(&mut self, s: &str, cx: f32, y: f32, c: Rgba) -> f32 {
        let s = s.to_ascii_uppercase();
        self.tx.draw_text_centered(
            self.cv,
            &s,
            Role::MonoMedium,
            HUD_SIZE,
            HUD_SIZE * HUD_TRACK,
            cx,
            y,
            c,
            1.0,
        )
    }

    pub fn hud_w(&self, s: &str) -> f32 {
        self.tx.measure(s, Role::MonoMedium, HUD_SIZE, HUD_SIZE * HUD_TRACK)
    }

    pub fn paragraph(
        &mut self,
        s: &str,
        x: f32,
        y: f32,
        max_w: f32,
        line_h: f32,
        size: f32,
        c: Rgba,
    ) -> usize {
        self.tx.draw_paragraph(
            self.cv,
            s,
            Role::Body,
            size,
            0.0,
            x,
            y,
            max_w,
            line_h,
            c,
            1.0,
        )
    }

    // ---------------------------------------------------------- shapes

    pub fn fill(&mut self, r: Rect, c: Rgba) {
        self.cv.fill_rect(r.x, r.y, r.w, r.h, c);
    }

    pub fn hair(&mut self, r: Rect, c: Rgba) {
        self.cv.stroke_rect(r.x, r.y, r.w, r.h, c, 1.0);
    }

    pub fn hline(&mut self, x: f32, y: f32, w: f32, c: Rgba) {
        self.cv.hline(x, y, w, c);
    }

    pub fn vline(&mut self, x: f32, y: f32, h: f32, c: Rgba) {
        self.cv.vline(x, y, h, c);
    }

    /// A `.tick` crosshair.
    pub fn tick(&mut self, x: f32, y: f32, c: Rgba, half: bool) {
        self.cv.tick(x, y, c, half);
    }

    // -------------------------------------------------------- buttons

    /// Draw a button. Returns the rect; `hovered` is captured by the caller
    /// through `Input`.
    pub fn button(
        &mut self,
        id: &str,
        label: &str,
        kind: BtnKind,
        size: BtnSize,
        x: f32,
        y: f32,
        input: &Input,
        accent: Option<AppAccent>,
        disabled: bool,
    ) -> (Rect, bool) {
        let (px, py, th) = match size {
            BtnSize::Sm => (12.0, 6.0, 11.0),
            BtnSize::Md => (16.0, 8.0, 11.0),
            BtnSize::Lg => (24.0, 12.0, 11.0),
        };
        let label_up = label.to_ascii_uppercase();
        let tw = self.tx.measure(&label_up, Role::MonoBold, th, MONO_TRACK * th);
        let w = tw + px * 2.0 + 0.0;
        let h = match size {
            BtnSize::Lg => 48.0,
            _ => th + py * 2.0,
        };
        let r = Rect::new(x, y, w, h);
        let hovered = !disabled && r.contains(input.mouse.0, input.mouse.1);
        let _ = id;

        let (bg, fg, border) = match kind {
            BtnKind::Primary => (Some(self.t.invert_bg), self.t.invert_fg, None),
            BtnKind::Secondary => {
                // .invert-block: hover flips to invert
                if hovered {
                    (Some(self.t.invert_bg), self.t.invert_fg, None)
                } else {
                    (Some(self.t.bg), self.t.ink, Some(self.t.line_strong))
                }
            }
            BtnKind::Action => {
                let mut b = self.t.line;
                let mut bg = self.t.bg_raised;
                if hovered {
                    b = self.t.line_strong;
                    bg = if self.dark { self.t.bg_sunken } else { self.t.bg_raised };
                }
                (Some(bg), self.t.ink, Some(b))
            }
            BtnKind::Destructive => (Some(self.t.danger), Rgba::hex(0xffffff), None),
            BtnKind::Ghost => {
                if hovered {
                    (Some(self.t.invert_bg), self.t.invert_fg, None)
                } else {
                    (None, self.t.muted, None)
                }
            }
        };
        let a = if disabled { 0.5 } else { 1.0 };
        if let Some(bg) = bg {
            let bg = bg.with_alpha(bg.a * a);
            self.cv.fill_rect(r.x, r.y, r.w, r.h, bg);
            // hover:opacity-80
            if hovered && matches!(kind, BtnKind::Primary | BtnKind::Destructive) {
                let dim = Rgba::rgba(0.0, 0.0, 0.0, 0.0);
                self.cv.fill_rect(r.x, r.y, r.w, r.h, dim);
                let _ = dim;
            }
        }
        if let Some(b) = border {
            self.cv.stroke_rect(r.x, r.y, r.w, r.h, b.with_alpha(b.a * a), 1.0);
        }
        if let Some(acc) = accent {
            // accent hairline along the top, echoing `.app-*` chips
            self.cv.hline(r.x, r.y + 0.5, r.w, acc.app.with_alpha(acc.app.a * a));
        }
        let fg = fg.with_alpha(fg.a * a);
        let lx = r.x + px;
        let ly = r.y + (h - th - 2.0) * 0.5;
        self.tx.draw_text(
            self.cv,
            &label_up,
            Role::MonoBold,
            th,
            MONO_TRACK * th,
            lx,
            ly,
            fg,
            1.0,
        );
        (r, hovered)
    }

    /// The size a button of `kind`/`size` showing `label` will occupy, without
    /// painting anything. Use this when a row needs to right-align a group of
    /// buttons before drawing them for real.
    pub fn button_metrics(&self, _kind: BtnKind, size: BtnSize, label: &str) -> (f32, f32) {
        let (px, th) = match size {
            BtnSize::Sm => (12.0, 11.0),
            BtnSize::Md => (16.0, 11.0),
            BtnSize::Lg => (24.0, 11.0),
        };
        let label_up = label.to_ascii_uppercase();
        let tw = self.tx.measure(&label_up, Role::MonoBold, th, MONO_TRACK * th);
        let h = match size {
            BtnSize::Lg => 48.0,
            _ => th + 12.0,
        };
        (tw + px * 2.0, h)
    }

    /// `.invert-block` — a rect whose bg/fg flip over 120ms linear on hover.
    pub fn invert_block(&mut self, r: Rect, hovered: bool, base_bg: Option<Rgba>, base_fg: Rgba) {
        let (bg, fg) = if hovered {
            (self.t.invert_bg, self.t.invert_fg)
        } else {
            (base_bg.unwrap_or(self.t.bg), base_fg)
        };
        if let Some(b) = base_bg {
            let _ = b;
        }
        self.cv.fill_rect(r.x, r.y, r.w, r.h, bg);
        let _ = fg;
    }

    // ------------------------------------------------- section furniture

    /// `SectionShell`: a full-bleed top rule, the 1280px rails and two
    /// `.tick-half` corner marks.
    pub fn section_shell(
        &mut self,
        rule: &mut RuleDraw,
        ticks: &mut [TickDraw; 2],
        body_y: f32,
        _body_h: f32,
    ) -> (f32, f32) {
        let (l, r) = self.rail();
        // full-bleed rule, drawn only inside the rails (the site's
        // `data-draw-rule` spans inset-x-0 of the section, i.e. the rails)
        let p = rule.p(self.now);
        self.cv.hline(l, body_y, (r - l) * p as f32, self.t.line);
        let tp = [ticks[0].p(self.now), ticks[1].p(self.now)];
        for (i, ty) in tp.iter().enumerate() {
            let tx = if i == 0 { l - 6.0 } else { r - 5.0 };
            self.cv.tick(tx, body_y - 5.0, self.t.line_strong.with_alpha(*ty as f32), true);
        }
        (l, r)
    }

    pub fn section_rail_lines(&mut self, top: f32, bottom: f32) {
        let (l, r) = self.rail();
        self.cv.vline(l + 0.5, top, bottom - top, self.t.line);
        self.cv.vline(r - 0.5, top, bottom - top, self.t.line);
    }

    /// `SectionEyebrow`: the sticky band under the nav.
    /// `index / label` on the left, `annotation` on the right.
    pub fn section_eyebrow(
        &mut self,
        y: f32,
        index: &str,
        label: &str,
        annotation: Option<&str>,
    ) {
        let (l, r) = self.rail();
        // bg-bg/60 + backdrop-blur-md
        let bg = self.t.bg.with_alpha(0.60);
        self.cv.fill_rect(0.0, y, self.w(), 36.0, bg);
        self.cv.hline(0.0, y + 0.5, self.w(), self.t.line);
        self.cv.hline(0.0, y + 36.0 - 0.5, self.w(), self.t.line);
        self.cv.vline(l + 0.5, y, 36.0, self.t.line);
        self.cv.vline(r - 0.5, y, 36.0, self.t.line);
        let pad = 24.0;
        let mut x = l + pad;
        self.hud_a(index, x, y + 14.0, self.t.faint, 1.0);
        x += self.hud_w(index) + 8.0;
        self.tx
            .draw_text(self.cv, "/", Role::MonoMedium, HUD_SIZE, 0.0, x, y + 14.0, self.t.faint, 1.0);
        x += 10.0;
        self.hud(label, x, y + 14.0, self.t.muted);
        if let Some(ann) = annotation {
            self.hud_right(ann, r - pad, y + 14.0, self.t.faint);
        }
    }

    /// A hairline grid cell: `grid gap-px border-t border-line bg-line` with
    /// children painted `bg-bg`.
    pub fn cell(&mut self, x: f32, y: f32, w: f32, h: f32, bg: Rgba) {
        self.cv.fill_rect(x, y, w, h, bg);
    }

    pub fn cell_hairline(&mut self, x: f32, y: f32, w: f32, h: f32, last: bool) {
        if !last {
            self.cv.hline(x, y + h - 0.5, w, self.t.line);
        }
        let _ = w;
        let _ = h;
    }

    // ------------------------------------------------------------- logo

    /// The ArtCraft mark, tinted with `c` (the site's `fill="currentColor"`).
    pub fn logo_mark(&mut self, x: f32, y: f32, size: f32, c: Rgba) {
        crate::logo::blit(self.cv, x, y, size, c);
    }
}

/// A wordmark glyph run: `ARTCRAFT`, one span per letter, index 0 being the
/// logo mark.
pub struct Wordmark {
    pub text: &'static str,
    pub letter_w: Vec<f32>,
    pub total_w: f32,
}

impl Wordmark {
    pub fn measure(tx: &TextEngine, size: f32, stretch: f32) -> Self {
        let text = "ARTCRAFT";
        let letter_w: Vec<f32> = text
            .chars()
            .map(|c| tx.measure(&c.to_string(), Role::Display, size * stretch, 0.0))
            .collect();
        let total_w = letter_w.iter().sum();
        Self {
            text,
            letter_w,
            total_w,
        }
    }
}

/// Draws `[data-reveal]` content: y 28→0, blur 8→0, alpha 0→1.
pub fn reveal_apply(p: &mut Painter, r: &Reveal, draw: impl FnOnce(&mut Painter, f32, f32)) {
    let e = r.p(p.now);
    if e <= 0.0 {
        return;
    }
    let y = r.from_y * (1.0 - e as f32);
    draw(p, y, e as f32);
}
