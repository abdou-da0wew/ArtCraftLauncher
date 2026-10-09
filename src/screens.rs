//! The launcher's screens, drawn with the ArtCraft HUD language.
//!
//! Layout mirrors the site: a sticky 48px nav, hairline rails at 1280px, a
//! sticky `index / label / annotation` eyebrow under the nav, and a
//! `gap-px bg-line` grid whose children are painted `bg-bg`. Every button is
//! a verbatim `button.tsx` variant.


use crate::anim::{Marquee, Odometer};
use crate::apps::{CraftApp, Platform};
use crate::config::Settings;
use crate::gfx::text::Role;
use crate::state::{Launcher, Nav, Screen, ToastKind};
use crate::theme::{Rgba, NAV_H, RAIL_MAX};
use crate::ui::{BtnKind, BtnSize, Input, Painter, Rect};

/// Where the mouse is and what it did this frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct Frame {
    pub mouse: (f32, f32),
    pub click: Option<(f32, f32)>,
    pub down: bool,
    pub wheel: f32,
}

impl From<Frame> for Input {
    fn from(f: Frame) -> Self {
        Input {
            mouse: f.mouse,
            click: f.click,
            down: f.down,
            scroll: f.wheel,
            any_hover: false,
        }
    }
}

/// Something the shell must do outside of drawing.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    None,
    Launch(String, String),
    Install(String, String),
    Prompt(String, String),
    AcceptPrompt,
    DeclinePrompt,
    Rollback(String),
    Backup(String),
    Restore(String),
    Wipe(String),
    Integrate(String),
    CheckAll,
    Goto(Screen),
    Select(usize),
    Theme(crate::theme::Theme),
    Export,
    Import,
    OpenData(String),
    SetEndpoint(String),
    SetTelemetry(bool),
    ToggleIntegration(&'static str),
    ClearSpool,
    Quit,
}

pub const NAV_W: f32 = 232.0;

/// Everything the draw pass needs that is not the painter itself.
pub struct Env<'a> {
    pub now: f64,
    pub intro: f64,
    pub reveal: f64,
    pub scroll: f32,
    pub scroll_max: f32,
    pub marquee: &'a Marquee,
    pub odo: &'a Odometer,
    pub shows: f64,
}

/// Draw the whole window. Returns the action the shell should take.
pub fn draw(p: &mut Painter, l: &mut Launcher, f: Frame, e: &Env<'_>) -> Action {
    let mut input: Input = f.into();
    input.any_hover = true;
    let t = p.t;
    let _w = p.w();
    let h = p.h();

    p.cv.clear(t.bg);

    // ---- sticky top nav -------------------------------------------------
    let nav_action = top_nav(p, l, &input);

    // ---- body -----------------------------------------------------------
    let body_y = NAV_H + if e.shows > 0.0 { 0.0 } else { 0.0 };
    let (l_rail, r_rail) = p.rail();

    let (rule, ticks, action) = match l.screen {
        Screen::Library => {
            let (r, k, a) = draw_library(p, l, &input, e, body_y);
            (r, k, if a != Action::None { a } else { nav_action })
        }
        Screen::App => {
            let (r, k, a) = draw_app(p, l, &input, e, body_y);
            (r, k, if a != Action::None { a } else { nav_action })
        }
        Screen::Updates => {
            let (r, k, a) = draw_updates(p, l, &input, e, body_y);
            (r, k, if a != Action::None { a } else { nav_action })
        }
        Screen::Settings => {
            let (r, k, a) = draw_settings(p, l, &input, e, body_y);
            (r, k, if a != Action::None { a } else { nav_action })
        }
        Screen::Telemetry => {
            let (r, k, a) = draw_reports(p, l, &input, e, body_y);
            (r, k, if a != Action::None { a } else { nav_action })
        }
    };

    // The rails and the top rule run the full height of the body.
    let mut rule = rule;
    let mut ticks = ticks;
    p.section_shell(&mut rule, &mut ticks, body_y, h);
    let _ = (l_rail, r_rail);

    // ---- update prompt modal -------------------------------------------
    if let Some((slug, version)) = l.prompt.clone() {
        draw_prompt(p, l, &input, e, &slug, &version);
    }

    // ---- toasts ---------------------------------------------------------
    draw_toasts(p, l, e.now);

    // ---- scroll ruler instrument ---------------------------------------
    draw_ruler(p, e, h);

    action
}

fn top_nav(p: &mut Painter, l: &mut Launcher, input: &Input) -> Action {
    let t = p.t;
    let w = p.w();
    let h = NAV_H;

    // bg-bg/95 + backdrop-blur-sm
    let bg = t.bg.with_alpha(0.95);
    p.cv.fill_rect(0.0, 0.0, w, h, bg);
    p.cv.hline(0.0, h - 0.5, w, t.line);
    p.cv.vline(RAIL_MAX.min(w) + w * 0.0 - (w - (RAIL_MAX.min(w))) * 0.0 + (w - RAIL_MAX.min(w)) * 0.0 - (w - (w)) * 0.0, 0.0, 0.0, t.line);
    let rail = RAIL_MAX.min(w);
    p.cv.vline(0.5, 0.0, h, t.line);
    p.cv.vline(rail - 0.5, 0.0, h, t.line);
    let cx = w * 0.5;
    let left = cx - rail * 0.5;
    let right = cx + rail * 0.5;

    // logo cell: border-r px-4 sm:px-5, hover:opacity-70
    let logo_w = 56.0;
    let logo_hover = Rect::new(left, 0.0, logo_w, h).contains(input.mouse.0, input.mouse.1);
    let logo_alpha = if logo_hover { 0.7 } else { 1.0 };
    p.logo_mark(left + 18.0, h * 0.5 - 11.0, 24.0, t.ink_strong.with_alpha(logo_alpha));
    p.cv.vline(left + logo_w - 0.5, 0.0, h, t.line);

    // nav items, each `group flex h-full items-center px-2.5 hud-label`, with the
    // label wrapped in a chip that inverts on hover.
    let items: [(&str, i32); 4] = [
        ("Apps", 0),
        ("Updates", 1),
        ("Settings", 2),
        ("Reports", 3),
    ];
    let mut x = left + logo_w + 10.0;
    for (label, idx) in items {
        let active = match idx {
            0 => l.nav == Nav::Studio,
            1 => l.nav == Nav::Updates,
            2 => l.nav == Nav::Settings,
            _ => l.nav == Nav::Reports,
        };
        let pending = idx == 1 && !l.pending_updates().is_empty();
        let tw = p.hud_w(label);
        let chip_w = tw + 12.0;
        let cell = Rect::new(x, 0.0, chip_w + 20.0, h);
        let hovered = cell.contains(input.mouse.0, input.mouse.1);
        let chip = Rect::new(x + 10.0, h * 0.5 - 10.0, chip_w, 20.0);
        // NAV_LABEL_ACTIVE_CLASSES = bg-invert-bg text-invert-fg
        let (bg, fg) = if active {
            (Some(t.invert_bg), t.invert_fg)
        } else if hovered {
            (Some(t.invert_bg), t.invert_fg)
        } else {
            (None, t.muted)
        };
        if let Some(b) = bg {
            p.cv.fill_rect(chip.x, chip.y, chip.w, chip.h, b);
        }
        p.hud(label, chip.x + 6.0, chip.y + 5.0, fg);
        if pending {
            // a small dot marking pending updates
            p.cv.fill_rect(chip.right() - 5.0, chip.y + 4.0, 3.0, 3.0, t.danger);
        }
        if hovered && input.click.map(|c| cell.contains(c.0, c.1)).unwrap_or(false) {
            l.nav = match idx {
                0 => Nav::Studio,
                1 => Nav::Updates,
                2 => Nav::Settings,
                _ => Nav::Reports,
            };
            l.screen = l.nav.screen();
            l.selected = 0;
            l.scroll = 0.0;
            return Action::Goto(l.screen);
        }
        x += chip_w + 20.0;
    }

    // right cluster: theme toggle + Launch
    let mut rx = right;
    // theme toggle: w-12 border-l border-line, invert-block
    let tt = Rect::new(rx - 48.0, 0.0, 48.0, h);
    let tt_h = tt.contains(input.mouse.0, input.mouse.1);
    let (tbg, tfg) = if tt_h {
        (t.invert_bg, t.invert_fg)
    } else {
        (t.bg, t.muted)
    };
    p.cv.fill_rect(tt.x, tt.y, tt.w, tt.h, tbg);
    let _ = tfg;
    // sun / moon glyph
    let cxm = tt.x + 24.0;
    let cym = h * 0.5;
    if l.settings.theme == crate::theme::Theme::Dark {
        p.cv.fill_rect(cxm - 4.5, cym - 0.5, 9.0, 1.0, tfg);
        p.cv.fill_rect(cxm - 4.5, cym + 0.5, 9.0, 1.0, tfg);
        p.cv.vline(cxm - 4.5, cym - 2.0, 4.0, tfg);
        p.cv.vline(cxm + 3.5, cym - 2.0, 4.0, tfg);
    } else {
        for (dx, dy, dw, dh) in [
            (-0.5, -6.0, 1.0, 12.0),
            (-6.0, -0.5, 12.0, 1.0),
            (-4.0, -4.0, 1.0, 8.0),
            (3.0, -4.0, 1.0, 8.0),
            (-4.0, -4.0, 8.0, 1.0),
            (-4.0, 3.0, 8.0, 1.0),
        ] {
            p.cv.fill_rect(cxm + dx, cym + dy, dw, dh, tfg);
        }
    }
    p.cv.vline(tt.x - 0.5, 0.0, h, t.line);
    if tt_h && input.click.map(|c| tt.contains(c.0, c.1)).unwrap_or(false) {
        l.settings.theme = l.settings.theme.next();
        let _ = l.settings.save();
        return Action::Theme(l.settings.theme);
    }
    rx -= 48.0;

    // "Check" action — h-12 px-5, border-l
    let cw = 96.0;
    let cb = Rect::new(rx - cw, 0.0, cw, h);
    let cb_h = cb.contains(input.mouse.0, input.mouse.1);
    let (cbg, cfg) = if cb_h {
        (t.invert_bg, t.invert_fg)
    } else {
        (t.bg, t.muted)
    };
    p.cv.fill_rect(cb.x, cb.y, cb.w, cb.h, cbg);
    p.hud("Check", cb.x + 24.0, h * 0.5 - 6.0, cfg);
    p.cv.vline(cb.x - 0.5, 0.0, h, t.line);
    if cb_h && input.click.map(|c| cb.contains(c.0, c.1)).unwrap_or(false) {
        return Action::CheckAll;
    }
    Action::None
}

// --------------------------------------------------------------- library

fn draw_library(
    p: &mut Painter,
    l: &mut Launcher,
    input: &Input,
    e: &Env,
    body_y: f32,
) -> (crate::anim::RuleDraw, [crate::anim::TickDraw; 2], Action) {
    let t = p.t;
    let (left, right) = p.rail();
    let cx = (left + right) * 0.5;
    let inner = right - left - 96.0;
    let mut y = body_y + 40.0;

    // ---- hero: the wordmark, exactly as the site forms it ----------------
    let reveal = e.reveal.clamp(0.0, 1.0);
    if reveal > 0.0 {
        // Fit the wordmark inside the rail: measure, then scale to 82% of the
        // available width so it never runs off the page.
        let probe = 100.0f32;
        let w0 = crate::ui::Wordmark::measure(p.tx, probe, 1.0);
        let logo_cap_frac = 0.70f32; // wordmarkDefaults.logoCap
        let word_w = w0.total_w + probe * logo_cap_frac * 0.62;
        let size = (inner * 0.82 / word_w * probe).min(112.0).max(28.0);
        let wm = crate::ui::Wordmark::measure(p.tx, size, 1.0);
        let logo_cap = size * logo_cap_frac;
        let logo_w = logo_cap * (116.34 / 97.5);

        let x0 = cx - (wm.total_w + logo_w + 6.0) * 0.5;
        let cap_top = y; // the em-box top for the letters
        let baseline_off = size * 0.90; // Role::Display ascent
        let logo_y = cap_top + (baseline_off - logo_cap) * 0.5;

        // Per-letter reveal: opacity 0 → 1, y +18 → 0, staggered .04
        let mut pen = x0 + logo_w + 6.0;
        for (i, ch) in wm.text.chars().enumerate() {
            let li = i as f64;
            let local = ((reveal * 2.2) - li * 0.045).clamp(0.0, 1.0);
            let adv = wm.letter_w[i];
            if local <= 0.0 {
                pen += adv;
                continue;
            }
            let e2 = crate::anim::ease_out_cubic(local);
            let dy = (1.0 - e2) * 18.0;
            // `draw_text` positions from the em-box top, so hand it the top.
            p.text_a(
                &ch.to_string(),
                Role::Display,
                size,
                pen,
                cap_top + dy as f32,
                t.ink_strong,
                e2 as f32,
            );
            pen += adv;
        }
        // The logo mark is letter 0 of the site's wordmark.
        {
            let local = (reveal * 2.2).clamp(0.0, 1.0);
            let e2 = crate::anim::ease_out_cubic(local);
            let dy = (1.0 - e2) * 18.0;
            let a = t.ink_strong.with_alpha(e2 as f32);
            p.logo_mark(x0, logo_y + dy as f32, logo_w, a);
        }

        y += size * 1.16 + 14.0;

        // subcopy
        let sub = "ArtCraft builds open source tools for artists. We pride ourselves on freedom of control and expression.";
        p.tx.draw_paragraph(
            p.cv,
            sub,
            Role::Body,
            16.0,
            0.0,
            cx - inner * 0.5,
            y,
            inner * 0.5,
            25.0,
            t.muted,
            reveal as f32,
        );
        y += 34.0;
    }

    // --- capability strip (static, not moving) ---
    p.tx.draw_text(p.cv, "Seedance 2.5  ·  Nano Banana 2  ·  3D Mesh  ·  Background Removal  ·  Character Posing  ·  Mixed Assets", Role::MonoMedium, 10.0, 0.0, left + 24.0, y, t.muted, 1.0);
    y += 22.0;

    // ---- sticky section eyebrow -----------------------------------------
    p.section_eyebrow(y, "01", "Crafting apps", Some("Seven apps · Open source · Pure Rust"));
    y += 37.0;

    // ---- the app grid ----------------------------------------------------
    let cols = if inner > 1100.0 { 4 } else if inner > 760.0 { 3 } else if inner > 380.0 { 2 } else { 1 };
    let n = crate::apps::APPS.len();
    let rows = (n + cols - 1) / cols;
    let gap = 1.0;
    let cw = (inner - gap * (cols as f32 - 1.0)) / cols as f32;
    // Content flow: 36px header + a 16:10 shot + 30px of icon bleed + 30px
    // wordmark + up to 3 lines of pitch + 26px of badges + 62px action row.
    let ch = 232.0 + cw * 0.625;

    let mut rule = crate::anim::RuleDraw::default();
    let mut ticks = [crate::anim::TickDraw::default(); 2];
    rule.trigger(e.now);
    ticks[0].trigger(e.now);
    ticks[1].trigger(e.now);

    for (i, a) in crate::apps::APPS.iter().enumerate() {
        let col = i % cols;
        let row = i / cols;
        let x = left + col as f32 * (cw + gap);
        let cy = y + row as f32 * (ch + gap);
        let hovered = Rect::new(x, cy, cw, ch).contains(input.mouse.0, input.mouse.1);
        // `[data-reveal-group]`: the site staggers by 0.18 * (childLeft -
        // groupLeft)/groupWidth, so the grid washes in left→right. Building
        // the delay directly (rather than through `Reveal`) keeps each card
        // independent of what is on screen.
        let horiz = if inner > 0.0 {
            ((x - left) / inner).clamp(0.0, 1.0) as f64 * 0.18
        } else {
            0.0
        };
        let d = 0.04 * row as f64 + horiz;
        let local = ((e.now - d) / 0.9).clamp(0.0, 1.0);
        let alpha = crate::anim::ease_out_cubic(local) as f32;
        if alpha <= 0.0 {
            continue;
        }
        draw_app_card(p, l, a, i, x, cy, cw, ch, hovered, input, alpha);
    }

    let bottom = y + rows as f32 * (ch + gap);
    p.cv.hline(left, bottom, right - left, t.line);
    (rule, ticks, Action::None)
}

/// The capability ticker: two identical tracks, `36s linear infinite`, the
/// duplicate `aria-hidden`.
fn draw_ticker(p: &mut Painter, e: &Env, y: f32, left: f32, right: f32) -> f32 {
    let t = p.t;
    let items = [
        "Seedance 2.5",
        "Nano Banana 2",
        "Image to Location",
        "3D Compositing",
        "Character Posing",
        "Image to 3D Mesh",
        "Background Removal",
        "2D Compositing",
        "Mixed Assets",
    ];
    p.cv.hline(left, y + 0.5, right - left, t.line);
    let h = 30.0;
    let off = (e.marquee.offset % 1.0) as f32;
    let w = right - left;
    // measure one track
    let mut tw = 0.0;
    for it in items {
        tw += p.hud_w(it) + 48.0;
    }
    if tw <= 0.0 {
        return y + h;
    }
    let base = y;
    let mut xx = left - off * tw;
    let mut guard = 0;
    while xx < right && guard < 40 {
        if xx + tw >= left {
            let mut px = xx;
            for it in items {
                if px > right {
                    break;
                }
                let tw2 = p.hud_w(it);
                let dot = p.hud_w("▪");
                p.tx.draw_text(
                    p.cv,
                    "▪",
                    Role::MonoMedium,
                    11.0,
                    11.0 * 0.15,
                    px,
                    base + 10.0,
                    t.accent_ink,
                    1.0,
                );
                p.hud(it, px + dot + 12.0, base + 10.0, t.muted);
                px += tw2 + 48.0;
            }
        }
        xx += tw;
        guard += 1;
    }
    p.cv.hline(left, y + h - 0.5, w, t.line);
    y + h
}

// ------------------------------------------------------------- app detail

fn draw_app(
    p: &mut Painter,
    l: &mut Launcher,
    input: &Input,
    e: &Env,
    body_y: f32,
) -> (crate::anim::RuleDraw, [crate::anim::TickDraw; 2], Action) {
    let t = p.t;
    let (left, right) = p.rail();
    let a = l.app();
    let idx = l.selected;
    let acc = a.accent;
    let dark = p.dark;
    let inner = right - left;
    let pad = 24.0;
    let mut y = body_y + 28.0 - (e.scroll * 0.3).min(20.0) as f32;

    let mut rule = crate::anim::RuleDraw::default();
    let mut ticks = [crate::anim::TickDraw::default(); 2];
    rule.trigger(e.now);
    ticks[0].trigger(e.now);
    ticks[1].trigger(e.now);

    p.section_eyebrow(y, "02", "Manage", None);
    y += 37.0;

    // ---- header: icon, headline, category, status -----------------------
    let isz = 84.0;
    if let Some((bytes, sw, sh)) = icon(a.slug) {
        p.cv.image(&bytes, sw as usize, sh as usize, left + pad, y, isz, isz, 1.0);
        p.cv.fill_rect(left + pad + 3.0, y + isz, isz - 6.0, 4.0, Rgba::rgba(0.0, 0.0, 0.0, 0.16));
    }
    let tx = left + pad + isz + 22.0;

    // The whole headline wrapped, with the middle word in the app's accent
    // colour. Drawing character-by-character keeps the accent range and the
    // wrapping from drifting apart, which per-part wrapping does not.
    let (h0, h1, h2) = a.headline;
    let hs = 34.0;
    let full = format!("{h0}{h1}{h2}");
    let acc_start = h0.chars().count();
    let acc_len = h1.chars().count();
    let avail = (right - pad - tx).max(80.0);
    let head_lines = p.tx.wrap(&full, Role::Display, hs, 0.0, avail);
    let line_h = hs * 1.08;
    let mut hy = y + 6.0;
    let mut base = 0usize;
    for line in &head_lines {
        let mut hx = tx;
        for (i, ch) in line.chars().enumerate() {
            let g = base + i;
            let col = if acc_len > 0 && g >= acc_start && g < acc_start + acc_len {
                acc.ink(dark)
            } else {
                t.ink_strong
            };
            let s = ch.to_string();
            let cw = p.tx.measure(&s, Role::Display, hs, 0.0);
            p.tx.draw_text(p.cv, &s, Role::Display, hs, 0.0, hx, hy, col, 1.0);
            hx += cw;
        }
        base += line.chars().count() + 1;
        hy += line_h;
    }
    let head_h = hy - y;

    p.hud(&a.category.to_ascii_uppercase(), tx, y + head_h + 10.0, t.muted);
    p.tx.draw_paragraph(
        p.cv,
        a.lede,
        Role::Body,
        15.0,
        0.0,
        tx,
        y + head_h + 34.0,
        (right - pad - tx).max(120.0),
        23.0,
        t.muted,
        1.0,
    );

    // status badge, top-right
    let up = a.status.to_ascii_uppercase();
    let bw = p.tx.measure(&up, Role::MonoMedium, 10.0, 10.0 * 0.15) + 16.0;
    p.cv.fill_rect(right - pad - bw, y + 6.0, bw, 20.0, t.bg_sunken);
    p.cv.stroke_rect(right - pad - bw, y + 6.0, bw, 20.0, t.line, 1.0);
    p.tx.draw_text(
        p.cv,
        &up,
        Role::MonoMedium,
        10.0,
        10.0 * 0.15,
        right - pad - bw + 8.0,
        y + 11.0,
        acc.ink(dark),
        1.0,
    );

    y += isz.max(head_h + 92.0) + 26.0;
    p.cv.hline(left, y, inner, t.line);
    y += 1.0;

    // ---- the two-column body ---------------------------------------------
    let colw = (inner - pad) * 0.5 - pad * 0.5;
    let mut ly = y;
    let mut ry = y;
    let r = l.runtime[idx].clone();

    // ===== left column: versions =====
    section_label(
        p,
        "Versions",
        r.active.as_deref().unwrap_or("none"),
        left + pad,
        &mut ly,

    );
    ly += 8.0;

    let active = r.active.clone();
    p.tx.draw_text(
        p.cv,
        active.as_deref().unwrap_or("—"),
        Role::Display,
        40.0,
        0.0,
        left + pad,
        ly,
        if active.is_some() { t.ink_strong } else { t.faint },
        1.0,
    );
    ly += 52.0;
    if let Some(lat) = &r.latest {
        p.hud(
            &format!("Latest on GitHub: {}", lat.version()),
            left + pad,
            ly,
            t.muted,
        );
        ly += 24.0;
        let notes = lat.notes(4);
        let n_notes = notes.len();
        for n in notes {
            let wl = p.tx.wrap(&n, Role::Body, 12.0, 0.0, colw - 24.0);
            for ln in wl.iter().take(2) {
                p.tx.draw_text(p.cv, ln, Role::Body, 12.0, 0.0, left + pad + 14.0, ly, t.muted, 1.0);
                ly += 17.0;
            }
        }
        if n_notes == 0 {
            ly += 4.0;
        }
    }
    ly += 8.0;

    let mut bx = left + pad;
    if r.active.is_none() {
        let (rect, _) = p.button(
            "detail-install",
            &format!("Install {}", a.pinned),
            BtnKind::Primary,
            BtnSize::Md,
            bx,
            ly,
            input,
            Some(acc),
            false,
        );
        if input.click.map(|c| rect.contains(c.0, c.1)).unwrap_or(false) {
            return (rule, ticks, Action::Install(a.slug.to_string(), a.pinned.to_string()));
        }
        bx += rect.w + 8.0;
    } else {
        if let Some(v) = r.offered_version() {
            let (rect, _) = p.button(
                "detail-update",
                &format!("Update to {}", v),
                BtnKind::Primary,
                BtnSize::Md,
                bx,
                ly,
                input,
                Some(acc),
                false,
            );
            if input.click.map(|c| rect.contains(c.0, c.1)).unwrap_or(false) {
                return (rule, ticks, Action::Install(a.slug.to_string(), v));
            }
            bx += rect.w + 8.0;
        }
        let ver = active.clone().unwrap_or_else(|| a.pinned.to_string());
        let (rect, _) = p.button(
            "detail-launch",
            "Launch",
            BtnKind::Secondary,
            BtnSize::Md,
            bx,
            ly,
            input,
            Some(acc),
            false,
        );
        if input.click.map(|c| rect.contains(c.0, c.1)).unwrap_or(false) {
            return (rule, ticks, Action::Launch(a.slug.to_string(), ver));
        }
        bx += rect.w + 8.0;
    }
    if r.installed.len() > 1 && r.previous.is_some() {
        let (rect, _) = p.button(
            "detail-rollback",
            "Roll back",
            BtnKind::Action,
            BtnSize::Md,
            bx,
            ly,
            input,
            Some(acc),
            false,
        );
        if input.click.map(|c| rect.contains(c.0, c.1)).unwrap_or(false) {
            return (rule, ticks, Action::Rollback(a.slug.to_string()));
        }
    }
    ly += 44.0;

    if !r.installed.is_empty() {
        p.hud("Installed", left + pad, ly, t.faint);
        ly += 20.0;
        for v in &r.installed {
            let cell = Rect::new(left + pad, ly, colw - 24.0, 28.0);
            let hov = cell.contains(input.mouse.0, input.mouse.1);
            if hov {
                p.cv.fill_rect(cell.x, cell.y, cell.w, cell.h, t.bg_sunken);
            }
            let cur = active.as_deref() == Some(v.as_str());
            let marker = if cur { "▪" } else { "·" };
            p.tx.draw_text(
                p.cv,
                marker,
                Role::MonoMedium,
                11.0,
                0.0,
                cell.x + 8.0,
                cell.y + 8.0,
                if cur { acc.ink(dark) } else { t.faint },
                1.0,
            );
            p.tx.draw_text(
                p.cv,
                v,
                Role::Mono,
                12.0,
                0.0,
                cell.x + 26.0,
                cell.y + 8.0,
                if cur { t.ink } else { t.muted },
                1.0,
            );
            if hov && input.click.map(|c| cell.contains(c.0, c.1)).unwrap_or(false) {
                l.runtime[idx].active = Some(v.clone());
                l.runtime[idx].previous = active.clone();
                l.save();
                l.toast(
                    format!("Switched to {} {}", a.name, v),
                    ToastKind::Good,
                    e.now,
                );
            }
            ly += 28.0;
        }
    }
    ly += 12.0;

    // ===== right column: data, backups, integrations =====
    let rx = left + pad + colw + pad;
    section_label(p, "App data", &bytes_human(r.data_bytes), rx, &mut ry);
    ry += 8.0;
    let dd = crate::paths::app_data_dir(a.slug);
    p.tx.draw_text(p.cv, &dd.to_string_lossy(), Role::Mono, 11.0, 0.0, rx, ry, t.muted, 1.0);
    ry += 22.0;
    let mut bx2 = rx;
    for (label, act) in [
        ("Back up", "backup"),
        ("Restore", "restore"),
        ("Wipe", "wipe"),
        ("Reveal", "reveal"),
    ] {
        let (rect, hov) =
            p.button("data-btn", label, BtnKind::Action, BtnSize::Sm, bx2, ry, input, None, false);
        if input.click.map(|c| rect.contains(c.0, c.1)).unwrap_or(false) {
            let slug = a.slug.to_string();
            return (
                rule,
                ticks,
                match act {
                    "backup" => Action::Backup(slug),
                    "restore" => Action::Restore(slug),
                    "wipe" => Action::Wipe(slug),
                    _ => Action::OpenData(slug),
                },
            );
        }
        let _ = hov;
        bx2 += rect.w + 8.0;
    }
    ry += 36.0;
    p.hud(&format!("{} backup(s) on disk", r.backups), rx, ry, t.faint);
    ry += 26.0;

    section_label(
        p,
        "System integration",
        if r.integrated { "registered" } else { "not registered" },
        rx,
        &mut ry,

    );
    ry += 8.0;
    let fully_installed = r.active.is_some() && r.installed.contains(&r.active.as_ref().unwrap().clone());
    let (rect, _) = p.button(
        "integrate",
        if !fully_installed { "Install first" } else if r.integrated { "Re-register" } else { "Register" },
        BtnKind::Action,
        BtnSize::Md,
        rx,
        ry,
        input,
        None,
        false,
    );
    if fully_installed && input.click.map(|c| rect.contains(c.0, c.1)).unwrap_or(false) {
        return (rule, ticks, Action::Integrate(a.slug.to_string()));
    }
    ry += rect.h + 10.0;
    for line in integration_summary(a.slug) {
        p.tx.draw_text(p.cv, &line, Role::Body, 12.0, 0.0, rx, ry, t.muted, 1.0);
        ry += 18.0;
    }
    ry += 8.0;

    section_label(
        p,
        "Launch parameters",
        &format!("{} arg(s), {} var(s)", r.params.args.len(), r.params.env.len()),
        rx,
        &mut ry,

    );
    ry += 8.0;
    if r.params.args.is_empty() && r.params.env.is_empty() && r.params.open_files.is_empty() {
        p.tx.draw_text(
            p.cv,
            "No custom arguments or environment.",
            Role::Body,
            12.0,
            0.0,
            rx,
            ry,
            t.faint,
            1.0,
        );
        ry += 18.0;
    } else {
        for x in &r.params.args {
            p.tx.draw_text(p.cv, &format!("arg  {}", x), Role::Mono, 11.0, 0.0, rx, ry, t.muted, 1.0);
            ry += 17.0;
        }
        for (k, v) in &r.params.env {
            p.tx.draw_text(p.cv, &format!("{}={}", k, v), Role::Mono, 11.0, 0.0, rx, ry, t.muted, 1.0);
            ry += 17.0;
        }
        for f in &r.params.open_files {
            p.tx.draw_text(p.cv, &format!("open {}", f), Role::Mono, 11.0, 0.0, rx, ry, t.muted, 1.0);
            ry += 17.0;
        }
    }
    ry += 4.0;

    let (rect, _) =
        p.button("params", "Add argument…", BtnKind::Ghost, BtnSize::Sm, rx, ry, input, None, false);
    if input.click.map(|c| rect.contains(c.0, c.1)).unwrap_or(false) {
        l.toast(
            "Pass extra args on the CLI:  artcraft-launcher open <app> -- --flag",
            ToastKind::Info,
            e.now,
        );
    }
    ry += rect.h + 6.0;

    let toggle = Rect::new(rx, ry, 34.0, 18.0);
    let hov = toggle.contains(input.mouse.0, input.mouse.1);
    p.cv.stroke_rect(toggle.x, toggle.y, toggle.w, toggle.h, if hov { t.line_strong } else { t.line }, 1.0);
    if r.prompt {
        p.cv.fill_rect(toggle.x + 4.0, toggle.y + 4.0, toggle.w - 8.0, toggle.h - 8.0, acc.app);
    }
    p.hud("Ask me about updates", toggle.x + 44.0, toggle.y + 4.0, t.muted);
    if hov && input.click.map(|c| toggle.contains(c.0, c.1)).unwrap_or(false) {
        l.runtime[idx].prompt = !l.runtime[idx].prompt;
        l.save();
    }

    let bottom = ly.max(ry) + 40.0;
    p.cv.hline(left, bottom, inner, t.line);
    (rule, ticks, Action::None)
}

// ------------------------------------------------------------- app card

fn draw_app_card(
    p: &mut Painter,
    l: &mut Launcher,
    a: &CraftApp,
    idx: usize,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    hovered: bool,
    input: &Input,
    alpha: f32,
) -> Action {
    let t = p.t;
    let acc = a.accent;
    let _dark = p.dark;
    // bg-bg
    p.cv.fill_rect(x, y, w, h, t.bg.with_alpha(t.bg.a * alpha));
    // the hover hairline sweep: h-0.5 origin-left scale-x-0 bg-(--app)
    if hovered {
        p.cv.hline(x + 0.5, y + 0.5, w - 1.0, acc.app.with_alpha(alpha));
    }

    // --- header row: index tab, then the category (drawn below) ----------
    let hh = 36.0;
    p.cv.hline(x, y + hh - 0.5, w, t.line.with_alpha(alpha));
    p.cv.fill_rect(x, y, 30.0, hh, acc.app.with_alpha(alpha));
    p.tx.draw_text(
        p.cv,
        &format!("{:02}", idx + 1),
        Role::MonoBold,
        11.0,
        11.0 * 0.15,
        x + 8.0,
        y + 12.0,
        Rgba::hex(0x000000).with_alpha(alpha),
        1.0,
    );

    // --- category, right-aligned in the header row -----------------------
    let cat = a.category.to_ascii_uppercase();
    p.tx.draw_text_right(
        p.cv,
        &cat,
        Role::MonoMedium,
        11.0,
        11.0 * 0.15,
        x + w - 16.0,
        y + 12.0,
        t.faint.with_alpha(alpha),
        1.0,
    );

    // --- hero shot: aspect-[16/10] overflow-hidden bg-bg-sunken ----------
    let shot_h = w * 10.0 / 16.0;
    let sy = y + hh;
    p.cv.fill_rect(x, sy, w, shot_h, t.bg_sunken.with_alpha(alpha));
    if let Some((bytes, sw, sh)) = screenshot(a.slug) {
        let scale = if hovered { 1.03 } else { 1.0 };
        let dw = w * scale;
        let dh = shot_h * scale;
        p.cv.image(
            &bytes,
            sw as usize,
            sh as usize,
            x + (w - dw) * 0.5,
            sy + (shot_h - dh) * 0.5,
            dw,
            dh,
            alpha,
        );
    }
    // three .tick corners, opacity-60
    let tc = t.line_strong.with_alpha(alpha * 0.6);
    p.cv.tick(x + 8.0, sy + 8.0, tc, false);
    p.cv.tick(x + 8.0, sy + shot_h - 19.0, tc, false);
    p.cv.tick(x + w - 19.0, sy + shot_h - 19.0, tc, false);

    // --- icon straddling the shot's bottom edge: -mt-9 h-18 w-18 --------
    let mut cy = sy + shot_h;
    let isz = 60.0;
    let ix = x + 16.0;
    let iy = cy - isz * 0.45;
    if let Some((bytes, sw, sh)) = icon(a.slug) {
        p.cv.image(&bytes, sw as usize, sh as usize, ix, iy, isz, isz, alpha);
        // drop-shadow-[0_6px_14px_rgba(0,0,0,0.28)]
        p.cv.fill_rect(ix + 2.0, iy + isz, isz - 4.0, 3.0, Rgba::rgba(0.0, 0.0, 0.0, 0.16 * alpha));
    }
    cy = iy + isz + 6.0;

    // --- wordmark ---------------------------------------------------------
    p.tx.draw_text(
        p.cv,
        a.name,
        Role::Display,
        22.0,
        0.0,
        x + 16.0,
        cy,
        t.ink_strong.with_alpha(alpha),
        1.0,
    );

    // --- pitch ------------------------------------------------------------
    let mut py = cy + 34.0;
    for ln in p.tx.wrap(a.pitch, Role::Body, 13.0, 0.0, w - 32.0).iter().take(2) {
        p.tx.draw_text(p.cv, ln, Role::Body, 13.0, 0.0, x + 16.0, py, t.muted.with_alpha(alpha), 1.0);
        py += 19.0;
    }

    // --- badges -----------------------------------------------------------
    let mut bx = x + 16.0;
    let by = py + 6.0;
    for label in [a.status, "Installers ready"] {
        let up = label.to_ascii_uppercase();
        let tw = p.tx.measure(&up, Role::MonoMedium, 10.0, 10.0 * 0.15);
        let bw = tw + 16.0;
        p.cv.fill_rect(bx, by, bw, 20.0, t.bg_sunken.with_alpha(alpha));
        p.cv.stroke_rect(bx, by, bw, 20.0, t.line.with_alpha(alpha), 1.0);
        p.tx.draw_text(
            p.cv,
            &up,
            Role::MonoMedium,
            10.0,
            10.0 * 0.15,
            bx + 8.0,
            by + 5.0,
            acc.ink(p.dark).with_alpha(alpha),
            1.0,
        );
        bx += bw + 6.0;
    }

    // --- version + action row ---------------------------------------------
    let ay = y + h - 62.0;
    p.cv.hline(x, ay - 0.5, w, t.line.with_alpha(alpha));
    let ver_label = match (&l.runtime[idx].active, &l.runtime[idx].latest) {
        (Some(cur), Some(lat)) => {
            if crate::github::cmp_ver(&lat.version(), cur) == std::cmp::Ordering::Greater {
                format!("{cur} → {}", lat.version())
            } else {
                cur.clone()
            }
        }
        (Some(cur), None) => cur.clone(),
        (None, Some(lat)) => format!("not installed · {}", lat.version()),
        (None, None) => format!("not installed · {}", a.pinned),
    };
    p.hud_a(&ver_label, x + 16.0, ay + 10.0, t.muted.with_alpha(alpha), alpha);

    // Right-align the buttons: measure first, then draw at real positions.
    let mut kinds: Vec<(BtnKind, BtnSize, String)> = Vec::new();
    if let Some(new) = l.runtime[idx].offered_version() {
        kinds.push((BtnKind::Primary, BtnSize::Sm, format!("Update to {new}")));
    }
    if l.runtime[idx].active.is_some() {
        kinds.push((BtnKind::Secondary, BtnSize::Sm, "Launch".to_string()));
    } else {
        kinds.push((BtnKind::Action, BtnSize::Sm, "Install".to_string()));
    }
    let widths: Vec<f32> = kinds.iter().map(|(k, sz, lbl)| p.button_metrics(*k, *sz, lbl).0).collect();
    let gap = 8.0;
    let total: f32 = widths.iter().sum::<f32>() + gap * (widths.len().saturating_sub(1)) as f32;
    let mut bx2 = x + w - 16.0 - total;

    if let Some(new) = l.runtime[idx].offered_version() {
        let (rect, _) = p.button(
            "card-update",
            &format!("Update to {new}"),
            BtnKind::Primary,
            BtnSize::Sm,
            bx2,
            ay + 8.0,
            input,
            Some(acc),
            false,
        );
        if input.click.map(|c| rect.contains(c.0, c.1)).unwrap_or(false) {
            return Action::Install(a.slug.to_string(), new);
        }
        bx2 += rect.w + gap;
    }

    if l.runtime[idx].active.is_some() {
        let ver = l.runtime[idx].active.clone().unwrap_or_default();
        let (rect, _) = p.button(
            "card-launch",
            "Launch",
            BtnKind::Secondary,
            BtnSize::Sm,
            bx2,
            ay + 8.0,
            input,
            Some(acc),
            false,
        );
        if input.click.map(|c| rect.contains(c.0, c.1)).unwrap_or(false) {
            return Action::Launch(a.slug.to_string(), ver);
        }
    } else {
        let v = l.runtime[idx]
            .latest
            .as_ref()
            .map(|x| x.version())
            .unwrap_or_else(|| a.pinned.to_string());
        let (rect, _) = p.button(
            "card-install",
            "Install",
            BtnKind::Action,
            BtnSize::Sm,
            bx2,
            ay + 8.0,
            input,
            Some(acc),
            false,
        );
        if input.click.map(|c| rect.contains(c.0, c.1)).unwrap_or(false) {
            return Action::Install(a.slug.to_string(), v);
        }
    }

    // "Explore <Name>" affordance at the card's bottom-left
    let afford = format!("Explore {}", a.name);
    p.tx.draw_text(
        p.cv,
        &afford.to_ascii_uppercase(),
        Role::MonoMedium,
        11.0,
        11.0 * 0.15,
        x + 16.0,
        y + h - 18.0,
        (if hovered { t.ink } else { t.muted }).with_alpha(alpha),
        1.0,
    );

    if input
        .click
        .map(|c| Rect::new(x, y, w, ay).contains(c.0, c.1))
        .unwrap_or(false)
    {
        l.selected = idx;
        l.screen = Screen::App;
        l.scroll = 0.0;
        return Action::Select(idx);
    }
    Action::None
}
// ---------------------------------------------------------------- assets

use std::sync::OnceLock;

static ICONS: OnceLock<Vec<Option<(Vec<u8>, u32, u32)>>> = OnceLock::new();
static SHOTS: OnceLock<Vec<Option<(Vec<u8>, u32, u32)>>> = OnceLock::new();

fn icon_bytes(slug: &str) -> &'static [u8] {
    match slug {
        "photocraft" => include_bytes!("../assets/images/photocraft-icon.png"),
        "vectorcraft" => include_bytes!("../assets/images/vectorcraft-icon.png"),
        "filmcraft" => include_bytes!("../assets/images/filmcraft-icon.png"),
        "lightcraft" => include_bytes!("../assets/images/lightcraft-icon.png"),
        "pdfcraft" => include_bytes!("../assets/images/pdfcraft-icon.png"),
        "effectcraft" => include_bytes!("../assets/images/effectcraft-icon.png"),
        "designcraft" => include_bytes!("../assets/images/designcraft-icon.png"),
        _ => include_bytes!("../assets/images/pdfcraft-icon.png"),
    }
}

fn shot_bytes(slug: &str) -> &'static [u8] {
    match slug {
        "photocraft" => include_bytes!("../assets/images/photocraft-hero.png"),
        "vectorcraft" => include_bytes!("../assets/images/vectorcraft-hero.png"),
        "filmcraft" => include_bytes!("../assets/images/filmcraft-hero.png"),
        "lightcraft" => include_bytes!("../assets/images/lightcraft-hero.png"),
        "pdfcraft" => include_bytes!("../assets/images/pdfcraft-hero.png"),
        "effectcraft" => include_bytes!("../assets/images/effectcraft-hero.png"),
        "designcraft" => include_bytes!("../assets/images/designcraft-hero.png"),
        _ => include_bytes!("../assets/images/pdfcraft-hero.png"),
    }
}

fn decode_png(bytes: &'static [u8]) -> Option<(Vec<u8>, u32, u32)> {
    let mut d = png::Decoder::new(std::io::Cursor::new(bytes))
        .read_info()
        .ok()?;
    let mut buf = vec![0u8; d.output_buffer_size()];
    let info = d.next_frame(&mut buf).ok()?;
    let b = &buf[..info.buffer_size()];
    let rgba = match info.color_type {
        png::ColorType::Rgba => b.to_vec(),
        png::ColorType::Rgb => {
            let mut o = Vec::with_capacity(b.len() / 3 * 4);
            for p in b.chunks_exact(3) {
                o.extend_from_slice(&[p[0], p[1], p[2], 255]);
            }
            o
        }
        _ => return None,
    };
    Some((rgba, info.width, info.height))
}

pub fn icon(slug: &str) -> Option<(Vec<u8>, u32, u32)> {
    let v = ICONS.get_or_init(|| crate::apps::APPS.iter().map(|a| decode_png(icon_bytes(a.slug))).collect());
    let i = crate::apps::APPS.iter().position(|a| a.slug == slug)?;
    v[i].clone()
}

pub fn screenshot(slug: &str) -> Option<(Vec<u8>, u32, u32)> {
    let v = SHOTS.get_or_init(|| crate::apps::APPS.iter().map(|a| decode_png(shot_bytes(a.slug))).collect());
    let i = crate::apps::APPS.iter().position(|a| a.slug == slug)?;
    v[i].clone()
}

fn bytes_human(b: u64) -> String {
    const U: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < 4 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{b} B")
    } else {
        format!("{v:.1} {}", U[i])
    }
}

fn section_label(p: &mut Painter, label: &str, value: &str, x: f32, y: &mut f32) {
    let t = p.t;
    p.hud(label, x, *y, t.muted);
    let w = p.hud_w(label);
    p.hud(value, x + w + 14.0, *y, t.faint);
    *y += 22.0;
    p.cv.hline(x, *y, 120.0, t.line);
    *y += 2.0;
}

/// Human-readable summary of what registering an app will do on this OS.
pub fn integration_summary(slug: &str) -> Vec<String> {
    let a = match crate::apps::app(slug) {
        Some(a) => a,
        None => return Vec::new(),
    };
    match crate::paths::host_platform() {
        Platform::Linux => vec![
            format!("~/.local/share/applications/{slug}.desktop"),
            format!("Icons in ~/.local/share/icons/hicolor (ai.storyteller.{slug})"),
            format!(
                "MIME associations for {} …",
                a.mime.first().unwrap_or(&"")
            ),
            "Right-click: “Open with “, “Open in a new window”, “Send to”".to_string(),
            "KDE service menu + file-manager action".to_string(),
        ],
        Platform::Windows => vec![
            format!("%LOCALAPPDATA%\\ArtCraft\\icons\\{slug}.ico  (reserved)"),
            format!("Start Menu shortcut: {}.lnk", a.name),
            format!("App Paths registry entry for {slug}.exe"),
            format!("Open-with associations for {}", a.open_exts.join(", ")),
            "Explorer right-click via ProgID".to_string(),
        ],
        Platform::Macos => vec![
            format!("~/Applications/ArtCraft/{}.app", a.bundle),
            format!("Reserved icon: ~/Library/Application Support/ArtCraft Studio/icons/{slug}.icns"),
            "Launch Services registration (lsregister)".to_string(),
            format!("Document types for {}", a.mime.first().unwrap_or(&"")),
            "Dock + “Open with” entries".to_string(),
        ],
        Platform::Freebsd => vec![
            format!("~/.local/share/applications/{slug}.desktop"),
            "Icons + MIME + right-click actions".to_string(),
        ],
    }
}

// ---------------------------------------------------------------- updates

fn draw_updates(
    p: &mut Painter,
    l: &mut Launcher,
    input: &Input,
    e: &Env,
    body_y: f32,
) -> (crate::anim::RuleDraw, [crate::anim::TickDraw; 2], Action) {
    let t = p.t;
    let (left, right) = p.rail();
    let inner = right - left;
    let pad = 24.0;
    let mut y = body_y + 28.0 - (e.scroll * 0.3).min(20.0) as f32;

    let mut rule = crate::anim::RuleDraw::default();
    let mut ticks = [crate::anim::TickDraw::default(); 2];
    rule.trigger(e.now);
    ticks[0].trigger(e.now);
    ticks[1].trigger(e.now);

    p.section_eyebrow(y, "03", "Updates", Some("You always decide"));
    y += 37.0;

    let pending = l.pending_updates();
    let h = if pending.is_empty() {
        "Everything is current."
    } else {
        "Updates are ready."
    };
    let hs = 44.0;
    p.tx.draw_text(p.cv, h, Role::Display, hs, 0.0, left + pad, y, t.ink_strong, 1.0);
    y += hs + 14.0;
    p.tx.draw_text(
        p.cv,
        "The launcher never updates an app behind your back. Pick one, or decline and it will not ask again for that version.",
        Role::Body,
        15.0,
        0.0,
        left + pad,
        y,
        t.muted,
        1.0,
    );
    y += 40.0;
    p.cv.hline(left, y, inner, t.line);
    y += 1.0;

    if pending.is_empty() {
        p.tx.draw_text(
            p.cv,
            "No app has a newer release on GitHub right now.",
            Role::Body,
            14.0,
            0.0,
            left + pad,
            y + 28.0,
            t.faint,
            1.0,
        );
        y += 70.0;
    }

    for &idx in &pending {
        let a = &crate::apps::APPS[idx];
        let r = l.runtime[idx].clone();
        let cur = r.active.clone().unwrap_or_else(|| "not installed".to_string());
        let rh = 96.0;
        let cell = Rect::new(left, y, inner, rh);
        let hov = cell.contains(input.mouse.0, input.mouse.1);
        if hov {
            p.cv.fill_rect(cell.x, cell.y, cell.w, cell.h, t.bg_sunken);
        }
        let acc = a.accent;
        p.cv.fill_rect(cell.x, cell.y, 3.0, cell.h, acc.app);
        p.tx.draw_text(
            p.cv,
            a.name,
            Role::Display,
            22.0,
            0.0,
            cell.x + 20.0,
            cell.y + 14.0,
            t.ink_strong,
            1.0,
        );
        p.tx.draw_text(
            p.cv,
            &format!("{cur} → {}", r.latest.clone().map(|x| x.version()).unwrap_or_default()),
            Role::MonoMedium,
            12.0,
            0.0,
            cell.x + 20.0,
            cell.y + 46.0,
            t.muted,
            1.0,
        );
        let notes = r.latest.as_ref().map(|x| x.notes(2)).unwrap_or_default();
        if let Some(n) = notes.first() {
            p.tx.draw_text(p.cv, n, Role::Body, 12.0, 0.0, cell.x + 240.0, cell.y + 24.0, t.faint, 1.0);
        }

        // Not now / Don't ask / Update
        let mut bx = cell.right() - 24.0;
        let (lw, _) = p.button_metrics(BtnKind::Ghost, BtnSize::Sm, "Not now");
        bx -= lw;
        let later = Rect::new(bx, cell.y + 30.0, lw, 30.0);
        let lh = later.contains(input.mouse.0, input.mouse.1);
        p.tx.draw_text(
            p.cv,
            "NOT NOW",
            Role::MonoBold,
            11.0,
            11.0 * 0.12,
            later.x,
            later.y + 9.0,
            if lh { t.ink } else { t.faint },
            1.0,
        );
        if input.click.map(|c| later.contains(c.0, c.1)).unwrap_or(false) {
            l.toast(
                format!("{} left at {cur}", a.name),
                ToastKind::Info,
                e.now,
            );
        }
        bx -= 8.0;

        let (dw, _) = p.button_metrics(BtnKind::Action, BtnSize::Sm, "Don't ask");
        bx -= dw;
        let skip = Rect::new(bx, cell.y + 30.0, dw, 30.0);
        let sh = skip.contains(input.mouse.0, input.mouse.1);
        p.cv.fill_rect(skip.x, skip.y, skip.w, skip.h, t.bg_raised);
        p.cv.stroke_rect(
            skip.x,
            skip.y,
            skip.w,
            skip.h,
            if sh { t.line_strong } else { t.line },
            1.0,
        );
        p.tx.draw_text(
            p.cv,
            "DON'T ASK",
            Role::MonoBold,
            11.0,
            11.0 * 0.12,
            skip.x,
            skip.y + 9.0,
            t.muted,
            1.0,
        );
        if input.click.map(|c| skip.contains(c.0, c.1)).unwrap_or(false) {
            let ver = r.latest.clone().map(|x| x.version()).unwrap_or_default();
            l.runtime[idx].muted.push(ver.clone());
            l.save();
            l.toast(format!("{} {} muted", a.name, ver), ToastKind::Good, e.now);
        }
        bx -= 8.0;

        let (uw, _) = p.button_metrics(BtnKind::Primary, BtnSize::Sm, "Update");
        bx -= uw;
        let go = Rect::new(bx, cell.y + 30.0, uw, 30.0);
        p.cv.fill_rect(go.x, go.y, go.w, go.h, t.invert_bg);
        p.tx.draw_text(
            p.cv,
            "UPDATE",
            Role::MonoBold,
            11.0,
            11.0 * 0.12,
            go.x,
            go.y + 9.0,
            t.invert_fg,
            1.0,
        );
        if input.click.map(|c| go.contains(c.0, c.1)).unwrap_or(false) {
            let ver = r.latest.clone().map(|x| x.version()).unwrap_or_default();
            return (rule, ticks, Action::Install(a.slug.to_string(), ver));
        }

        p.cv.hline(cell.x, cell.bottom() - 0.5, inner, t.line);
        y += rh;
    }

    p.cv.hline(left, y + 24.0, inner, t.line);
    (rule, ticks, Action::None)
}

// --------------------------------------------------------------- settings

fn draw_settings(
    p: &mut Painter,
    l: &mut Launcher,
    input: &Input,
    e: &Env,
    body_y: f32,
) -> (crate::anim::RuleDraw, [crate::anim::TickDraw; 2], Action) {
    let t = p.t;
    let (left, right) = p.rail();
    let inner = right - left;
    let pad = 24.0;
    let colw = (inner - pad * 2.0) * 0.5;
    let mut y = body_y + 28.0 - (e.scroll * 0.3).min(20.0) as f32;

    let mut rule = crate::anim::RuleDraw::default();
    let mut ticks = [crate::anim::TickDraw::default(); 2];
    rule.trigger(e.now);
    ticks[0].trigger(e.now);
    ticks[1].trigger(e.now);

    p.section_eyebrow(y, "04", "Settings", Some("Preferences & integrations"));
    y += 37.0;

    p.tx.draw_text(p.cv, "Settings", Role::Display, 44.0, 0.0, left + pad, y, t.ink_strong, 1.0);
    y += 62.0;
    p.cv.hline(left, y, inner, t.line);
    y += 1.0;

    let lx = left + pad;
    let rx = left + pad + colw + pad;
    let mut ly = y;
    let mut ry = y;

    // ---- left: appearance + telemetry ------------------------------------
    p.hud("Appearance", lx, ly, t.muted);
    ly += 26.0;
    p.hud("Theme", lx, ly, t.muted);
    let themes = [
        crate::theme::Theme::Light,
        crate::theme::Theme::Dark,
        crate::theme::Theme::System,
    ];
    let mut tx = lx + 90.0;
    for th in themes {
        let label = th.label();
        let tw = p.tx.measure(label, Role::MonoMedium, 11.0, 11.0 * 0.15);
        let cell = Rect::new(tx, ly - 4.0, tw + 20.0, 24.0);
        let hov = cell.contains(input.mouse.0, input.mouse.1);
        let on = l.settings.theme == th;
        if on || hov {
            p.cv.fill_rect(cell.x, cell.y, cell.w, cell.h, t.invert_bg);
        }
        p.hud_a(
            label,
            cell.x + 10.0,
            ly,
            if on || hov { t.invert_fg } else { t.muted },
            1.0,
        );
        if hov && input.click.map(|c| cell.contains(c.0, c.1)).unwrap_or(false) {
            l.settings.theme = th;
            return (rule, ticks, Action::Theme(th));
        }
        tx += cell.w + 8.0;
    }
    ly += 34.0;
    ly = toggle_row(p, l, input, e, "Animations", l.settings.animations, lx, ly, |s, v| s.animations = v);
    ly = toggle_row(p, l, input, e, "Check for updates on launch", l.settings.check_on_launch, lx, ly, |s, v| s.check_on_launch = v);
    ly += 18.0;

    p.hud("Crash reporting", lx, ly, t.muted);
    ly += 26.0;
    ly = toggle_row(p, l, input, e, "Send reports to the endpoint", l.settings.telemetry.enabled, lx, ly, |s, v| s.telemetry.enabled = v);
    ly = toggle_row(p, l, input, e, "Include device metrics", l.settings.telemetry.metrics, lx, ly, |s, v| s.telemetry.metrics = v);
    ly = toggle_row(p, l, input, e, "Include the app's log tail", l.settings.telemetry.log_tail, lx, ly, |s, v| s.telemetry.log_tail = v);
    ly += 8.0;

    p.hud("Endpoint", lx, ly, t.faint);
    ly += 20.0;
    let ep = l.settings.telemetry.endpoint.clone();
    p.tx.draw_text(
        p.cv,
        if ep.is_empty() {
            "spool only — nothing leaves this machine"
        } else {
            &ep
        },
        Role::Mono,
        12.0,
        0.0,
        lx,
        ly,
        if ep.is_empty() { t.faint } else { t.muted },
        1.0,
    );
    ly += 22.0;
    let (rect, _) = p.button("set-ep", "Set endpoint…", BtnKind::Action, BtnSize::Sm, lx, ly, input, None, false);
    if input.click.map(|c| rect.contains(c.0, c.1)).unwrap_or(false) {
        l.toast(
            "Set it with:  export ARTCRAFT_TELEMETRY_URL=https://…",
            ToastKind::Info,
            e.now,
        );
    }
    ly += rect.h + 10.0;
    let spooled = crate::telemetry::Spool::new().len();
    p.hud(&format!("{spooled} report(s) spooled locally"), lx, ly, t.faint);
    ly += 22.0;
    let (rect, _) = p.button("clear-spool", "Clear spool", BtnKind::Ghost, BtnSize::Sm, lx, ly, input, None, false);
    if input.click.map(|c| rect.contains(c.0, c.1)).unwrap_or(false) {
        let s = crate::telemetry::Spool::new();
        for (p_, _) in s.take(500) {
            s.remove(&p_);
        }
        l.reports.clear();
        l.toast("Spool cleared", ToastKind::Good, e.now);
    }
    ly += rect.h + 20.0;

    // ---- right: integrations ---------------------------------------------
    p.hud("System integration", rx, ry, t.muted);
    ry += 26.0;
    let keys: [(&'static str, &str); 6] = [
        ("auto_register", "Register automatically on first run"),
        ("linux_desktop_entries", "Desktop entries"),
        ("linux_context_actions", "File-manager right-click"),
        ("windows_icons", "Reserve icons (.ico)"),
        ("windows_start_menu", "Start Menu shortcuts"),
        ("windows_associations", "File associations"),
    ];
    for (key, label) in keys {
        let on = match key {
            "auto_register" => l.settings.integrations.auto_register,
            "linux_desktop_entries" => l.settings.integrations.linux_desktop_entries,
            "linux_context_actions" => l.settings.integrations.linux_context_actions,
            "windows_icons" => l.settings.integrations.windows_icons,
            "windows_start_menu" => l.settings.integrations.windows_start_menu,
            "windows_associations" => l.settings.integrations.windows_associations,
            _ => false,
        };
        let relevant = match key {
            "auto_register" => true,
            "linux_desktop_entries" | "linux_context_actions" => {
                crate::paths::host_platform() == Platform::Linux
            }
            "windows_icons" | "windows_start_menu" | "windows_associations" => {
                crate::paths::host_platform() == Platform::Windows
            }
            _ => true,
        };
        if !relevant {
            continue;
        }
        ry = toggle_row(p, l, input, e, label, on, rx, ry, move |s, v| match key {
            "auto_register" => s.integrations.auto_register = v,
            "linux_desktop_entries" => s.integrations.linux_desktop_entries = v,
            "linux_context_actions" => s.integrations.linux_context_actions = v,
            "windows_icons" => s.integrations.windows_icons = v,
            "windows_start_menu" => s.integrations.windows_start_menu = v,
            "windows_associations" => s.integrations.windows_associations = v,
            _ => {}
        });
    }
    ry += 6.0;
    p.tx.draw_paragraph(
        p.cv,
        "With this on, the first run writes the launcher's own entry and icon and registers every app already installed; each new install registers itself.",
        Role::Body,
        12.0,
        0.0,
        rx,
        ry,
        colw - 24.0,
        18.0,
        t.faint,
        1.0,
    );
    ry += 34.0;

    p.hud("Settings file", rx, ry, t.faint);
    ry += 22.0;
    p.tx.draw_paragraph(
        p.cv,
        "Export writes one portable JSON file with the launcher preferences, every app's launch parameters, the data-directory manifest and the backup index.",
        Role::Body,
        12.0,
        0.0,
        rx,
        ry,
        colw - 24.0,
        18.0,
        t.muted,
        1.0,
    );
    ry += 40.0;
    let (rect, _) = p.button("export", "Export…", BtnKind::Action, BtnSize::Md, rx, ry, input, None, false);
    if input.click.map(|c| rect.contains(c.0, c.1)).unwrap_or(false) {
        return (rule, ticks, Action::Export);
    }
    let (rect2, _) = p.button("import", "Import…", BtnKind::Ghost, BtnSize::Md, rx + rect.w + 8.0, ry, input, None, false);
    if input.click.map(|c| rect2.contains(c.0, c.1)).unwrap_or(false) {
        return (rule, ticks, Action::Import);
    }
    ry += rect.h + 10.0;

    // ---- locations --------------------------------------------------------
    let mut by = ly.max(ry) + 20.0;
    p.cv.hline(left, by, inner, t.line);
    by += 18.0;
    p.hud("Locations", left + pad, by, t.muted);
    by += 24.0;
    let rows = [
        ("Launcher home", crate::paths::launcher_root()),
        ("Installs", crate::paths::install_root()),
        ("Data", crate::paths::data_home()),
        ("Config", crate::paths::config_home()),
        ("Cache", crate::paths::cache_home()),
        ("Backups", crate::paths::backups_root()),
        ("Spool", crate::paths::spool_dir()),
    ];
    for (k, v) in rows {
        p.tx.draw_text(p.cv, k, Role::MonoMedium, 11.0, 0.0, left + pad, by, t.faint, 1.0);
        p.tx.draw_text(
            p.cv,
            &v.to_string_lossy(),
            Role::Mono,
            11.0,
            0.0,
            left + pad + 130.0,
            by,
            t.muted,
            1.0,
        );
        by += 19.0;
    }
    by += 8.0;
    p.hud(
        &format!(
            "{} · {} · v{}",
            std::env::consts::OS,
            std::env::consts::ARCH,
            env!("CARGO_PKG_VERSION")
        ),
        left + pad,
        by,
        t.faint,
    );
    (rule, ticks, Action::None)
}

fn toggle_row(
    p: &mut Painter,
    l: &mut Launcher,
    input: &Input,
    _e: &Env,
    label: &str,
    on: bool,
    x: f32,
    y: f32,
    set: impl FnOnce(&mut Settings, bool),
) -> f32 {
    let t = p.t;
    let tg = Rect::new(x, y, 34.0, 18.0);
    let hov = tg.contains(input.mouse.0, input.mouse.1);
    p.cv.stroke_rect(
        tg.x,
        tg.y,
        tg.w,
        tg.h,
        if hov { t.line_strong } else { t.line },
        1.0,
    );
    if on {
        p.cv.fill_rect(tg.x + 4.0, tg.y + 4.0, tg.w - 8.0, tg.h - 8.0, t.accent);
    }
    p.tx.draw_text(p.cv, label, Role::Body, 13.0, 0.0, x + 44.0, y, t.muted, 1.0);
    if hov && input.click.map(|c| tg.contains(c.0, c.1)).unwrap_or(false) {
        set(&mut l.settings, !on);
        let _ = l.settings.save();
    }
    y + 30.0
}

// ------------------------------------------------------------------ reports

fn draw_reports(
    p: &mut Painter,
    _l: &Launcher,
    input: &Input,
    e: &Env,
    body_y: f32,
) -> (crate::anim::RuleDraw, [crate::anim::TickDraw; 2], Action) {
    let t = p.t;
    let (left, right) = p.rail();
    let inner = right - left;
    let pad = 24.0;
    let mut y = body_y + 28.0 - (e.scroll * 0.3).min(20.0) as f32;

    let mut rule = crate::anim::RuleDraw::default();
    let mut ticks = [crate::anim::TickDraw::default(); 2];
    rule.trigger(e.now);
    ticks[0].trigger(e.now);
    ticks[1].trigger(e.now);

    p.section_eyebrow(y, "05", "Crash reports", Some("Local spool · Never lost"));
    y += 37.0;

    p.tx.draw_text(p.cv, "Crashes", Role::Display, 44.0, 0.0, left + pad, y, t.ink_strong, 1.0);
    y += 60.0;
    let sub = "Every crash and failed launch is captured with its exit status, the app's own log tail, the launcher's recent actions as pre-crash state, inferred causes, and device resource metrics at the moment it happened.";
    p.tx.draw_paragraph(p.cv, sub, Role::Body, 14.0, 0.0, left + pad, y, inner - pad * 2.0, 22.0, t.muted, 1.0);
    y += 54.0;

    let spool = crate::telemetry::Spool::new();
    let reports = spool.take(60);
    p.cv.hline(left, y, inner, t.line);
    y += 1.0;

    if reports.is_empty() {
        p.tx.draw_text(
            p.cv,
            "Nothing spooled. If an app crashes, the report lands here first — and only leaves if you set an endpoint.",
            Role::Body,
            14.0,
            0.0,
            left + pad,
            y + 28.0,
            t.faint,
            1.0,
        );
        y += 70.0;
    }

    for (path, r) in &reports {
        let rh = 112.0;
        let cell = Rect::new(left, y, inner, rh);
        let hov = cell.contains(input.mouse.0, input.mouse.1);
        if hov {
            p.cv.fill_rect(cell.x, cell.y, cell.w, cell.h, t.bg_sunken);
        }
        if let Some(acc) = crate::apps::app(&r.app).map(|a| a.accent) {
            p.cv.fill_rect(cell.x, cell.y, 3.0, cell.h, acc.app);
        } else {
            p.cv.fill_rect(cell.x, cell.y, 3.0, cell.h, t.faint);
        }
        let name = crate::apps::app(&r.app).map(|a| a.name).unwrap_or(r.app.as_str());
        p.tx.draw_text(
            p.cv,
            &format!("{} {}", name, r.version),
            Role::Display,
            20.0,
            0.0,
            cell.x + 20.0,
            cell.y + 12.0,
            t.ink_strong,
            1.0,
        );
        let kind = match r.kind {
            crate::telemetry::ReportKind::Crash => "CRASH",
            crate::telemetry::ReportKind::Error => "ERROR",
            crate::telemetry::ReportKind::LaunchFailed => "LAUNCH FAILED",
            crate::telemetry::ReportKind::Oom => "OUT OF MEMORY",
            crate::telemetry::ReportKind::UpdateFailed => "UPDATE FAILED",
        };
        let kw = p.tx.measure(kind, Role::MonoBold, 10.0, 10.0 * 0.15);
        p.cv.fill_rect(cell.x + 20.0, cell.y + 40.0, kw + 16.0, 18.0, t.bg_sunken);
        p.tx.draw_text(p.cv, kind, Role::MonoBold, 10.0, 10.0 * 0.15, cell.x + 28.0, cell.y + 45.0, t.danger, 1.0);
        let msg = if r.message.is_empty() { "no message" } else { r.message.as_str() };
        p.tx.draw_text(p.cv, msg, Role::Body, 13.0, 0.0, cell.x + 20.0, cell.y + 64.0, t.muted, 1.0);
        if let Some(m) = &r.metrics {
            let met = format!(
                "cpu {:.0}% · mem {:.0}% · free {} · {} cores{}",
                m.cpu_load.unwrap_or(0.0) * 100.0,
                m.mem_used_frac().unwrap_or(0.0) * 100.0,
                m.disk_free.map(bytes_human).unwrap_or_else(|| "—".to_string()),
                m.cores,
                if m.degraded { " · degraded" } else { "" }
            );
            p.tx.draw_text(p.cv, &met, Role::Mono, 11.0, 0.0, cell.x + 20.0, cell.y + 86.0, t.faint, 1.0);
        }
        let mut cx = cell.x + 420.0;
        for c in r.causes.iter().take(2) {
            let label = format!("{} {:.0}%", c.kind, c.confidence * 100.0);
            let lw = p.tx.measure(&label, Role::MonoMedium, 10.0, 10.0 * 0.15);
            p.cv.fill_rect(cx, cell.y + 12.0, lw + 14.0, 18.0, t.bg_sunken);
            p.cv.stroke_rect(cx, cell.y + 12.0, lw + 14.0, 18.0, t.line, 1.0);
            p.tx.draw_text(
                p.cv,
                &label,
                Role::MonoMedium,
                10.0,
                10.0 * 0.15,
                cx + 7.0,
                cell.y + 17.0,
                t.accent_ink,
                1.0,
            );
            cx += lw + 22.0;
        }
        if let Some(c) = r.causes.first() {
            p.tx.draw_paragraph(
                p.cv,
                &c.detail,
                Role::Body,
                12.0,
                0.0,
                cell.x + 420.0,
                cell.y + 40.0,
                inner - 460.0,
                17.0,
                t.muted,
                1.0,
            );
        }
        if !r.pre_crash.is_empty() {
            p.tx.draw_text(
                p.cv,
                &format!("pre-crash: {}", r.pre_crash.join(" · ")),
                Role::Mono,
                11.0,
                0.0,
                cell.x + 20.0,
                cell.y + 90.0,
                t.faint,
                1.0,
            );
        }
        let del = Rect::new(cell.right() - 90.0, cell.y + cell.h - 34.0, 66.0, 24.0);
        let dh = del.contains(input.mouse.0, input.mouse.1);
        p.cv.fill_rect(del.x, del.y, del.w, del.h, if dh { t.invert_bg } else { t.bg });
        p.cv.stroke_rect(del.x, del.y, del.w, del.h, if dh { t.invert_bg } else { t.line }, 1.0);
        p.tx.draw_text(
            p.cv,
            "DELETE",
            Role::MonoBold,
            10.0,
            10.0 * 0.12,
            del.x + 10.0,
            del.y + 6.0,
            if dh { t.invert_fg } else { t.muted },
            1.0,
        );
        if input.click.map(|c| del.contains(c.0, c.1)).unwrap_or(false) {
            spool.remove(path);
        }
        p.cv.hline(cell.x, cell.bottom() - 0.5, inner, t.line);
        y += rh;
    }

    p.cv.hline(left, y + 24.0, inner, t.line);
    (rule, ticks, Action::None)
}

// ------------------------------------------------------------ update prompt

/// The "an update is ready" prompt. It can always be declined.
fn draw_prompt(
    p: &mut Painter,
    l: &Launcher,
    input: &Input,
    e: &Env,
    slug: &str,
    version: &str,
) -> Action {
    let t = p.t;
    let w = p.w();
    let h = p.h();
    let a = crate::apps::app(slug);
    let name = a.map(|x| x.name).unwrap_or(slug);
    let acc = a.map(|x| x.accent).unwrap_or_else(|| crate::theme::ACCENTS[0]);

    // scrim: bg-black/70
    p.cv.fill_rect(0.0, 0.0, w, h, Rgba::rgba(0.0, 0.0, 0.0, 0.7));

    let cw = 460.0;
    let ch = 268.0;
    let x = (w - cw) * 0.5;
    let y = (h - ch) * 0.5;
    p.cv.fill_rect(x, y, cw, ch, t.bg);
    p.cv.stroke_rect(x, y, cw, ch, t.line, 1.0);
    let c = t.line_strong;
    p.cv.tick(x + 8.0, y + 8.0, c, false);
    p.cv.tick(x + 8.0, y + ch - 19.0, c, false);
    p.cv.tick(x + cw - 19.0, y + 8.0, c, false);
    p.cv.tick(x + cw - 19.0, y + ch - 19.0, c, false);

    p.cv.fill_rect(x, y, cw, 3.0, acc.app);

    let pad = 28.0;
    p.hud("Update available", x + pad, y + 26.0, t.muted);
    let headline = format!("{} {}", name, version);
    p.tx.draw_paragraph(
        p.cv,
        &headline,
        Role::Display,
        30.0,
        0.0,
        x + pad,
        y + 52.0,
        cw - pad * 2.0,
        34.0,
        t.ink_strong,
        1.0,
    );
    let body = format!(
        "A new version of {} is on GitHub. You can install it now, or leave it — the launcher will not touch an app without you saying so.",
        name
    );
    p.tx.draw_paragraph(
        p.cv,
        &body,
        Role::Body,
        14.0,
        0.0,
        x + pad,
        y + 126.0,
        cw - pad * 2.0,
        21.0,
        t.muted,
        1.0,
    );

    let bw = 108.0;
    let bh = 40.0;
    let by = y + ch - 72.0;
    let mut bx = x + cw - pad - bw;

    let upd = Rect::new(bx, by, bw, bh);
    p.cv.fill_rect(upd.x, upd.y, upd.w, upd.h, t.invert_bg);
    p.tx.draw_text(p.cv, "UPDATE", Role::MonoBold, 11.0, 11.0 * 0.12, upd.x + 16.0, upd.y + 14.0, t.invert_fg, 1.0);
    if input.click.map(|c| upd.contains(c.0, c.1)).unwrap_or(false) {
        return Action::Install(slug.to_string(), version.to_string());
    }
    bx -= bw + 8.0;

    let skip = Rect::new(bx, by, bw, bh);
    let sh = skip.contains(input.mouse.0, input.mouse.1);
    p.cv.fill_rect(skip.x, skip.y, skip.w, skip.h, t.bg_raised);
    p.cv.stroke_rect(skip.x, skip.y, skip.w, skip.h, if sh { t.line_strong } else { t.line }, 1.0);
    p.tx.draw_text(p.cv, "DON'T ASK", Role::MonoBold, 11.0, 11.0 * 0.12, skip.x + 14.0, skip.y + 14.0, t.muted, 1.0);
    if input.click.map(|c| skip.contains(c.0, c.1)).unwrap_or(false) {
        let _ = l;
        return Action::DeclinePrompt;
    }
    bx -= 100.0;

    let later = Rect::new(bx, by, 96.0, bh);
    let lh = later.contains(input.mouse.0, input.mouse.1);
    p.tx.draw_text(
        p.cv,
        "NOT NOW",
        Role::MonoBold,
        11.0,
        11.0 * 0.12,
        later.x,
        later.y + 14.0,
        if lh { t.ink } else { t.faint },
        1.0,
    );
    if input.click.map(|c| later.contains(c.0, c.1)).unwrap_or(false) {
        return Action::DeclinePrompt;
    }
    let _ = e;
    Action::None
}

// ------------------------------------------------------------------ toasts

fn draw_toasts(p: &mut Painter, l: &Launcher, now: f64) {
    let t = p.t;
    let w = p.w();
    let mut y = p.h() - 60.0;
    for toast in l.toasts.iter().rev() {
        let age = now - toast.born;
        let life = 4.2;
        let a = if age > life {
            ((life + 0.4 - age) / 0.4).clamp(0.0, 1.0) as f32
        } else {
            1.0
        };
        if a <= 0.0 {
            continue;
        }
        let tw = p.tx.measure(&toast.text, Role::Body, 13.0, 0.0);
        let bw = tw + 32.0;
        let bh = 38.0;
        let x = w - bw - 20.0;
        let col = match toast.kind {
            ToastKind::Info => t.accent,
            ToastKind::Good => crate::theme::PLAN_BASIC,
            ToastKind::Warn => crate::theme::PLAN_MAX,
            ToastKind::Bad => t.danger,
        };
        p.cv.fill_rect(x, y, bw, bh, t.bg_raised.with_alpha(t.bg_raised.a * a));
        p.cv.stroke_rect(x, y, bw, bh, t.line.with_alpha(t.line.a * a), 1.0);
        p.cv.fill_rect(x, y, 3.0, bh, col.with_alpha(a));
        p.tx.draw_text(
            p.cv,
            &toast.text,
            Role::Body,
            13.0,
            0.0,
            x + 16.0,
            y + 11.0,
            t.ink.with_alpha(t.ink.a * a),
            1.0,
        );
        y -= bh + 8.0;
    }
}

// ------------------------------------------------------------------- ruler

/// The scroll-ruler instrument: frost rail, tick rows, accent needle, and the
/// odometer percentage.
fn draw_ruler(p: &mut Painter, e: &Env, h: f32) {
    let t = p.t;
    let rail_w = 15.0f32;
    let x = p.w() - rail_w;
    let frac = if e.scroll_max > 0.0 {
        (e.scroll / e.scroll_max).clamp(0.0, 1.0)
    } else {
        0.0
    };

    // frost underlay, masked to fade at both ends
    let top_a = t.bg.a * 0.55 * (frac * 3.0).clamp(0.0, 1.0);
    let bot_a = t.bg.a * 0.55 * ((1.0 - frac) * 3.0).clamp(0.0, 1.0);
    let mid_a = (t.bg.a * 0.55).min(top_a.max(bot_a));
    p.cv.fill_rect(x, 0.0, rail_w, h * 0.5, t.bg.with_alpha(top_a * 0.5 + mid_a * 0.5));
    p.cv.fill_rect(x, h * 0.5, rail_w, h * 0.5, t.bg.with_alpha(bot_a * 0.5 + mid_a * 0.5));

    // tick rows: major every 10%, minor every 2%
    for i in 0..=50 {
        let f = i as f32 / 50.0;
        let yy = f * h;
        let major = i % 5 == 0;
        let len = if major { 9.0 } else { 4.0 };
        let c = if major { t.ink } else { t.line_strong };
        p.cv.hline(x + rail_w - len, yy, len, c.with_alpha(if major { 0.55 } else { 0.35 }));
    }
    for i in 0..=10 {
        let f = i as f32 / 10.0;
        let yy = f * h;
        let label = format!("{}", i * 10);
        let tw = p.tx.measure(&label, Role::Mono, 9.0, 9.0 * 0.08);
        p.tx.draw_text(
            p.cv,
            &label,
            Role::Mono,
            9.0,
            9.0 * 0.08,
            x + rail_w - 13.0 - tw,
            yy - 5.0,
            t.ink.with_alpha(0.5),
            1.0,
        );
    }

    // accent needle with a glow
    let ny = frac * h;
    for i in 1..=3u32 {
        p.cv.fill_rect(x, ny - i as f32, rail_w, i as f32 * 2.0, t.accent.with_alpha(0.06 * (4 - i) as f32));
    }
    p.cv.fill_rect(x, ny - 1.0, rail_w, 1.0, t.accent.with_alpha(0.35));
    p.cv.fill_rect(x, ny, rail_w, 1.0, t.accent);
    p.cv.fill_rect(x, ny + 1.0, rail_w, 1.0, t.accent.with_alpha(0.35));

    // odometer readout tucked under the needle
    let od = format!("{:03}%", (frac * 100.0).round() as i32);
    let ow = p.tx.measure(&od, Role::Mono, 9.0, 9.0 * 0.08);
    let ox = x - ow - 2.0;
    p.cv.fill_rect(ox - 4.0, ny - 4.0, ow + 8.0, 12.0, t.bg.with_alpha(t.bg.a * 0.8));
    p.tx.draw_text(p.cv, &od, Role::Mono, 9.0, 9.0 * 0.08, ox, ny - 1.0, t.accent_ink, 1.0);
}
