//! Design tokens — a verbatim port of `globals.css` from getartcraft.com.
//!
//! Light `:root`:
//! ```css
//! --bg:#f2f1ee; --bg-raised:#faf9f7; --bg-sunken:#e9e8e4;
//! --ink:#101014; --ink-strong:#000;
//! --muted:rgba(16,16,20,.62);  --faint:rgba(16,16,20,.42);
//! --line:rgba(16,16,20,.16);   --line-strong:rgba(16,16,20,.4);
//! --accent:#2d81ff; --accent-ink:#1659c4;
//! --invert-bg:#101014; --invert-fg:#f2f1ee; --danger:#b3261e;
//! ```
//! Dark `:root[data-theme=dark]` swaps bg/ink, softens lines to 15%/40%,
//! and lifts `--accent-ink` to `#74aaff`.
//!
//! Every token cross-fades over 0.4s on theme switch, exactly as the site's
//! `html.theme-anim` does with registered `@property` colour transitions.

use serde::{Deserialize, Serialize};

pub const THEME_FADE: f32 = 0.4;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

impl Theme {
    pub fn label(self) -> &'static str {
        match self {
            Theme::Light => "Light",
            Theme::Dark => "Dark",
            Theme::System => "System",
        }
    }

    pub fn next(self) -> Theme {
        match self {
            Theme::Light => Theme::Dark,
            Theme::Dark => Theme::System,
            Theme::System => Theme::Light,
        }
    }
}

/// RGBA in 0..1, non-premultiplied, sRGB. `a == 0` means "no alpha".
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgba {
    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }
    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// `#rrggbb`
    pub const fn hex(v: u32) -> Self {
        Self::rgb(
            ((v >> 16) & 0xff) as f32 / 255.0,
            ((v >> 8) & 0xff) as f32 / 255.0,
            (v & 0xff) as f32 / 255.0,
        )
    }

    /// `#rrggbbaa` (the site's 8-digit tokens).
    pub const fn hexa(v: u32) -> Self {
        Self::rgba(
            ((v >> 24) & 0xff) as f32 / 255.0,
            ((v >> 16) & 0xff) as f32 / 255.0,
            ((v >> 8) & 0xff) as f32 / 255.0,
            (v & 0xff) as f32 / 255.0,
        )
    }

    /// `color-mix(in srgb, self <pct>, other)` — the site's alpha-tint idiom.
    pub fn mix(self, other: Rgba, t: f32) -> Rgba {
        let t = t.clamp(0.0, 1.0);
        Rgba {
            r: self.r + (other.r - self.r) * t,
            g: self.g + (other.g - self.g) * t,
            b: self.b + (other.b - self.b) * t,
            a: self.a + (other.a - self.a) * t,
        }
    }

    pub fn with_alpha(self, a: f32) -> Rgba {
        Rgba { a, ..self }
    }

    pub fn lerp(self, other: Rgba, t: f32) -> Rgba {
        self.mix(other, t)
    }

    pub fn to_bytes(self) -> [u8; 4] {
        [
            (self.r.clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
            (self.g.clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
            (self.b.clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
            (self.a.clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
        ]
    }

    pub fn luminance(self) -> f32 {
        0.2126 * self.r + 0.7152 * self.g + 0.0722 * self.b
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Tokens {
    pub bg: Rgba,
    pub bg_raised: Rgba,
    pub bg_sunken: Rgba,
    pub ink: Rgba,
    pub ink_strong: Rgba,
    pub muted: Rgba,
    pub faint: Rgba,
    pub line: Rgba,
    pub line_strong: Rgba,
    pub accent: Rgba,
    pub accent_ink: Rgba,
    pub invert_bg: Rgba,
    pub invert_fg: Rgba,
    pub danger: Rgba,
    pub scanline: Rgba,
}

impl Tokens {
    pub const LIGHT: Tokens = Tokens {
        bg: Rgba::rgb(0.95, 0.95, 0.93),
        bg_raised: Rgba::hex(0xfaf9f7),
        bg_sunken: Rgba::hex(0xe9e8e4),
        ink: Rgba::rgb(0.95, 0.95, 0.93),
        ink_strong: Rgba::hex(0x000000),
        muted: Rgba::hexa(0x101014_9e),
        faint: Rgba::hexa(0x101014_6b),
        line: Rgba::hexa(0x101014_29),
        line_strong: Rgba::hexa(0x101014_66),
        accent: Rgba::hex(0x2d81ff),
        accent_ink: Rgba::hex(0x1659c4),
        invert_bg: Rgba::hex(0x101014),
        invert_fg: Rgba::rgb(0.95, 0.95, 0.93),
        danger: Rgba::hex(0xb3261e),
        scanline: Rgba::hexa(0x101014_08),
    };

    pub const DARK: Tokens = Tokens {
        bg: Rgba::rgb(0.01, 0.01, 0.01),
        bg_raised: Rgba::hex(0x101014),
        bg_sunken: Rgba::hex(0x060607),
        ink: Rgba::rgb(0.95, 0.95, 0.93),
        ink_strong: Rgba::hex(0xffffff),
        muted: Rgba::hexa(0xf2f1ee_99),
        faint: Rgba::hexa(0xf2f1ee_61),
        line: Rgba::hexa(0xf2f1ee_26),
        line_strong: Rgba::hexa(0xf2f1ee_66),
        accent: Rgba::hex(0x2d81ff),
        accent_ink: Rgba::hex(0x74aaff),
        invert_bg: Rgba::hex(0xffffff),
        invert_fg: Rgba::hex(0x0b0b0c),
        danger: Rgba::hex(0xff6259),
        scanline: Rgba::hexa(0xffffff_06),
    };

    pub fn for_theme(t: Theme, prefers_dark: bool) -> Tokens {
        match t {
            Theme::Light => Tokens::LIGHT,
            Theme::Dark => Tokens::DARK,
            Theme::System => {
                if prefers_dark {
                    Tokens::DARK
                } else {
                    Tokens::LIGHT
                }
            }
        }
    }

    /// The site's 0.4s `@property` theme cross-fade, token by token.
    pub fn lerp(self, other: Tokens, t: f32) -> Tokens {
        let l = |a: Rgba, b: Rgba| a.lerp(b, t);
        Tokens {
            bg: l(self.bg, other.bg),
            bg_raised: l(self.bg_raised, other.bg_raised),
            bg_sunken: l(self.bg_sunken, other.bg_sunken),
            ink: l(self.ink, other.ink),
            ink_strong: l(self.ink_strong, other.ink_strong),
            muted: l(self.muted, other.muted),
            faint: l(self.faint, other.faint),
            line: l(self.line, other.line),
            line_strong: l(self.line_strong, other.line_strong),
            accent: l(self.accent, other.accent),
            accent_ink: l(self.accent_ink, other.accent_ink),
            invert_bg: l(self.invert_bg, other.invert_bg),
            invert_fg: l(self.invert_fg, other.invert_fg),
            danger: l(self.danger, other.danger),
            scanline: l(self.scanline, other.scanline),
        }
    }

    pub fn is_dark(&self) -> bool {
        self.bg.luminance() < 0.5
    }
}

/// Per-app accent, from `.app-<slug>` in `globals.css`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct AppAccent {
    pub app: Rgba,
    pub app_ink_light: Rgba,
    pub app_ink_dark: Rgba,
}

impl AppAccent {
    pub fn ink(self, dark: bool) -> Rgba {
        if dark {
            self.app_ink_dark
        } else {
            self.app_ink_light
        }
    }
}

pub const ACCENTS: [AppAccent; 7] = [
    // .app-photocraft
    AppAccent {
        app: Rgba::hex(0x2f7bf5),
        app_ink_light: Rgba::hex(0x1a5bc4),
        app_ink_dark: Rgba::hex(0x7fb0ff),
    },
    // .app-vectorcraft
    AppAccent {
        app: Rgba::hex(0xe8573f),
        app_ink_light: Rgba::hex(0xb83a24),
        app_ink_dark: Rgba::hex(0xff8a70),
    },
    // .app-filmcraft
    AppAccent {
        app: Rgba::hex(0x8b5cf6),
        app_ink_light: Rgba::hex(0x6a3fd6),
        app_ink_dark: Rgba::hex(0xb69cff),
    },
    // .app-lightcraft
    AppAccent {
        app: Rgba::hex(0xf2a516),
        app_ink_light: Rgba::hex(0x8a5800),
        app_ink_dark: Rgba::hex(0xffc14d),
    },
    // .app-pdfcraft
    AppAccent {
        app: Rgba::hex(0x12a58a),
        app_ink_light: Rgba::hex(0x0a7563),
        app_ink_dark: Rgba::hex(0x3fd6b8),
    },
    // .app-effectcraft
    AppAccent {
        app: Rgba::hex(0xe0368f),
        app_ink_light: Rgba::hex(0xb0206c),
        app_ink_dark: Rgba::hex(0xff7ab8),
    },
    // .app-designcraft
    AppAccent {
        app: Rgba::hex(0x7bb51c),
        app_ink_light: Rgba::hex(0x457008),
        app_ink_dark: Rgba::hex(0xa6e04a),
    },
];

/// Plan colours, from `.plan-<tier>`.
pub const PLAN_BASIC: Rgba = Rgba::hex(0x00a873);
pub const PLAN_PRO: Rgba = Rgba::hex(0x9d4cff);
pub const PLAN_MAX: Rgba = Rgba::hex(0xd97700);
pub const PLAN_ENTERPRISE: Rgba = Rgba::hex(0x3568c9);

/// Type scale — the site's `.font-display{font-stretch:118%;font-weight:620}`.
pub const DISPLAY_WEIGHT: f32 = 620.0;
pub const DISPLAY_STRETCH: f32 = 118.0;
pub const WORDMARK_WEIGHT: f32 = 750.0;
pub const WORDMARK_STRETCH: f32 = 125.0;

/// Geometry from the site's compiled utilities.
pub const NAV_H: f32 = 48.0; // h-12 = 3rem
pub const TICK: f32 = 11.0; // .tick { width:11px; height:11px }
pub const TICK_HALF_OFF: f32 = 5.0;
pub const RAIL_MAX: f32 = 1280.0; // max-w-[1280px]
pub const ZERO_RADIUS: bool = true; // --radius-*: initial — square everywhere
