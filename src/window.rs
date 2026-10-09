//! The window, event loop and the frame driver: input → state → draw → GPU.

use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use crate::anim::{Clock, IntroClock, Marquee, Odometer};
use crate::apps::{Arch, Platform, APPS};
use crate::config::{self, Settings};
use crate::gfx::canvas::Canvas;
use crate::gfx::text::TextEngine;
use crate::render::Gpu;
use crate::screens::{Action, Env, Frame};
use crate::state::{Launcher, ToastKind};
use crate::telemetry::{self, DeviceMetrics, Report, ReportKind, Trail, Watched};
use crate::theme::{Theme, Tokens};
use crate::ui::Painter;

pub struct App {
    /// Declared BEFORE `gpu` so the surface is dropped first — see the safety
    /// note on `render::Gpu::new`.
    pub gpu: Option<Gpu>,
    pub window: Option<Window>,
    pub canvas: Canvas,
    pub text: TextEngine,
    pub clock: Clock,
    pub intro: IntroClock,
    pub marquee: Box<Marquee>,
    pub odo: Box<Odometer>,
    pub l: Launcher,
    pub scale: f32,
    pub last: Instant,
    pub trail: Trail,
    pub watched: Vec<Watched>,
    pub spool: telemetry::Spool,
    pub metrics: DeviceMetrics,
    pub job_rx: Option<Receiver<JobMsg>>,
    pub job_tx: Option<std::sync::mpsc::Sender<JobMsg>>,
    pub cursor: (f32, f32),
    pub clicking: Option<(f32, f32)>,
    pub down: bool,
    pub scroll: f32,
    pub scroll_target: f32,
    pub scroll_max: f32,
    pub fps: f64,
    pub hero_started: bool,
    pub pending_check: bool,
    pub last_theme: Theme,
    pub intro_gen: u64,
}

pub enum JobMsg {
    Progress(String, String, u64, u64),
    Done(String, String, Result<(), String>),
    Latest(String, String),
}

impl App {
    pub fn new() -> Self {
        let mut l = Launcher::new();
        let clock = Clock::new(!l.settings.animations);
        let marquee = Box::new(Marquee::default());
        let odo = Box::new(Odometer::default());
        let spool = telemetry::Spool::new();
        let mut reports = Vec::new();
        for (_, r) in spool.take(200) {
            reports.push(r);
        }
        l.reports = reports;
        let metrics = DeviceMetrics::sample();
        Self {
            window: None,
            gpu: None,
            canvas: Canvas::new(1, 1),
            text: TextEngine::new(),
            clock,
            intro: IntroClock::default(),
            marquee,
            odo,
            l,
            scale: 1.0,
            last: Instant::now(),
            trail: Trail::default(),
            watched: Vec::new(),
            spool,
            metrics,
            job_rx: None,
            job_tx: None,
            cursor: (0.0, 0.0),
            clicking: None,
            down: false,
            scroll: 0.0,
            scroll_target: 0.0,
            scroll_max: 1.0,
            fps: 60.0,
            hero_started: false,
            pending_check: l_pending_check_default(),
            last_theme: Theme::System,
            intro_gen: 0,
        }
    }

    fn took_theme(&self) -> Theme {
        self.l.settings.theme
    }

    /// The intro restarts when the theme changes, mirroring `onIntroReplay`.
    fn maybe_restart_intro(&mut self) {
        if self.took_theme() != self.last_theme {
            self.last_theme = self.took_theme();
            self.intro.restart();
            self.intro_gen += 1;
        }
    }

    fn tokens(&self) -> Tokens {
        Tokens::for_theme(self.l.settings.theme, false)
    }

    pub fn check_updates(&mut self) {
        self.pending_check = true;
        let tx = self.job_tx.clone().or_else(|| {
            let (tx, rx) = std::sync::mpsc::channel();
            self.job_tx = Some(tx.clone());
            self.job_rx = Some(rx);
            Some(tx)
        });
        let Some(tx) = tx else { return };
        let slugs: Vec<&'static str> = APPS.iter().map(|a| a.slug).collect();
        std::thread::spawn(move || {
            for slug in slugs {
                let Some(a) = crate::apps::app(slug) else { continue };
                if let Ok(r) = crate::github::latest(a, false) {
                    let _ = tx.send(JobMsg::Latest(slug.to_string(), r.version()));
                }
                std::thread::sleep(std::time::Duration::from_millis(400));
            }
        });
    }
}

fn l_pending_check_default() -> bool {
    false
}

// ------------------------------------------------------------------- input

impl App {
    fn screen_scroll_bounds(&self) -> f32 {
        // Content height estimate; the launcher is short enough that most
        // screens fit, so this stays generous rather than precise.
        900.0
    }
}

// --------------------------------------------------------------- the shell

struct Shell {
    app: App,
}

impl ApplicationHandler for Shell {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.app.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("ArtCraft Studio")
            .with_inner_size(winit::dpi::LogicalSize::new(1180.0f64, 760.0f64))
            .with_min_inner_size(winit::dpi::LogicalSize::new(760.0f64, 520.0f64));
        let window = el
            .create_window(attrs)
            .expect("could not create the window");
        window.set_visible(true);
        self.app.window = Some(window);

        let size = self.app.window.as_ref().unwrap().inner_size();
        let scale = self
            .app
            .window
            .as_ref()
            .unwrap()
            .scale_factor() as f32;
        self.app.scale = scale;
        let w = (size.width as f32 * scale) as u32;
        let h = (size.height as f32 * scale) as u32;

        let shots: Vec<(Vec<u8>, u32, u32)> = APPS
            .iter()
            .filter_map(|a| crate::screens::screenshot(a.slug))
            .collect();
        let gpu = match self.app.window.as_ref().map(|win| Gpu::new(win, (w, h), &shots)) {
            Some(Ok(g)) => Some(g),
            Some(Err(e)) => {
                eprintln!("artcraft-launcher: {e}");
                let t = self.app.clock.t();
                self.app.l.toast(format!("GPU unavailable: {e}"), ToastKind::Bad, t);
                None
            }
            None => None,
        };
        self.app.gpu = gpu;
        self.app.canvas = Canvas::new(w as usize, h as usize);
        if let Some(g) = &mut self.app.gpu {
            g.hero.resize(w, h);
        }
        // First run: write the launcher's own desktop entry / shortcut /
        // bundle and reserve every icon, then register the apps that are
        // already installed. Silent, and skippable in Settings.
        self.app.onboard_once();
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, ev: WindowEvent) {
        match ev {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(sz) => {
                let scale = self.app.scale;
                let w = (sz.width as f32 * scale) as u32;
                let h = (sz.height as f32 * scale) as u32;
                self.app.canvas = Canvas::new(w as usize, h as usize);
                if let Some(g) = &mut self.app.gpu {
                    g.resize(w, h);
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.app.scale = scale_factor as f32;
                if let Some(win) = &self.app.window {
                    let sz = win.inner_size();
                    let w = (sz.width as f32 * scale_factor as f32) as u32;
                    let h = (sz.height as f32 * scale_factor as f32) as u32;
                    self.app.canvas = Canvas::new(w as usize, h as usize);
                    if let Some(g) = &mut self.app.gpu {
                        g.resize(w, h);
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let scale = self.app.scale;
                self.app.cursor = (position.x as f32 * scale, position.y as f32 * scale);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if button == winit::event::MouseButton::Left {
                    self.app.down = state == ElementState::Pressed;
                    if state == ElementState::Pressed {
                        self.app.clicking = Some(self.app.cursor);
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let d = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y * 44.0,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32,
                };
                self.app.scroll_target = (self.app.scroll_target + d).max(0.0);
            }
            WindowEvent::RedrawRequested => {
                self.frame_dispatch(el);
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _el: &ActiveEventLoop) {
        if let Some(w) = &self.app.window {
            w.request_redraw();
        }
    }
}

impl Shell {
    fn frame_dispatch(&mut self, el: &ActiveEventLoop) {
        self.app.forward_frame(el);
    }
}

impl App {
    fn forward_frame(&mut self, el: &ActiveEventLoop) {
        let now = Instant::now();
        let dt = (now - self.last).as_secs_f64();
        self.last = now;
        self.clock.advance(dt);
        let t = self.clock.t();

        // background jobs
        if self.pending_check {
            self.poll_jobs();
        }
        // watch running apps
        self.poll_watched();

        // intro + animations
        self.maybe_restart_intro();
        let intro_done = self.intro.advance(dt, self.l.settings.animations == false);
        let _ = intro_done;
        self.marquee.tick(if self.l.settings.animations { dt } else { 0.0 });
        self.odo.set(
            self.scroll / self.scroll_max.max(1.0) * 100.0,
            t,
        );

        // scroll easing
        let k = (dt * 12.0).clamp(0.0, 1.0) as f32;
        self.scroll += (self.scroll_target - self.scroll) * k;
        let bounds = self.screen_scroll_bounds();
        self.scroll_max = bounds.max(1.0);
        self.scroll_target = self.scroll_target.min(bounds);

        let tokens = self.tokens();
        let dark = tokens.is_dark();

        // ---- draw -------------------------------------------------------
        let w = self.canvas.w;
        let h = self.canvas.h;
        let mut painter = Painter::new(
            &mut self.canvas,
            &mut self.text,
            tokens,
            t,
            &self.clock,
            !self.l.settings.animations,
        );
        let frame = Frame {
            mouse: self.cursor,
            click: self.clicking.take(),
            down: self.down,
            wheel: 0.0,
        };
        let env = Env {
            now: t,
            intro: self.intro.copy_p(!self.l.settings.animations),
            reveal: self.intro.wordmark_p(!self.l.settings.animations),
            scroll: self.scroll,
            scroll_max: self.scroll_max,
            marquee: &self.marquee,
            odo: &self.odo,
            shows: if self.intro.played { 1.0 } else { 0.0 },
        };
        let action = crate::screens::draw(&mut painter, &mut self.l, frame, &env);

        // ---- GPU --------------------------------------------------------
        if let Some(g) = &mut self.gpu {
            g.hero.set_dark(dark);
            g.hero.update(
                dt,
                self.intro.scrim_p(!self.l.settings.animations),
                self.scroll as f64,
            );
            g.render(&self.canvas);
        }

        // ---- act --------------------------------------------------------
        self.apply(action, el);
        if self.l.toasts.iter().any(|x| t - x.born > 5.0) {
            self.l.toasts.retain(|x| t - x.born < 5.0);
        }
        let _ = (w, h);
        el.set_control_flow(ControlFlow::Poll);
    }

    /// The first-run pass: register the launcher itself and every app that is
    /// already installed. Runs once, off the config's `onboarded` flag, and is
    /// a no-op if the user opted out of automatic registration.
    fn onboard_once(&mut self) {
        if !self.l.needs_onboarding() {
            return;
        }
        let exe = std::env::current_exe().ok();
        let mut registered: Vec<String> = Vec::new();
        let mut notes: Vec<String> = Vec::new();

        // 1. the launcher itself
        match exe.as_deref().map(crate::osint::register_launcher) {
            Some(Ok(_)) => self.l.onboarding.launcher = true,
            Some(Err(e)) => notes.push(format!("launcher: {e}")),
            None => notes.push("launcher: current_exe unavailable".into()),
        }

        // 2. every app already on disk
        for i in 0..APPS.len() {
            let slug = APPS[i].slug;
            let version = self.l.runtime[i].active.clone();
            let Some(v) = version else { continue };
            match self.register_one(i, &v) {
                Ok(_) => registered.push(slug.to_string()),
                Err(e) => notes.push(format!("{slug}: {e}")),
            }
        }

        self.settings_mark_onboarded();
        self.l.onboarding.done = true;
        self.l.onboarding.apps = registered;
        self.l.onboarding.notes = notes;
        self.l.onboarding.ran_at = config::now_unix();

        let n = self.l.onboarding.apps.len();
        self.l.toast(
            format!(
                "Registered {} app{} with the system. You can turn this off in Settings.",
                n,
                if n == 1 { "" } else { "s" }
            ),
            ToastKind::Good,
            self.clock.t(),
        );
    }

    /// Register one app and record that it is integrated.
    fn register_one(&mut self, idx: usize, version: &str) -> Result<(), String> {
        let a = &APPS[idx];
        let exe = std::env::current_exe().ok();
        let settings: Settings = self.l.settings.clone();
        crate::osint::integrate(a, version, &settings, exe.as_deref())?;
        self.l.runtime[idx].integrated = true;
        self.l.save();
        Ok(())
    }

    /// Register `slug` because it was just installed or the user asked.
    fn auto_register(&mut self, slug: &str) {
        if !self.l.settings.integrations.auto_register {
            return;
        }
        let Some(idx) = APPS.iter().position(|x| x.slug == slug) else {
            return;
        };
        let Some(v) = self.l.runtime[idx].active.clone() else {
            return;
        };
        match self.register_one(idx, &v) {
            Ok(_) => {
                self.l.onboarding.apps.push(slug.to_string());
            }
            Err(e) => self.l.toast(
                format!("{} register failed: {e}", APPS[idx].name),
                ToastKind::Warn,
                self.clock.t(),
            ),
        }
    }

    fn settings_mark_onboarded(&mut self) {
        self.l.settings.integrations.onboarded = true;
        let _ = self.l.settings.save();
    }

    fn apply(&mut self, action: Action, el: &ActiveEventLoop) {
        match action {
            Action::None => {}
            Action::Goto(_) => {}
            Action::Select(_) => {}
            Action::Theme(th) => {
                self.l.settings.theme = th;
                let _ = self.l.settings.save();
            }
            Action::Quit => el.exit(),
            Action::Install(slug, ver) => self.start_install(&slug, &ver),
            Action::Launch(slug, ver) => self.launch(&slug, &ver),
            Action::CheckAll => {
                self.check_updates();
                self.l.toast("Checking GitHub…", ToastKind::Info, self.clock.t());
            }
            Action::Prompt(slug, ver) => {
                self.l.prompt = Some((slug, ver));
            }
            Action::AcceptPrompt => {
                if let Some((slug, ver)) = self.l.prompt.take() {
                    self.start_install(&slug, &ver);
                }
            }
            Action::DeclinePrompt => {
                self.l.prompt = None;
                self.l.toast("Left alone", ToastKind::Info, self.clock.t());
            }
            Action::Rollback(slug) => {
                if let Some(a) = crate::apps::app(&slug) {
                    match crate::installer::rollback(a) {
                        Ok(v) => {
                            let i = APPS.iter().position(|x| x.slug == slug).unwrap_or(0);
                            self.l.runtime[i].previous = self.l.runtime[i].active.clone();
                            self.l.runtime[i].active = Some(v.clone());
                            self.l.save();
                            self.l.toast(
                                format!("Rolled {} back to {}", a.name, v),
                                ToastKind::Good,
                                self.clock.t(),
                            );
                        }
                        Err(e) => self.l.toast(e, ToastKind::Bad, self.clock.t()),
                    }
                }
            }
            Action::Backup(slug) => {
                if let Some(a) = crate::apps::app(&slug) {
                    let v = self
                        .l
                        .runtime
                        .iter()
                        .find(|_r| true)
                        .map(|_| {
                            APPS
                                .iter()
                                .position(|x| x.slug == slug)
                                .and_then(|i| self.l.runtime[i].active.clone())
                                .unwrap_or_default()
                        })
                        .unwrap_or_default();
                    match crate::installer::backup_data(a, &v, crate::installer::BackupKind::Data, "manual") {
                        Ok(m) => {
                            self.l.refresh_from_disk();
                            self.l.toast(
                                format!("Backed up {} ({} files)", a.name, m.entries),
                                ToastKind::Good,
                                self.clock.t(),
                            );
                        }
                        Err(e) => self.l.toast(e, ToastKind::Bad, self.clock.t()),
                    }
                }
            }
            Action::Restore(slug) => {
                if let Some(a) = crate::apps::app(&slug) {
                    match crate::installer::restore_backup(a.slug, None) {
                        Ok(_m) => self.l.toast(
                            format!("Restored {} backup", a.name),
                            ToastKind::Good,
                            self.clock.t(),
                        ),
                        Err(e) => self.l.toast(format!("Restore failed: {e}"), ToastKind::Bad, self.clock.t()),
                    }
                }
            }
            Action::Wipe(slug) => {
                if let Some(a) = crate::apps::app(&slug) {
                    match crate::installer::wipe_data(a, "0.0.0") {
                        Ok(_) => {
                            self.l.refresh_from_disk();
                            self.l.toast(
                                format!("Wiped {} data (a safety copy was kept)", a.name),
                                ToastKind::Warn,
                                self.clock.t(),
                            );
                        }
                        Err(e) => self.l.toast(format!("Wipe failed: {e}"), ToastKind::Bad, self.clock.t()),
                    }
                }
            }
            Action::OpenData(slug) => self.open_data_dir(&slug),
            Action::Integrate(slug) => self.integrate(&slug),
            Action::Export => self.export(),
            Action::Import => self.import(),
            Action::SetEndpoint(_) | Action::SetTelemetry(_) => {
                let _ = self.l.settings.save();
            }
            Action::ToggleIntegration(_) => {
                let _ = self.l.settings.save();
            }
            Action::ClearSpool => {
                let s = telemetry::Spool::new();
                for (p, _) in s.take(500) {
                    s.remove(&p);
                }
                self.l.reports.clear();
            }
        }
    }

    // ------------------------------------------------------------- actions

    fn start_install(&mut self, slug: &str, version: &str) {
        let slug = slug.to_string();
        let version = version.to_string();
        let a = match crate::apps::app(&slug) {
            Some(a) => a,
            None => return,
        };
        let kind = self.l.runtime[APPS.iter().position(|x| x.slug == slug).unwrap_or(0)].kind;
        let (tx, rx) = std::sync::mpsc::channel();
        self.job_tx = Some(tx.clone());
        self.job_rx = Some(rx);
        self.l.jobs.push(crate::state::Job {
            slug: slug.clone(),
            phase: "starting".into(),
            got: 0,
            total: 0,
            done: false,
            error: None,
            version: version.clone(),
        });
        self.trail.push(format!("install {} {}", slug, version));
        // Clone everything the thread will touch before it takes the closure
        // by move; the progress closure owns one sender, the thread the other.
        let slug_p = slug.clone();
        let version_p = version.clone();
        let slug_done = slug.clone();
        let version_done = version.clone();
        let tx_progress = tx.clone();
        let install_version = version_p.clone();
        std::thread::spawn(move || {
            let res = crate::installer::install(a, &install_version, kind, move |p| {
                let _ = tx_progress.send(JobMsg::Progress(
                    slug_p.clone(),
                    version_p.clone(),
                    p.got,
                    p.total,
                ));
            });
            let out = res.map(|_| ()).map_err(|e| e);
            let _ = tx.send(JobMsg::Done(slug_done, version_done, out));
        });
    }

    fn launch(&mut self, slug: &str, version: &str) {
        let Some(a) = crate::apps::app(slug) else {
            return;
        };
        // Snapshot the settings for the app, capture pre-crash state, launch,
        // and keep the child around so we can report an abnormal exit.
        let idx = APPS.iter().position(|x| x.slug == slug).unwrap_or(0);
        let params = self.l.runtime[idx].params.clone();
        self.trail.push(format!("launch {} {}", slug, version));
        let pre = self.trail.recent(12);
        let log_path = crate::paths::app_log_file(slug);
        match crate::installer::spawn(a, version, &params) {
            Ok(child) => {
                self.l.runtime[idx].active = Some(version.to_string());
                self.l.runtime[idx].launches += 1;
                self.l.runtime[idx].last_launch = config::now_unix();
                self.l.runtime[idx].running = true;
                self.l.save();
                let i = idx;
                self.watched.push(Watched {
                    slug: slug.to_string(),
                    version: version.to_string(),
                    child,
                    started_ms: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis())
                        .unwrap_or(0),
                    metrics: DeviceMetrics::sample(),
                    log_offset: std::fs::metadata(&log_path).map(|m| m.len()).unwrap_or(0),
                    log_path,
                    pre_crash: pre,
                });
                let _ = i;
            }
            Err(e) => {
                self.report_manual(slug, version, ReportKind::LaunchFailed, &e, Vec::new(), pre);
                self.l.toast(e, ToastKind::Bad, self.clock.t());
            }
        }
    }

    fn report_manual(
        &mut self,
        slug: &str,
        version: &str,
        kind: ReportKind,
        message: &str,
        log_tail: Vec<String>,
        pre: Vec<String>,
    ) {
        let r = Report {
            id: format!("{slug}-{version}-{}", telemetry::now_ms()),
            ts: telemetry::now_unix_or(0),
            ts_ms: telemetry::now_ms(),
            kind,
            app: slug.to_string(),
            version: version.to_string(),
            launcher: env!("CARGO_PKG_VERSION").to_string(),
            exit_code: None,
            signal: None,
            message: message.to_string(),
            stack: Vec::new(),
            log_tail,
            pre_crash: pre,
            metrics: Some(DeviceMetrics::sample()),
            causes: Vec::new(),
            ran_ms: 0,
        };
        self.spool.put(&r);
        self.l.reports.insert(0, r);
        telemetry::log_event("launch_failed", message);
    }

    fn poll_watched(&mut self) {
        let cfg = self.l.settings.telemetry.clone();
        let mut reports: Vec<Report> = Vec::new();
        let mut keep = Vec::new();
        for mut w in std::mem::take(&mut self.watched) {
            if let Some(r) = w.poll(&cfg, &self.trail) {
                if r.kind != ReportKind::Error || r.exit_code.unwrap_or(0) != 0 {
                    reports.push(r.clone());
                    self.spool.put(&r);
                } else {
                    self.spool.put(&r);
                }
                let i = APPS
                    .iter()
                    .position(|x| x.slug == w.slug.as_str())
                    .unwrap_or(0);
                self.l.runtime[i].running = false;
                self.l.runtime[i].last_launch = config::now_unix();
            } else {
                keep.push(w);
            }
        }
        self.watched = keep;
        for r in reports {
            self.l.reports.insert(0, r);
        }
        // flush the spool
        let sent = telemetry::flush(&self.l.settings.telemetry, &self.spool);
        let _ = sent;
    }

    fn poll_jobs(&mut self) {
        let Some(rx) = &self.job_rx else { return };
        let mut msgs = Vec::new();
        loop {
            match rx.try_recv() {
                Ok(m) => msgs.push(m),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.pending_check = false;
                    break;
                }
            }
        }
        for m in msgs {
            match m {
                JobMsg::Progress(slug, ver, got, total) => {
                    if let Some(j) = self
                        .l
                        .jobs
                        .iter_mut()
                        .find(|j| j.slug == slug && j.version == ver && !j.done)
                    {
                        j.got = got;
                        j.total = total;
                        j.phase = "downloading".into();
                    }
                }
                JobMsg::Latest(slug, ver) => {
                    let i = APPS.iter().position(|x| x.slug == slug).unwrap_or(0);
                    self.l.runtime[i].latest_checking = false;
                    if crate::github::cmp_ver(&ver, self.l.runtime[i].active.as_deref().unwrap_or("0.0.0"))
                        == std::cmp::Ordering::Greater
                    {
                        self.l.runtime[i].latest = Some(crate::github::Release {
                            tag_name: format!("v{ver}"),
                            name: None,
                            body: None,
                            prerelease: false,
                            draft: false,
                            published_at: None,
                            html_url: format!("https://github.com/storytold/{slug}/releases/tag/v{ver}"),
                            assets: Vec::new(),
                        });
                        // Only ask if the user has not already declined it.
                        if self.l.runtime[i].prompt
                            && !self.l.runtime[i].muted.iter().any(|m| m == &ver)
                            && self.l.runtime[i].declined.as_deref() != Some(ver.as_str())
                            && self.l.settings.check_on_launch
                        {
                            self.l.prompt = Some((slug.clone(), ver.clone()));
                        }
                    }
                }
                JobMsg::Done(slug, ver, res) => {
                    if let Some(j) = self
                        .l
                        .jobs
                        .iter_mut()
                        .find(|j| j.slug == slug && j.version == ver)
                    {
                        j.done = true;
                        match res {
                            Ok(_) => {
                                j.phase = "installed".into();
                                let i = APPS.iter().position(|x| x.slug == slug).unwrap_or(0);
                                self.l.runtime[i].active = Some(ver.clone());
                                self.l.runtime[i].previous = self.l.runtime[i].active.clone();
                                self.l.save();
                                self.trail.push(format!("installed {} {}", slug, ver));
                                // A freshly installed app registers itself, so
                                // it is immediately launchable and has a
                                // right-click entry.
                                self.auto_register(&slug);
                                self.l.toast(
                                    format!("{} {} installed", crate::apps::app(&slug).map(|a| a.name).unwrap_or(&slug), ver),
                                    ToastKind::Good,
                                    self.clock.t(),
                                );
                            }
                            Err(e) => {
                                j.error = Some(e.clone());
                                self.l.toast(
                                    format!("Install failed: {e}"),
                                    ToastKind::Bad,
                                    self.clock.t(),
                                );
                                self.trail.push(format!("install failed {} {}: {}", slug, ver, e));
                            }
                        }
                    }
                }
            }
        }
        // A background "latest release" probe stashes its result through Done
        // with a bogus path; translate that into `latest`.
        self.pending_check = false;
    }

    fn open_data_dir(&mut self, slug: &str) {
        let d = crate::paths::app_data_dir(slug);
        self.l.data_dir_open = Some(d.clone());
        let _ = std::fs::create_dir_all(&d);
        let res = (|| -> Result<(), String> {
            #[cfg(target_os = "macos")]
            {
                std::process::Command::new("open").arg(&d).spawn().map_err(|e| e.to_string())?;
            }
            #[cfg(target_os = "windows")]
            {
                std::process::Command::new("explorer").arg(&d).spawn().map_err(|e| e.to_string())?;
            }
            #[cfg(all(unix, not(target_os = "macos")))]
            {
                std::process::Command::new("xdg-open").arg(&d).spawn().map_err(|e| e.to_string())?;
            }
            Ok(())
        })();
        match res {
            Ok(()) => self.l.toast(
                format!("Opened {}", d.display()),
                ToastKind::Info,
                self.clock.t(),
            ),
            Err(_) => self.l.toast(
                format!("Data dir: {}", d.display()),
                ToastKind::Info,
                self.clock.t(),
            ),
        }
    }

    fn integrate(&mut self, slug: &str) {
        let Some(a) = crate::apps::app(slug) else { return };
        let exe = std::env::current_exe().ok();
        let s: Settings = self.l.settings.clone();
        match crate::osint::integrate(a, &self.version_of(slug), &s, exe.as_deref()) {
            Ok(rep) => {
                let i = APPS.iter().position(|x| x.slug == slug).unwrap_or(0);
                self.l.runtime[i].integrated = true;
                self.l.save();
                let mut what = Vec::new();
                if rep.desktop_entry.is_some() {
                    what.push("desktop entry");
                }
                if !rep.context_actions.is_empty() {
                    what.push("right-click actions");
                }
                if !rep.icons.is_empty() {
                    what.push("icons");
                }
                if !rep.shortcuts.is_empty() {
                    what.push("shortcut");
                }
                if rep.reserved.is_some() {
                    what.push("reserved icon");
                }
                self.l.toast(
                    format!("Registered {}: {}", a.name, what.join(", ")),
                    ToastKind::Good,
                    self.clock.t(),
                );
                self.trail.push(format!("integrate {}", slug));
            }
            Err(e) => self.l.toast(
                format!("Register failed: {e}"),
                ToastKind::Bad,
                self.clock.t(),
            ),
        }
    }

    fn version_of(&self, slug: &str) -> String {
        APPS
            .iter()
            .position(|x| x.slug == slug)
            .and_then(|i| self.l.runtime[i].active.clone())
            .unwrap_or_else(|| {
                crate::apps::app(slug).map(|a| a.pinned.to_string()).unwrap_or_default()
            })
    }

    fn export(&mut self) {
        let path = crate::paths::default_export_path();
        match config::export_to(&path, &self.l.settings) {
            Ok(p) => self.l.toast(
                format!("Exported to {}", p.display()),
                ToastKind::Good,
                self.clock.t(),
            ),
            Err(e) => self.l.toast(format!("Export failed: {e}"), ToastKind::Bad, self.clock.t()),
        }
    }

    fn import(&mut self) {
        let path = crate::paths::default_export_path();
        match config::import_from(&path) {
            Ok(e) => {
                self.l.settings = e.settings;
                let _ = self.l.settings.save();
                self.l.refresh_from_disk();
                self.l.toast("Imported settings", ToastKind::Good, self.clock.t());
            }
            Err(e) => self.l.toast(format!("Import failed: {e}"), ToastKind::Bad, self.clock.t()),
        }
    }
}

/// Run the GUI.
pub fn run() -> Result<(), String> {
    let el = EventLoop::new().map_err(|e| e.to_string())?;
    let mut shell = Shell {
        app: App::new(),
    };
    el.run_app(&mut shell).map_err(|e| e.to_string())
}

/// Re-export for the CLI.
pub fn arch() -> Arch {
    Arch::host()
}
pub fn platform() -> Platform {
    Platform::host()
}
