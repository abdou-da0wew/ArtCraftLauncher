//! Motion — a faithful port of the site's GSAP choreography onto one clock.
//!
//! The original drives everything from `introClock.t` against named tuner
//! thresholds (`copyAt`, `cardsAt`) plus `ScrollTrigger` enter callbacks. All
//! of the numeric values below are lifted from the bundle:
//!
//! | target | from | to | dur | ease | offsets |
//! |---|---|---|---|---|---|
//! | `[data-reveal]` | `autoAlpha:0, y:28, blur(8px)` | `autoAlpha:1, y:0, blur(0)` | .9s | power3.out | stagger .08 |
//! | `[data-draw-rule]` | `scaleX:0` | `scaleX:1` | .7s | power2.out | at 0 |
//! | `[data-draw-eyebrow]` | reveal | reveal | .9s | power3.out | at +.15s |
//! | `[data-draw-tick]` | `scale:0, opacity:0` | `scale:1, opacity:1` | .35s | power3.out | at +.35s, stagger .06 |
//! | hero scrim | opacity 0 | 1 | .9s | linear | from cardsAt |
//!
//! Group children get an extra left-to-right wash:
//! `delay = 0.18 * (childLeft - groupLeft) / groupWidth`.

use std::time::Instant;

/// GSAP `Power.easeOut` for n = 1..=4.
#[inline]
pub fn power_out(n: i32, t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(n)
}

/// `cubic-bezier(.2,.9,.2,1)` — the ruler digit roll.
#[inline]
pub fn cubic_bezier_2_9_2_1(t: f64) -> f64 {
    // Newton-free approximation: sample the curve and linear-interp.
    const N: usize = 64;
    let t = t.clamp(0.0, 1.0);
    let mut prev_x = 0.0;
    let mut prev_y = 0.0;
    for i in 1..=N {
        let s = i as f64 / N as f64;
        let x = bez(s, 0.2, 0.9);
        let y = bez(s, 0.2, 1.0);
        if x >= t {
            let f = if (x - prev_x).abs() < 1e-9 {
                0.0
            } else {
                (t - prev_x) / (x - prev_x)
            };
            return prev_y + (y - prev_y) * f;
        }
        prev_x = x;
        prev_y = y;
    }
    1.0
}

#[inline]
fn bez(s: f64, p1: f64, p2: f64) -> f64 {
    let u = 1.0 - s;
    3.0 * u * u * s * p1 + 3.0 * u * s * s * p2 + s * s * s
}

#[inline]
pub fn ease_out_quad(t: f64) -> f64 {
    power_out(2, t)
}

#[inline]
pub fn ease_out_cubic(t: f64) -> f64 {
    power_out(3, t)
}

#[inline]
pub fn clamp01(t: f64) -> f64 {
    t.clamp(0.0, 1.0)
}

/// The single monotonic clock the whole choreography reads from.
pub struct Clock {
    start: Instant,
    t: f64,
    pub paused: bool,
    pub reduced: bool,
}

impl Clock {
    pub fn new(reduced: bool) -> Self {
        Self {
            start: Instant::now(),
            t: 0.0,
            paused: false,
            reduced,
        }
    }

    pub fn advance(&mut self, dt: f64) {
        if !self.paused {
            self.t += if self.reduced { dt * 0.0 } else { dt };
        }
    }

    pub fn t(&self) -> f64 {
        self.t
    }

    pub fn real_dt(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }
}

/// The hero's intro clock with the site's two tuner thresholds.
pub struct IntroClock {
    pub t: f64,
    pub copy_at: f64,
    pub cards_at: f64,
    pub wordmark_at: f64,
    pub played: bool,
    generation: u64,
}

impl Default for IntroClock {
    fn default() -> Self {
        Self {
            t: 0.0,
            // The site's tuner defaults: the wordmark forms first, then the
            // copy, then the field scales in.
            wordmark_at: 0.00,
            copy_at: 0.34,
            cards_at: 0.62,
            played: false,
            generation: 0,
        }
    }
}

impl IntroClock {
    pub fn restart(&mut self) {
        self.t = 0.0;
        self.played = false;
        self.generation += 1;
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Advance by dt, returning true on the frame the intro completes.
    pub fn advance(&mut self, dt: f64, reduced: bool) -> bool {
        if reduced {
            if !self.played {
                self.played = true;
                self.t = self.cards_at.max(self.copy_at) + 1.0;
                return true;
            }
            return false;
        }
        self.t += dt;
        if !self.played && self.t >= self.cards_at + 1.2 {
            self.played = true;
            return true;
        }
        false
    }

    /// 0..1 — how far through the wordmark formation we are.
    pub fn wordmark_p(&self, reduced: bool) -> f64 {
        if reduced {
            return 1.0;
        }
        clamp01((self.t - self.wordmark_at) / 0.9)
    }

    /// 0..1 — hero copy reveal progress envelope start.
    pub fn copy_p(&self, reduced: bool) -> f64 {
        if reduced {
            return 1.0;
        }
        clamp01((self.t - self.copy_at) / 0.9)
    }

    /// 0..1 — hero scrim opacity, linear over .9s from `cards_at`.
    pub fn scrim_p(&self, reduced: bool) -> f64 {
        if reduced {
            return 0.0;
        }
        clamp01((self.t - self.cards_at) / 0.9)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ease {
    /// GSAP `power3.out`
    Power3Out,
    /// GSAP `power2.out`
    Power2Out,
    Linear,
    /// `cubic-bezier(.2,.9,.2,1)`
    DigitRoll,
}

impl Ease {
    pub fn apply(self, t: f64) -> f64 {
        match self {
            Ease::Power3Out => power_out(3, t),
            Ease::Power2Out => power_out(2, t),
            Ease::Linear => clamp01(t),
            Ease::DigitRoll => cubic_bezier_2_9_2_1(t),
        }
    }
}

/// A `[data-reveal]` element: `y 28 → 0`, `blur 8 → 0`, `alpha 0 → 1`.
#[derive(Clone, Copy, Debug)]
pub struct Reveal {
    pub delay: f64,
    pub dur: f64,
    pub started: Option<f64>,
    pub from_y: f32,
    pub from_blur: f32,
}

impl Reveal {
    pub const DEFAULT_DUR: f64 = 0.9;

    pub fn new() -> Self {
        Self {
            delay: 0.0,
            dur: Self::DEFAULT_DUR,
            started: None,
            from_y: 28.0,
            from_blur: 8.0,
        }
    }

    /// Arm the reveal. `now` is the clock time.
    pub fn trigger(&mut self, now: f64) {
        if self.started.is_none() {
            self.started = Some(now);
        }
    }

    /// Extra left-to-right wash: `0.18 * (childLeft - groupLeft)/groupWidth`.
    pub fn with_horizontal(mut self, child_left: f32, group_left: f32, group_w: f32) -> Self {
        if group_w > 0.0 {
            let d = (((child_left - group_left) / group_w).clamp(0.0, 1.0) as f64) * 0.18;
        self.delay += d;
        }
        self
    }

    pub fn with_delay(mut self, d: f64) -> Self {
        self.delay = d;
        self
    }

    pub fn with_stagger(mut self, i: usize, step: f64) -> Self {
        self.delay += i as f64 * step;
        self
    }

    /// 0..1, eased.
    pub fn p(&self, now: f64) -> f64 {
        match self.started {
            None => 0.0,
            Some(t0) => {
                let local = (now - t0 - self.delay) / self.dur;
                if local <= 0.0 {
                    0.0
                } else if local >= 1.0 {
                    1.0
                } else {
                    Ease::Power3Out.apply(local)
                }
            }
        }
    }

    pub fn done(&self, now: f64) -> bool {
        self.p(now) >= 1.0
    }

    /// True when the element should still be drawn through the blur path.
    pub fn active(&self, now: f64) -> bool {
        self.started.is_some_and(|t0| (now - t0 - self.delay) < self.dur) && self.p(now) < 1.0
    }
}

/// A `[data-draw-rule]` wipe: `scaleX 0 → 1`, .7s power2.out.
#[derive(Clone, Copy, Debug, Default)]
pub struct RuleDraw {
    pub started: Option<f64>,
}

impl RuleDraw {
    pub fn trigger(&mut self, now: f64) {
        if self.started.is_none() {
            self.started = Some(now);
        }
    }
    pub fn p(&self, now: f64) -> f64 {
        match self.started {
            None => 0.0,
            Some(t0) => {
                let l = (now - t0) / 0.7;
                if l <= 0.0 {
                    0.0
                } else if l >= 1.0 {
                    1.0
                } else {
                    Ease::Power2Out.apply(l)
                }
            }
        }
    }
}

/// A `[data-draw-tick]`: `scale 0→1, opacity 0→1`, .35s power3.out, staggered.
#[derive(Clone, Copy, Debug, Default)]
pub struct TickDraw {
    pub started: Option<f64>,
    pub delay: f64,
}

impl TickDraw {
    pub fn trigger(&mut self, now: f64) {
        if self.started.is_none() {
            self.started = Some(now);
        }
    }
    pub fn p(&self, now: f64) -> f64 {
        match self.started {
            None => 0.0,
            Some(t0) => {
                let l = (now - t0 - self.delay) / 0.35;
                if l <= 0.0 {
                    0.0
                } else if l >= 1.0 {
                    1.0
                } else {
                    Ease::Power3Out.apply(l)
                }
            }
        }
    }
}

/// Linear value tween with an ease, used everywhere a raw number animates.
#[derive(Clone, Copy, Debug)]
pub struct Tween {
    pub from: f32,
    pub to: f32,
    pub dur: f64,
    pub delay: f64,
    pub ease: Ease,
    pub started: Option<f64>,
    pub done: bool,
}

impl Tween {
    pub fn new(from: f32, to: f32, dur: f64, ease: Ease) -> Self {
        Self {
            from,
            to,
            dur,
            delay: 0.0,
            ease,
            started: None,
            done: false,
        }
    }
    pub fn delay(mut self, d: f64) -> Self {
        self.delay = d;
        self
    }
    pub fn trigger(&mut self, now: f64) {
        if self.started.is_none() {
            self.started = Some(now);
        }
    }
    pub fn get(&mut self, now: f64) -> f32 {
        let Some(t0) = self.started else {
            return self.from;
        };
        let l = (now - t0 - self.delay) / self.dur;
        if l <= 0.0 {
            self.from
        } else if l >= 1.0 {
            self.done = true;
            self.to
        } else {
            let e = self.ease.apply(l);
            self.from + (self.to - self.from) as f64 as f32 * e as f32
        }
    }
}

/// The capability ticker: `36s linear infinite`.
pub struct Marquee {
    pub dur: f64,
    pub offset: f64,
}

impl Default for Marquee {
    fn default() -> Self {
        Self {
            dur: 36.0,
            offset: 0.0,
        }
    }
}

impl Marquee {
    pub fn tick(&mut self, dt: f64) {
        if self.dur > 0.0 {
            self.offset = (self.offset + dt / self.dur).fract();
        }
    }
}

/// One odometer digit column: `transform .28s cubic-bezier(.2,.9,.2,1)`
/// between the previous and next digit.
pub struct OdomoDigit {
    pub value: u8,
    /// -1 … +1 fractional roll between digits.
    roll: f32,
    roll_from: f32,
    roll_to: f32,
    roll_t: f64,
    roll_started: Option<f64>,
}

impl OdomoDigit {
    pub const fn new() -> Self {
        Self {
            value: 0,
            roll: 0.0,
            roll_from: 0.0,
            roll_to: 0.0,
            roll_t: 0.0,
            roll_started: None,
        }
    }
    pub fn set(&mut self, v: u8, now: f64) {
        let v = v % 10;
        if v == self.value {
            return;
        }
        self.roll_from = self.roll;
        self.roll += if v > self.value {
            (v - self.value) as f32
        } else {
            -((self.value - v) as f32)
        };
        self.roll_to = self.roll;
        self.roll_t = 0.28;
        self.roll_started = Some(now);
        self.value = v;
    }
    /// `translateY` in em units to apply to the digit column.
    pub fn translate(&self, now: f64) -> f32 {
        match self.roll_started {
            None => self.roll,
            Some(t0) => {
                let l = (now - t0) / self.roll_t;
                if l <= 0.0 {
                    self.roll_from
                } else if l >= 1.0 {
                    self.roll_to
                } else {
                    self.roll_from + (self.roll_to - self.roll_from) * Ease::DigitRoll.apply(l) as f32
                }
            }
        }
    }
}

/// Three odometer columns spelling a percentage.
pub struct Odometer {
    pub digits: [OdomoDigit; 3],
}

impl Default for Odometer {
    fn default() -> Self {
        Self {
            digits: [OdomoDigit::new(), OdomoDigit::new(), OdomoDigit::new()],
        }
    }
}

impl Odometer {
    pub fn set(&mut self, pct: f32, now: f64) {
        let v = pct.clamp(0.0, 999.0) as u32;
        self.digits[0].set((v / 100) as u8, now);
        self.digits[1].set(((v / 10) % 10) as u8, now);
        self.digits[2].set((v % 10) as u8, now);
    }
}

/// Frame-time EMA governor, ported from the hero scene's `frameEma`.
/// Drives adaptive quality so the launcher never drops frames on weak GPUs.
pub struct FrameGovernor {
    pub ema: f64,
    pub quality: f64,
    pub target: f64,
}

impl Default for FrameGovernor {
    fn default() -> Self {
        Self {
            ema: 1.0 / 60.0,
            quality: 1.0,
            target: 1.0 / 60.0,
        }
    }
}

impl FrameGovernor {
    pub fn sample(&mut self, dt: f64) {
        let a = 0.1;
        self.ema += (dt - self.ema) * a;
        // quality follows the ratio of target to actual frame time
        let ratio = (self.target / self.ema.max(1e-4)).clamp(0.25, 1.0);
        self.quality += (ratio - self.quality) * 0.05;
        self.quality = self.quality.clamp(0.25, 1.0);
    }
}
