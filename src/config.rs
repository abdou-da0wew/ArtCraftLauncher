//! Launcher settings, per-app preferences, and export/import.
//!
//! The settings file is `launcher_root()/config.json`. Export produces a
//! single portable JSON document (version + launcher prefs + every app's
//! prefs, launch params, data-dir manifest and backups index) that can be
//! imported on another machine — including across platforms, where paths are
//! rewritten to the target's conventions.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::apps::{Arch, Channel, Distro, InstallKind, Platform};
use crate::paths;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchParams {
    /// Extra CLI arguments appended to the app's own argv.
    #[serde(default)]
    pub args: Vec<String>,
    /// Extra environment variables set for the child process.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Open the given file(s) after launch.
    #[serde(default)]
    pub open_files: Vec<String>,
    /// Keep the app's log open in the launcher's crash tracer.
    #[serde(default = "default_true")]
    pub trace: bool,
}

fn default_true() -> bool {
    true
}

impl Default for LaunchParams {
    fn default() -> Self {
        Self {
            args: Vec::new(),
            env: BTreeMap::new(),
            open_files: Vec::new(),
            trace: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppState {
    /// Version currently installed (the "active" version), if any.
    pub installed: Option<String>,
    /// Version to fall back to after a rollback.
    pub previous: Option<String>,
    /// Release channel pinned for this app.
    #[serde(default = "Channel::stable")]
    pub channel: Channel,
    /// How the app was installed.
    #[serde(default = "InstallKind::portable_default")]
    pub kind: InstallKind,
    /// Versions the user has said "don't ask me about this one again".
    #[serde(default)]
    pub muted_versions: Vec<String>,
    /// Whether the auto-update prompt is enabled at all.
    #[serde(default = "default_true")]
    pub prompt_updates: bool,
    #[serde(default)]
    pub params: LaunchParams,
    /// Last known good version, captured before an update.
    #[serde(default)]
    pub last_good: Option<String>,
    /// Unix seconds of the last successful launch.
    #[serde(default)]
    pub last_launch: u64,
    /// Total launches.
    #[serde(default)]
    pub launches: u64,
    /// Whether the desktop entry / shortcut / file associations were written.
    #[serde(default)]
    pub os_integrated: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            installed: None,
            previous: None,
            channel: Channel::Stable,
            kind: InstallKind::SelfManaged,
            muted_versions: Vec::new(),
            prompt_updates: true,
            params: LaunchParams::default(),
            last_good: None,
            last_launch: 0,
            launches: 0,
            os_integrated: false,
        }
    }
}

impl AppState {
    pub fn has_update_pending(&self, latest: &str) -> bool {
        match &self.installed {
            None => true,
            Some(cur) => crate::github::cmp_ver(latest, cur) == std::cmp::Ordering::Greater,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryConfig {
    /// Where crash reports are POSTed. Empty means "spool only".
    #[serde(default)]
    pub endpoint: String,
    /// Bearer token, if the endpoint needs one.
    #[serde(default)]
    pub token: String,
    /// Master switch. Defaults to on, and can always be turned off.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Include device resource metrics.
    #[serde(default = "default_true")]
    pub metrics: bool,
    /// Include the app's own log tail (the last N lines of <slug>.log).
    #[serde(default = "default_true")]
    pub log_tail: bool,
    /// How many spooled reports to keep on disk.
    #[serde(default = "default_spool")]
    pub max_spool: usize,
    /// Upload interval, seconds.
    #[serde(default = "default_interval")]
    pub interval: u64,
}

fn default_spool() -> usize {
    200
}
fn default_interval() -> u64 {
    60
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            endpoint: String::new(),
            token: String::new(),
            enabled: true,
            metrics: true,
            log_tail: true,
            max_spool: default_spool(),
            interval: default_interval(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub schema: u32,
    #[serde(default)]
    pub theme: crate::theme::Theme,
    #[serde(default = "InstallKind::default_kind")]
    pub install_kind: InstallKind,
    #[serde(default = "default_true")]
    pub check_on_launch: bool,
    #[serde(default = "default_true")]
    pub animations: bool,
    #[serde(default)]
    pub integrations: Integrations,
    #[serde(default)]
    pub telemetry: TelemetryConfig,
    #[serde(default)]
    pub apps: BTreeMap<String, AppState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Integrations {
    /// Write `.desktop` files, icons and file associations on Linux.
    #[serde(default = "default_true")]
    pub linux_desktop_entries: bool,
    /// Add file-manager right-click actions on Linux.
    #[serde(default = "default_true")]
    pub linux_context_actions: bool,
    /// Reserve a taskbar/Explorer icon on Windows.
    #[serde(default = "default_true")]
    pub windows_icons: bool,
    /// Create Start Menu shortcuts on Windows.
    #[serde(default = "default_true")]
    pub windows_start_menu: bool,
    /// Register file associations on Windows.
    #[serde(default = "default_true")]
    pub windows_associations: bool,
    /// Reserve a per-app icon in the macOS bundle + Launch Services.
    #[serde(default = "default_true")]
    pub macos_icons: bool,
    /// Register the bundle with Launch Services (mdimport / lsregister).
    #[serde(default = "default_true")]
    pub macos_launch_services: bool,
    /// Do all of the above automatically the first time the launcher runs, and
    /// whenever an app is installed. Off only if the user opts out.
    #[serde(default = "default_true")]
    pub auto_register: bool,
    /// Set once the first-run onboarding has completed.
    #[serde(default)]
    pub onboarded: bool,
}

/// `Default` must agree with the `#[serde(default)]` values — the derived
/// `Default` would give `false` for every bool, silently disabling all
/// integration for anyone who never writes a config file.
impl Default for Integrations {
    fn default() -> Self {
        Self {
            linux_desktop_entries: true,
            linux_context_actions: true,
            windows_icons: true,
            windows_start_menu: true,
            windows_associations: true,
            macos_icons: true,
            macos_launch_services: true,
            auto_register: true,
            onboarded: false,
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema: SCHEMA_VERSION,
            theme: crate::theme::Theme::System,
            install_kind: InstallKind::default_kind(),
            check_on_launch: true,
            animations: true,
            integrations: Integrations::default(),
            telemetry: TelemetryConfig::default(),
            apps: BTreeMap::new(),
        }
    }
}

impl Settings {
    pub fn app(&self, slug: &str) -> AppState {
        self.apps.get(slug).cloned().unwrap_or_default()
    }
    pub fn app_mut(&mut self, slug: &str) -> &mut AppState {
        self.apps.entry(slug.to_string()).or_default()
    }
    pub fn load() -> Self {
        let p = paths::config_file();
        match std::fs::read_to_string(&p) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }
    pub fn save(&self) -> Result<(), String> {
        let p = paths::config_file();
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        let tmp = p.with_extension("json.tmp");
        let s = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&tmp, s).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &p).map_err(|e| e.to_string())?;
        Ok(())
    }
}

/// The portable export document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Export {
    pub schema: u32,
    pub exported_by: String,
    pub unix: u64,
    pub platform: Platform,
    pub arch: Arch,
    pub distro: Distro,
    #[serde(default)]
    pub settings: Settings,
    /// Per-app data-dir snapshots: slug → path + byte size + entry count.
    #[serde(default)]
    pub data_manifest: BTreeMap<String, DataManifest>,
    /// Backups recorded at export time, so an import can relink them.
    #[serde(default)]
    pub backups: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DataManifest {
    pub path: String,
    pub exists: bool,
    pub bytes: u64,
    pub entries: u64,
}

pub fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn build_export(settings: &Settings) -> Export {
    let mut data_manifest = BTreeMap::new();
    let mut backups = BTreeMap::new();
    for a in crate::apps::APPS {
        let d = paths::app_data_dir(a.slug);
        let (exists, bytes, entries) = if d.exists() {
            let bytes = paths::dir_size(&d);
            let entries = std::fs::read_dir(&d).map(|r| r.count() as u64).unwrap_or(0);
            (true, bytes, entries)
        } else {
            (false, 0, 0)
        };
        data_manifest.insert(
            a.slug.to_string(),
            DataManifest {
                path: d.to_string_lossy().to_string(),
                exists,
                bytes,
                entries,
            },
        );
        let mut bs = Vec::new();
        let bdir = paths::backups_root().join(a.slug);
        if let Ok(rd) = std::fs::read_dir(&bdir) {
            for e in rd.flatten() {
                if e.path().is_dir() {
                    bs.push(e.file_name().to_string_lossy().to_string());
                }
            }
        }
        bs.sort();
        backups.insert(a.slug.to_string(), bs);
    }
    Export {
        schema: SCHEMA_VERSION,
        exported_by: format!("{} {}", LAUNCHER_NAME, env!("CARGO_PKG_VERSION")),
        unix: now_unix(),
        platform: Platform::host(),
        arch: Arch::host(),
        distro: Distro::detect(),
        settings: settings.clone(),
        data_manifest,
        backups,
    }
}

pub const LAUNCHER_NAME: &str = "ArtCraft Studio";

pub fn export_to(path: &Path, settings: &Settings) -> Result<PathBuf, String> {
    let e = build_export(settings);
    let s = serde_json::to_string_pretty(&e).map_err(|e| e.to_string())?;
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, s).map_err(|e| e.to_string())?;
    Ok(path.to_path_buf())
}

pub fn import_from(path: &Path) -> Result<Export, String> {
    let s = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let e: Export = serde_json::from_str(&s).map_err(|e| e.to_string())?;
    if e.schema > SCHEMA_VERSION {
        return Err(format!(
            "export schema {} is newer than this launcher ({}); upgrade",
            e.schema, SCHEMA_VERSION
        ));
    }
    Ok(e)
}
