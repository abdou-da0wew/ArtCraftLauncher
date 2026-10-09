//! Launcher state: the screen router, per-app runtime model, and the
//! in-flight update prompt the user is allowed to decline.

use std::path::PathBuf;

use crate::apps::{Channel, CraftApp, InstallKind};
use crate::config::{AppState, Settings};
use crate::github::Release;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Screen {
    Library,
    App,
    Updates,
    Settings,
    Telemetry,
}

impl Screen {
    pub fn title(self) -> &'static str {
        match self {
            Screen::Library => "Studio",
            Screen::App => "App",
            Screen::Updates => "Updates",
            Screen::Settings => "Settings",
            Screen::Telemetry => "Crash reports",
        }
    }
    /// The sticky eyebrow: `index / label / annotation`.
    pub fn eyebrow(self) -> (&'static str, &'static str, Option<&'static str>) {
        match self {
            Screen::Library => ("01", "Crafting apps", Some("Seven apps · Open source · Pure Rust")),
            Screen::App => ("02", "Manage", None),
            Screen::Updates => ("03", "Updates", Some("You always decide")),
            Screen::Settings => ("04", "Settings", Some("Preferences & integrations")),
            Screen::Telemetry => ("05", "Crash reports", Some("Local spool · Never lost")),
        }
    }
}

/// A background job (download/install/update), driven from the UI thread so
/// there is exactly one place work happens and it is always cancellable.
#[derive(Clone, Debug)]
pub struct Job {
    pub slug: String,
    pub phase: String,
    pub got: u64,
    pub total: u64,
    pub done: bool,
    pub error: Option<String>,
    pub version: String,
}

impl Job {
    pub fn frac(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            (self.got as f64 / self.total as f64).clamp(0.0, 1.0) as f32
        }
    }
}

/// Live runtime state for one app.
#[derive(Debug, Clone, Default)]
pub struct AppRuntime {
    pub installed: Vec<String>,
    pub active: Option<String>,
    pub previous: Option<String>,
    pub channel: Channel,
    pub kind: InstallKind,
    pub latest: Option<Release>,
    pub latest_checking: bool,
    pub muted: Vec<String>,
    pub prompt: bool,
    pub params: crate::config::LaunchParams,
    pub last_launch: u64,
    pub launches: u64,
    pub integrated: bool,
    /// Set when the user declined this version, so we never ask again.
    pub declined: Option<String>,
    pub running: bool,
    pub data_bytes: u64,
    pub backups: usize,
}

impl AppRuntime {
    /// The active version is the recorded one if it is still on disk, else the
    /// newest installed version — a reset config must never forget what is
    /// actually installed.
    fn active_version(a: &CraftApp, s: &AppState, disk: &crate::installer::Disk) -> Option<String> {
        match &s.installed {
            Some(v) if disk.versions(a.slug).iter().any(|x| x == v) => Some(v.clone()),
            _ => disk.versions(a.slug).into_iter().next(),
        }
    }

    pub fn from(a: &CraftApp, s: &AppState, disk: &crate::installer::Disk) -> Self {
        let active = Self::active_version(a, s, disk);
        Self {
            installed: disk.versions(a.slug),
            active,
            previous: s.previous.clone(),
            channel: s.channel,
            kind: s.kind,
            latest: None,
            latest_checking: false,
            muted: s.muted_versions.clone(),
            prompt: s.prompt_updates,
            params: s.params.clone(),
            last_launch: s.last_launch,
            launches: s.launches,
            integrated: s.os_integrated,
            declined: None,
            running: false,
            data_bytes: disk.data_bytes(a.slug),
            backups: disk.backups(a.slug),
        }
    }

    pub fn update_pending(&self) -> bool {
        match (&self.latest, &self.active) {
            (Some(r), Some(cur)) => {
                crate::github::cmp_ver(&r.version(), cur) == std::cmp::Ordering::Greater
            }
            (Some(_), None) => true,
            _ => false,
        }
    }

    /// The version the user should be offered, if any, honouring declines.
    pub fn offered_version(&self) -> Option<String> {
        if !self.prompt || self.declined.is_some() {
            return None;
        }
        let r = self.latest.as_ref()?;
        let v = r.version();
        if self.muted.iter().any(|m| m == &v) {
            return None;
        }
        if self.update_pending() {
            Some(v)
        } else {
            None
        }
    }
}

/// Which nav item is active in the side rail.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Nav {
    Studio,
    Updates,
    Settings,
    Reports,
}

impl Nav {
    pub fn all() -> [Nav; 4] {
        [Nav::Studio, Nav::Updates, Nav::Settings, Nav::Reports]
    }
    pub fn label(self) -> &'static str {
        match self {
            Nav::Studio => "Apps",
            Nav::Updates => "Updates",
            Nav::Settings => "Settings",
            Nav::Reports => "Reports",
        }
    }
    pub fn screen(self) -> Screen {
        match self {
            Nav::Studio => Screen::Library,
            Nav::Updates => Screen::Updates,
            Nav::Settings => Screen::Settings,
            Nav::Reports => Screen::Telemetry,
        }
    }
}

/// What the first-run onboarding pass did. The UI shows it once, so the user
/// understands why their system suddenly knows about seven new apps.
#[derive(Debug, Clone, Default)]
pub struct Onboarding {
    pub done: bool,
    pub launcher: bool,
    pub apps: Vec<String>,
    pub notes: Vec<String>,
    pub ran_at: u64,
}

/// A transient toast.
#[derive(Clone, Debug)]
pub struct Toast {
    pub text: String,
    pub kind: ToastKind,
    pub born: f64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ToastKind {
    Info,
    Good,
    Warn,
    Bad,
}

pub struct Launcher {
    pub settings: Settings,
    pub screen: Screen,
    pub nav: Nav,
    /// Index into `crate::apps::APPS`.
    pub selected: usize,
    pub runtime: Vec<AppRuntime>,
    pub jobs: Vec<Job>,
    pub toasts: Vec<Toast>,
    pub scroll: f32,
    pub scroll_target: f32,
    /// The pending update prompt, if the user has not answered it yet.
    pub prompt: Option<(String, String)>,
    pub reports: Vec<crate::telemetry::Report>,
    pub data_dir_open: Option<PathBuf>,
    pub last_check: u64,
    pub onboarding: Onboarding,
    pub debug_stats: String,
}

impl Launcher {
    pub fn new() -> Self {
        let settings = Settings::load();
        let disk = crate::installer::Disk::scan();
        let runtime: Vec<AppRuntime> = crate::apps::APPS
            .iter()
            .map(|a| AppRuntime::from(a, &settings.app(a.slug), &disk))
            .collect();
        Self {
            settings,
            screen: Screen::Library,
            nav: Nav::Studio,
            selected: 0,
            runtime,
            jobs: Vec::new(),
            toasts: Vec::new(),
            scroll: 0.0,
            scroll_target: 0.0,
            prompt: None,
            reports: Vec::new(),
            data_dir_open: None,
            last_check: 0,
            onboarding: Onboarding::default(),
            debug_stats: String::new(),
        }
    }

    pub fn app(&self) -> &'static CraftApp {
        &crate::apps::APPS[self.selected]
    }

    pub fn rt(&self) -> &AppRuntime {
        &self.runtime[self.selected]
    }
    pub fn rt_mut(&mut self) -> &mut AppRuntime {
        &mut self.runtime[self.selected]
    }

    pub fn toast(&mut self, text: impl Into<String>, kind: ToastKind, now: f64) {
        self.toasts.push(Toast {
            text: text.into(),
            kind,
            born: now,
        });
        if self.toasts.len() > 4 {
            self.toasts.remove(0);
        }
    }

    pub fn pending_updates(&self) -> Vec<usize> {
        (0..self.runtime.len())
            .filter(|&i| self.runtime[i].offered_version().is_some())
            .collect()
    }

    /// True when the first-run pass should run: the user has not opted out and
    /// it has not completed yet.
    pub fn needs_onboarding(&self) -> bool {
        self.settings.integrations.auto_register && !self.settings.integrations.onboarded
    }

    pub fn save(&mut self) {
        for (i, a) in crate::apps::APPS.iter().enumerate() {
            let r = &self.runtime[i];
            let st = self.settings.app_mut(a.slug);
            st.installed = r.active.clone();
            st.previous = r.previous.clone();
            st.channel = r.channel;
            st.kind = r.kind;
            st.muted_versions = r.muted.clone();
            st.prompt_updates = r.prompt;
            st.params = r.params.clone();
            st.last_launch = r.last_launch;
            st.launches = r.launches;
            st.os_integrated = r.integrated;
        }
        let _ = self.settings.save();
    }

    pub fn refresh_from_disk(&mut self) {
        let disk = crate::installer::Disk::scan();
        for (i, a) in crate::apps::APPS.iter().enumerate() {
            self.runtime[i].installed = disk.versions(a.slug);
            self.runtime[i].data_bytes = disk.data_bytes(a.slug);
            self.runtime[i].backups = disk.backups(a.slug);
        }
    }
}
