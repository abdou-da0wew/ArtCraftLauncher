//! Windows integration.
//!
//! * **Reserved icon** — a multi-size `.ico` per app laid into
//!   `%LOCALAPPDATA%\ArtCraft\icons\<slug>.ico`, plus one for the launcher
//!   itself baked into the `.exe` at build time (see `build.rs`).
//! * **Start Menu shortcuts** — a `.lnk` per app with `IconLocation` pointed
//!   at the reserved `.ico`, so the pinned/taskbar icon is the app's own art.
//! * **App Paths** — `HKLM`/`HKCU\Software\Microsoft\Windows\CurrentVersion\App Paths`
//!   so `ArtCraft` apps resolve from the Run dialog and `start`.
//! * **File associations** — `HKCU\...\OpenWithProgids` and `Applications\...`
//!   so Explorer's "Open with" and the right-click menu offer the app.
//!
//! Registry writes go through `reg.exe` rather than an FFI binding: they are
//! rare, user-visible, and this keeps the launcher free of `winapi`, which
//! would otherwise dominate the dependency tree.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::apps::{app_id, CraftApp};
use crate::paths;

pub fn is_windows() -> bool {
    cfg!(target_os = "windows")
}

/// Write the reserved `.ico` for `app` into the launcher's icon store.
pub fn install_icon(app: &CraftApp, version: &str) -> Result<Vec<PathBuf>, String> {
    // Done by `crate::osint::reserve_icon`, which already wrote
    // `%LOCALAPPDATA%\ArtCraft\icons\<slug>.ico`. Copy it next to the install
    // too, so a portable install carries its own icon.
    let src = paths::reserved_icon_dir().join(format!("{}.ico", app.slug));
    if !src.exists() {
        return Err(format!("{} not reserved yet", src.display()));
    }
    let dst = paths::app_install_dir(app.slug, version).join(format!("{}.ico", app.slug));
    if let Some(p) = dst.parent() {
        std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
    }
    std::fs::copy(&src, &dst).map_err(|e| e.to_string())?;
    Ok(vec![dst])
}

/// Create a Start Menu shortcut for one app, pointing at the reserved icon.
pub fn start_menu_shortcut(app: &CraftApp) -> Result<Vec<PathBuf>, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let ico = paths::reserved_icon_dir().join(format!("{}.ico", app.slug));
    let lnk = paths::win_start_menu_dir().join(format!("{}.lnk", app.name));
    let ps = format!(
        "$s=(New-Object -COM WScript.Shell).CreateShortcut('{}'); \
         $s.TargetPath='{}'; \
         $s.Arguments='open {}'; \
         $s.WorkingDirectory='{}'; \
         $s.Description='{}'; \
         $s.IconLocation='{},0'; \
         $s.Save()",
        ps_quote(&lnk),
        ps_quote(&exe),
        app.slug,
        ps_quote(&paths::app_install_dir(app.slug, &version_of(app))),
        ps_quote_str(app.pitch),
        ps_quote(&ico),
    );
    run_powershell(&ps)?;
    Ok(vec![lnk])
}

/// The launcher's own Start Menu shortcut.
pub fn launcher_shortcut(launcher_exe: &Path) -> Result<Vec<PathBuf>, String> {
    let ico = paths::reserved_icon_dir().join("artcraft-launcher.ico");
    let lnk = paths::win_start_menu_dir().join("ArtCraft Studio.lnk");
    let ps = format!(
        "$s=(New-Object -COM WScript.Shell).CreateShortcut('{}'); \
         $s.TargetPath='{}'; \
         $s.Description='Launcher and manager for the ArtCraft apps'; \
         $s.IconLocation='{},0'; \
         $s.Save()",
        ps_quote(&lnk),
        ps_quote(launcher_exe),
        ps_quote(&ico),
    );
    run_powershell(&ps)?;
    Ok(vec![lnk])
}

/// Write the launcher's own reserved `.ico` into the icon store.
pub fn install_launcher_icon() -> Result<Vec<PathBuf>, String> {
    // Already written by `crate::osint::register_launcher`; just copy it next
    // to the launcher's install so it is discoverable.
    let src = crate::paths::reserved_icon_dir().join("artcraft-launcher.ico");
    if !src.exists() {
        return Err("launcher icon not reserved yet".into());
    }
    let dst = crate::paths::reserved_icon_dir().join("artcraft-launcher.ico");
    std::fs::copy(&src, &dst).map_err(|e| e.to_string())?;
    Ok(vec![dst])
}

fn version_of(app: &CraftApp) -> String {
    // The newest installed version, or the pinned one.
    let v = crate::installer::installed_versions(app.slug);
    v.into_iter().next().unwrap_or_else(|| app.pinned.to_string())
}

/// `App Paths` so the apps resolve by name from Run/`start`.
pub fn register_app_paths(app: &CraftApp) -> Result<(), String> {
    let key = format!(
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\App Paths\{}.exe",
        app.slug
    );
    let exe = portable_exe(app);
    reg_add(&key, None, &exe.to_string_lossy())?;
    reg_add(&key, Some("Path"), &paths::app_install_dir(app.slug, &version_of(app)).to_string_lossy())?;
    Ok(())
}

fn portable_exe(app: &CraftApp) -> PathBuf {
    let root = paths::app_install_dir(app.slug, &version_of(app));
    for c in [
        root.join("bin").join(format!("{}", app.slug)).with_extension("exe"),
        root.join(format!("{}.exe", app.slug)),
    ] {
        if c.exists() {
            return c;
        }
    }
    root.join("bin").join(format!("{}.exe", app.slug))
}

/// File associations + Explorer "Open with" entries.
pub fn register_associations(app: &CraftApp) -> Result<Vec<String>, String> {
    let mut notes = Vec::new();
    let id = app_id(app.slug);
    let progid = format!("ArtCraft.{}", app.slug);
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let cmd = format!(
        "\"{}\" open {} -- \"%1\"",
        exe.to_string_lossy(),
        app.slug
    );

    for ext in app.open_exts {
        // Per-extension ProgID under HKCU, and an OpenWithProgids entry.
        let prog_key = format!(r"HKCU\Software\Classes\{ext}\OpenWithProgids");
        reg_add(&prog_key, Some(&progid), "0")?;
        let cmd_key = format!(r"HKCU\Software\Classes\{progid}\shell\open\command");
        reg_add(&cmd_key, None, &cmd)?;
        let open_key = format!(r"HKCU\Software\Classes\{progid}\shell\open");
        reg_add(
            &open_key,
            Some("Icon"),
            &paths::reserved_icon_dir()
                .join(format!("{}.ico", app.slug))
                .to_string_lossy(),
        )?;
        notes.push(format!(".{ext} → {progid}"));
    }

    // Applications\<launcher>.exe\SupportedTypes so the picker lists the app.
    let app_key = format!(
        r"HKCU\Software\Classes\Applications\{}.exe\SupportedTypes",
        exe.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
    );
    for ext in app.open_exts {
        reg_add(&app_key, Some(&format!(".{ext}")), "")?;
    }
    let _ = id;
    Ok(notes)
}

/// Undo the Windows integration for one app.
pub fn remove(app: &CraftApp) -> Result<(), String> {
    let _ = std::fs::remove_file(
        paths::win_start_menu_dir().join(format!("{}.lnk", app.name)),
    );
    let progid = format!("ArtCraft.{}", app.slug);
    reg_delete(&format!(r"HKCU\Software\Classes\{progid}"))?;
    for ext in app.open_exts {
        let _ = reg_delete(&format!(r"HKCU\Software\Classes\{ext}\OpenWithProgids"));
    }
    Ok(())
}

// ------------------------------------------------------------- reg.exe glue

fn reg_add(key: &str, value: Option<&str>, data: &str) -> Result<(), String> {
    let mut c = Command::new("reg");
    c.arg("add").arg(key);
    if let Some(v) = value {
        c.arg("/v").arg(v).arg("/d").arg(data);
    } else {
        c.arg("/ve").arg("/d").arg(data);
    }
    c.arg("/f");
    let out = c.output().map_err(|e| format!("reg.exe: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "reg add {key} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

fn reg_delete(key: &str) -> Result<(), String> {
    let out = Command::new("reg")
        .arg("delete")
        .arg(key)
        .arg("/f")
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        // Not existing is fine.
        Ok(())
    }
}

fn run_powershell(script: &str) -> Result<(), String> {
    let out = Command::new("powershell")
        .arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-WindowStyle")
        .arg("Hidden")
        .arg("-Command")
        .arg(script)
        .output()
        .map_err(|e| format!("powershell: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "powershell failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

fn ps_quote(p: &std::path::Path) -> String {
    format!("'{}'", p.to_string_lossy().replace('\'', "''"))
}

fn ps_quote_str(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}
