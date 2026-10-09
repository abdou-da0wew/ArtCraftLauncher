//! Every filesystem location the launcher owns, per platform.
//!
//! The Craft apps use the standard `dirs::data_dir()` convention, which I
//! verified by running `pdfcraft` with a sandboxed `$HOME`: it created
//! `~/.local/share/pdfcraft/logs/pdfcraft.log`. So on Linux the data dir is
//! `$XDG_DATA_HOME/<slug>` (default `~/.local/share/<slug>`), on macOS
//! `~/Library/Application Support/<slug>`, and on Windows
//! `%APPDATA%\<slug>` (Roaming).

use std::path::{Path, PathBuf};

use crate::apps::{Distro, Platform};

pub const LAUNCHER_ID: &str = "ai.storyteller.artcraft-launcher";
pub const LAUNCHER_NAME: &str = "ArtCraft Studio";

fn env_path(k: &str) -> Option<PathBuf> {
    std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from)
}

// ---------------------------------------------------------------- home roots

/// `$XDG_DATA_HOME` | `~/Library/Application Support` | `%APPDATA%`
pub fn data_home() -> PathBuf {
    if let Some(p) = env_path("ARTCRAFT_DATA_HOME") {
        return p;
    }
    match Platform::host() {
        Platform::Macos => home().join("Library").join("Application Support"),
        Platform::Windows => env_path("APPDATA").unwrap_or_else(|| home().join("AppData").join("Roaming")),
        _ => env_path("XDG_DATA_HOME").unwrap_or_else(|| home().join(".local").join("share")),
    }
}

/// `$XDG_CONFIG_HOME` | `~/Library/Application Support` | `%APPDATA%`
pub fn config_home() -> PathBuf {
    if let Some(p) = env_path("ARTCRAFT_CONFIG_HOME") {
        return p;
    }
    match Platform::host() {
        Platform::Macos => data_home(),
        Platform::Windows => data_home(),
        _ => env_path("XDG_CONFIG_HOME").unwrap_or_else(|| home().join(".config")),
    }
}

/// `$XDG_CACHE_HOME` | `~/Library/Caches` | `%LOCALAPPDATA%`
pub fn cache_home() -> PathBuf {
    if let Some(p) = env_path("ARTCRAFT_CACHE_HOME") {
        return p;
    }
    match Platform::host() {
        Platform::Macos => home().join("Library").join("Caches"),
        Platform::Windows => {
            env_path("LOCALAPPDATA").unwrap_or_else(|| home().join("AppData").join("Local"))
        }
        _ => env_path("XDG_CACHE_HOME").unwrap_or_else(|| home().join(".cache")),
    }
}

/// `$XDG_STATE_HOME` | `~/.local/state`
pub fn state_home() -> PathBuf {
    if let Some(p) = env_path("ARTCRAFT_STATE_HOME") {
        return p;
    }
    if Platform::host() == Platform::Macos {
        return data_home();
    }
    env_path("XDG_STATE_HOME").unwrap_or_else(|| home().join(".local").join("state"))
}

pub fn home() -> PathBuf {
    env_path("HOME").or_else(|| env_path("USERPROFILE")).unwrap_or_else(|| PathBuf::from("."))
}

// ------------------------------------------------------------- launcher root

/// Where the launcher keeps its own state: `~/.local/share/artcraft-launcher`
pub fn launcher_root() -> PathBuf {
    if let Some(p) = env_path("ARTCRAFT_LAUNCHER_HOME") {
        return p;
    }
    match Platform::host() {
        Platform::Macos => data_home().join("ArtCraft Studio"),
        Platform::Windows => data_home().join("ArtCraft"),
        _ => data_home().join("artcraft-launcher"),
    }
}

pub fn config_file() -> PathBuf {
    launcher_root().join("config.json")
}

pub fn state_dir() -> PathBuf {
    launcher_root().join("state")
}

pub fn apps_meta_dir() -> PathBuf {
    state_dir().join("apps")
}

pub fn apps_meta_file(slug: &str) -> PathBuf {
    apps_meta_dir().join(format!("{slug}.json"))
}

/// Staged downloads, including in-flight `.part` files.
pub fn downloads_dir() -> PathBuf {
    launcher_root().join("downloads")
}

pub fn staging_dir() -> PathBuf {
    launcher_root().join("staging")
}

/// Extracted, ready-to-run versions: `.../apps/<slug>/<version>/`
pub fn install_root() -> PathBuf {
    launcher_root().join("apps")
}

pub fn app_install_dir(slug: &str, version: &str) -> PathBuf {
    install_root().join(slug).join(version)
}

/// Backups of per-app data: `.../backups/<slug>/<timestamp>/`
pub fn backups_root() -> PathBuf {
    launcher_root().join("backups")
}

pub fn backup_dir(slug: &str, stamp: &str) -> PathBuf {
    backups_root().join(slug).join(stamp)
}

pub fn telemetry_dir() -> PathBuf {
    state_dir().join("telemetry")
}

/// Undelivered crash reports, spooled as `.jsonl`.
pub fn spool_dir() -> PathBuf {
    telemetry_dir().join("spool")
}

pub fn telemetry_log() -> PathBuf {
    telemetry_dir().join("tracer.jsonl")
}

pub fn exports_dir() -> PathBuf {
    launcher_root().join("exports")
}

// ------------------------------------------------------------------ per-app

/// The app's own data dir, following `dirs::data_dir()` exactly.
pub fn app_data_dir(slug: &str) -> PathBuf {
    if let Ok(v) = std::env::var(format!("{}_DATA_DIR", slug.to_ascii_uppercase())) {
        if !v.is_empty() {
            return PathBuf::from(v);
        }
    }
    data_home().join(slug)
}

/// The app's own log file: `<data>/logs/<slug>.log`
pub fn app_log_file(slug: &str) -> PathBuf {
    if let Ok(v) = std::env::var(format!("{}_LOG_DIR", slug.to_ascii_uppercase())) {
        if !v.is_empty() {
            return PathBuf::from(v).join(format!("{slug}.log"));
        }
    }
    app_data_dir(slug).join("logs").join(format!("{slug}.log"))
}

pub fn app_config_dir(slug: &str) -> PathBuf {
    config_home().join(slug)
}

pub fn app_cache_dir(slug: &str) -> PathBuf {
    cache_home().join(slug)
}

/// Size of a directory tree, in bytes. Follows symlinks only one level to
/// avoid loops; used purely for display so it must never fail the caller.
pub fn dir_size(p: &Path) -> u64 {
    let mut total = 0u64;
    let mut stack = vec![p.to_path_buf()];
    let mut seen = 0;
    while let Some(cur) = stack.pop() {
        seen += 1;
        if seen > 200_000 {
            break;
        }
        let rd = match std::fs::read_dir(&cur) {
            Ok(r) => r,
            Err(_) => continue,
        };
        for e in rd.flatten() {
            let md = match e.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            if md.is_dir() {
                stack.push(e.path());
            } else if md.is_file() {
                total = total.saturating_add(md.len());
            }
        }
    }
    total
}

// ------------------------------------------------------------ OS integration

/// `~/.local/share/icons/hicolor/<size>/apps/`
pub fn linux_icon_dir(size: u32) -> PathBuf {
    data_home().join("icons").join("hicolor").join(format!("{size}x{size}")).join("apps")
}

pub fn linux_scalable_icon_dir() -> PathBuf {
    data_home().join("icons").join("hicolor").join("scalable").join("apps")
}

/// `~/.local/share/applications`
pub fn linux_applications_dir() -> PathBuf {
    data_home().join("applications")
}

/// Nautilus/Dolphin/KDE right-click actions live alongside the desktop file.
pub fn linux_actions_dir() -> PathBuf {
    linux_applications_dir()
}

pub fn linux_mimeapps_list() -> PathBuf {
    config_home().join("mimeapps.list")
}

/// `~/Applications/ArtCraft`
pub fn mac_apps_dir() -> PathBuf {
    if let Some(p) = env_path("ARTCRAFT_MAC_APPS") {
        return p;
    }
    home().join("Applications").join("ArtCraft")
}

pub fn mac_app_bundle(slug: &str) -> PathBuf {
    let a = crate::apps::app(slug).expect("unknown slug");
    mac_apps_dir().join(format!("{}.app", a.bundle))
}

/// The launcher's own reserved-icon store, valid on every platform:
/// `launcher_home()`/icons. Each app reserves its `.ico`, `.icns` and a `-256.png`
/// here, which is then installed into the platform's icon locations.
pub fn reserved_icon_dir() -> PathBuf {
    if let Some(p) = env_path("ARTCRAFT_ICON_DIR") {
        return p;
    }
    launcher_root().join("icons")
}

/// `%LOCALAPPDATA%\ArtCraft` — Windows only (kept for the Windows integration).
pub fn win_root() -> PathBuf {
    env_path("ARTCRAFT_WIN_ROOT")
        .unwrap_or_else(|| env_path("LOCALAPPDATA").unwrap_or_else(|| home().join("AppData").join("Local")).join("ArtCraft"))
}

/// Start Menu shortcut folder.
pub fn win_start_menu_dir() -> PathBuf {
    env_path("APPDATA").map(|a| a.join("Microsoft").join("Windows").join("Start Menu").join("Programs").join("ArtCraft"))
        .unwrap_or_else(|| home().join("AppData").join("Roaming").join("Microsoft").join("Windows").join("Start Menu").join("Programs").join("ArtCraft"))
}

pub fn win_desktop_dir() -> PathBuf {
    env_path("USERPROFILE").map(|h| h.join("Desktop")).unwrap_or_else(|| home().join("Desktop"))
}

/// Shortcut (.lnk) target on Windows, or the desktop-file Exec on Linux.
pub fn win_app_shortcut(slug: &str) -> PathBuf {
    win_root().join(format!("{slug}.exe"))
}

pub fn prefers_system_packages() -> bool {
    std::env::var("ARTCRAFT_PREFER_SYSTEM_PACKAGES").map(|v| v == "1").unwrap_or(false)
}

pub fn distro() -> Distro {
    Distro::detect()
}

pub fn host_platform() -> Platform {
    Platform::host()
}

/// Where exported settings files land when the user hits "Export".
pub fn default_export_path() -> PathBuf {
    exports_dir().join("artcraft-settings.json")
}
