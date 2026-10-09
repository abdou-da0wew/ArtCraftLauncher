//! Linux desktop integration.
//!
//! 1. A `.desktop` entry per app in `~/.local/share/applications`
//! 2. The vendor's hicolor icons copied into `~/.local/share/icons`
//! 3. A `mimeinfo.cache` / `mimeapps.list` association per app
//! 4. **Right-click extras**: `Actions=` entries in the desktop file plus a
//!    file-manager action script, so Nautilus, Dolphin, Nemo, Caja, Thunar and
//!    the KDE file dialog all offer "Open with <App>" and, for the formats an
//!    app understands, "Send to <App>".

use std::path::{Path, PathBuf};

use crate::apps::{app_id, CraftApp};
use crate::config::Settings;
use crate::osint::icons;
use crate::paths;

/// The `.desktop` file for `app`.
pub fn desktop_entry(app: &CraftApp, launcher_exe: Option<&Path>) -> Result<PathBuf, String> {
    let exe = launcher_exe
        .map(Path::to_path_buf)
        .unwrap_or_else(|| std::env::current_exe().unwrap_or_else(|_| PathBuf::from("artcraft-launcher")));
    let path = paths::linux_applications_dir().join(format!("{}.desktop", app.slug));

    let exec = format!(
        "{} open {} --",
        shell_quote(&exe),
        app.slug
    );
    let mimes = app.mime.join(";");
    let cats = categories(app);

    // `Actions=` gives every file manager a right-click submenu for this app.
    let action_ids: Vec<(&str, String)> = vec![
        ("OpenWith", format!("Open with {}", app.name)),
        ("OpenWithNew", format!("Open in a new {} window", app.name)),
        ("ConvertWith", format!("Send to {}", app.name)),
    ];
    let actions: String = action_ids
        .iter()
        .map(|(id, _)| format!("{id};"))
        .collect();

    let mut s = String::new();
    s.push_str("[Desktop Entry]\n");
    s.push_str("Type=Application\n");
    s.push_str(&format!("Name={}\n", app.name));
    s.push_str(&format!("GenericName={}\n", app.category));
    s.push_str(&format!("Comment={}\n", app.pitch));
    s.push_str(&format!("Exec={exec}\n"));
    s.push_str(&format!("Icon={}\n", app_id(app.slug)));
    s.push_str(&format!("Terminal=false\n"));
    s.push_str(&format!("StartupWMClass={}\n", app.bundle));
    s.push_str(&format!("Categories={cats}\n"));
    s.push_str(&format!("MimeType={mimes}\n"));
    s.push_str(&format!("Keywords={};{};artcraft;rust;open source;\n", app.slug, app.category));
    s.push_str(&format!("Actions={actions}\n"));
    s.push_str(&format!("X-ArtCraft-App={}\n", app.slug));
    s.push_str("X-ArtCraft-Launcher=true\n");
    s.push('\n');

    for (id, name) in &action_ids {
        s.push_str(&format!("[Desktop Action {id}]\n"));
        s.push_str(&format!("Name={name}\n"));
        s.push_str(&format!("Icon={}\n", app_id(app.slug)));
        match *id {
            "OpenWith" => {
                s.push_str(&format!("Exec={} open {} -- %F\n", shell_quote(&exe), app.slug));
            }
            "OpenWithNew" => {
                s.push_str(&format!(
                    "Exec={} open {} --new -- %F\n",
                    shell_quote(&exe),
                    app.slug
                ));
            }
            "ConvertWith" => {
                s.push_str(&format!(
                    "Exec={} open {} --send-to -- %F\n",
                    shell_quote(&exe),
                    app.slug
                ));
            }
            _ => {}
        }
        s.push('\n');
    }

    let _p = paths::linux_applications_dir();
    icons::write_file(&path, s.as_bytes(), true)?;

    Ok(path)
}

fn categories(app: &CraftApp) -> &'static str {
    match app.slug {
        "photocraft" => "Graphics;2DGraphics;RasterGraphics;",
        "vectorcraft" => "Graphics;2DGraphics;VectorGraphics;",
        "filmcraft" => "AudioVideo;Video;Editor;",
        "lightcraft" => "Graphics;Photography;RasterGraphics;",
        "pdfcraft" => "Office;Viewer;",
        "effectcraft" => "AudioVideo;Video;Editor;Graphics;",
        "designcraft" => "Office;Publishing;Graphics;",
        _ => "Graphics;",
    }
}

/// The launcher's own `.desktop` entry, so "ArtCraft" itself is searchable
/// and carries its own right-click actions.
pub fn launcher_desktop_entry(exe: &Path) -> Result<PathBuf, String> {
    let path = paths::linux_applications_dir().join("artcraft-launcher.desktop");
    let ls = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=ArtCraft Studio\n\
         GenericName=App launcher\n\
         Comment=Launcher and manager for the ArtCraft apps\n\
         Exec={} \n\
         Icon=artcraft-launcher\n\
         Terminal=false\n\
         StartupWMClass=ArtCraft\n\
         Categories=Utility;Development;Graphics;\n\
         Keywords=artcraft;launcher;photocraft;vectorcraft;filmcraft;lightcraft;pdfcraft;effectcraft;designcraft;\n\
         Actions=UpdateAll;Reports;RegisterAll;\n\
         X-ArtCraft-Launcher=true\n\
         \n\
         [Desktop Action UpdateAll]\n\
         Name=Update all apps\n\
         Icon=artcraft-launcher\n\
         Exec={} update --all --yes\n\
         \n\
         [Desktop Action Reports]\n\
         Name=Crash reports\n\
         Icon=artcraft-launcher\n\
         Exec={} telemetry list\n\
         \n\
         [Desktop Action RegisterAll]\n\
         Name=Register every app with the system\n\
         Icon=artcraft-launcher\n\
         Exec={} register-all\n",
        shell_quote(exe),
        shell_quote(exe),
        shell_quote(exe),
        shell_quote(exe)
    );
    icons::write_file(&path, ls.as_bytes(), true)?;
    Ok(path)
}

/// The launcher's own icons at every hicolor size.
pub fn install_launcher_icons() -> Result<Vec<PathBuf>, String> {
    let (rgba, w, h) = crate::logo::mark_rgba(256).ok_or("logo")?;
    let mut out = Vec::new();
    for size in [16u32, 24, 32, 48, 64, 128, 256] {
        let dst = paths::linux_icon_dir(size).join("artcraft-launcher.png");
        if let Some(p) = dst.parent() {
            std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        // The mark's aspect ratio is fixed, so height follows the width.
        let nh = (size as f32 * (crate::logo::MARK_H / crate::logo::MARK_W)).round() as u32;
        let src = crate::osint::IconSource::from_rgba(rgba.clone(), w, h);
        let img = src.resized(size, nh.max(1));
        std::fs::write(&dst, icons::encode_png(&img.rgba, size, nh.max(1)))
            .map_err(|e| e.to_string())?;
        out.push(dst);
    }
    Ok(out)
}

/// File-manager and KDE actions that launch the launcher itself.
pub fn launcher_context_actions(exe: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    let fm = paths::linux_applications_dir().join("artcraft-file-manager");
    let f = fm.join("artcraft-launcher-open.desktop");
    let s = format!(
        "[Desktop Entry]\n\
         Type=Action\n\
         Name=Open in ArtCraft Studio\n\
         Icon=artcraft-launcher\n\
         Exec={} open -- %f\n\
         X-Nemo-SingleFile=true\n\
         X-Action-Extensions=psd;png;jpg;svg;pdf;mp4;dng;\n",
        shell_quote(exe)
    );
    icons::write_file(&f, s.as_bytes(), false)?;
    out.push(f);

    let kdir = paths::state_home().join("kio").join("servicemenus");
    let kf = kdir.join("artcraft-launcher.desktop");
    let k = format!(
        "[Desktop Entry]\n\
         Type=Service\n\
         Name=Open in ArtCraft Studio\n\
         ServiceTypes=KonqPopupMenu/Plugin\n\
         Exec={} open -- %f\n\
         Icon=artcraft-launcher\n",
        shell_quote(exe)
    );
    icons::write_file(&kf, k.as_bytes(), false)?;
    out.push(kf);
    Ok(out)
}

/// Install the app's icons into the icon theme, one file per size.
pub fn install_icons(app: &CraftApp, version: &str) -> Result<Vec<PathBuf>, String> {
    let id = app_id(app.slug);
    let root = crate::paths::app_install_dir(app.slug, version);
    let mut out = Vec::new();

    // 1. Scalable SVG, if the release shipped one.
    let svg = root
        .join("share/icons/hicolor/scalable/apps")
        .join(format!("{id}.svg"));
    if svg.exists() {
        let dst = paths::linux_scalable_icon_dir().join(format!("{id}.svg"));
        if let Some(p) = dst.parent() {
            std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        std::fs::copy(&svg, &dst).map_err(|e| e.to_string())?;
        out.push(dst);
    }

    // 2. Raster sizes that actually exist in the release.
    for size in [16u32, 24, 32, 48, 64, 128, 256, 512] {
        let src = root
            .join("share/icons/hicolor")
            .join(format!("{size}x{size}/apps"))
            .join(format!("{id}.png"));
        if !src.exists() {
            continue;
        }
        let dst = paths::linux_icon_dir(size).join(format!("{id}.png"));
        if let Some(p) = dst.parent() {
            std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        std::fs::copy(&src, &dst).map_err(|e| e.to_string())?;
        out.push(dst);
    }

    // 3. The reserved copy the launcher made, as a guaranteed fallback.
    let reserved = paths::reserved_icon_dir().join(format!("{}-256.png", app.slug));
    if reserved.exists() {
        let dst = paths::linux_icon_dir(256).join(format!("{id}.png"));
        if let Some(p) = dst.parent() {
            std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        if !dst.exists() {
            std::fs::copy(&reserved, &dst).map_err(|e| e.to_string())?;
            out.push(dst);
        }
    }

    // The launcher's own icon.
    let (rgba, w, h) = crate::logo::mark_rgba(256).unwrap_or((vec![], 1, 1));
    for size in [16u32, 24, 32, 48, 64, 128, 256] {
        let dst = paths::linux_icon_dir(size).join("artcraft-launcher.png");
        if let Some(p) = dst.parent() {
            std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        let src = crate::osint::IconSource::from_rgba(rgba.clone(), w, h);
        let img = src.resized(size, size);
        let png = icons::encode_png(&img.rgba, size, size);
        std::fs::write(&dst, png).map_err(|e| e.to_string())?;
        out.push(dst);
    }

    Ok(out)
}

/// Write `mimeapps.list` associations for every MIME type the app handles.
pub fn mime_packages(app: &CraftApp) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    let id = app_id(app.slug);

    // Per-app defaults, so the association is removable per app.
    let per_app = paths::linux_applications_dir().join(format!("{}-mimeapps.list", app.slug));
    let mut s = String::from("[Default Applications]\n");
    for m in app.mime {
        s.push_str(&format!("{m}={id}.desktop;\n"));
    }
    s.push_str("\n[Added Associations]\n");
    for m in app.mime {
        s.push_str(&format!("{m}={id}.desktop;\n"));
    }
    icons::write_file(&per_app, s.as_bytes(), false)?;
    out.push(per_app);

    // Merge into the user's mimeapps.list, preserving unknown keys.
    let list = paths::linux_mimeapps_list();
    let existing = std::fs::read_to_string(&list).unwrap_or_default();
    let merged = rewrite_defaults(&existing, app.slug, app.mime);
    icons::write_file(&list, merged.as_bytes(), false)?;
    out.push(list);

    // mimeinfo.cache is what the "Open With" dialog actually reads.
    let cache = paths::linux_applications_dir().join("mimeinfo.cache");
    let mut cm = std::fs::read_to_string(&cache).unwrap_or_default();
    cm = rewrite_mimeinfo(&cm, &format!("{id}.desktop"), app.mime);
    icons::write_file(&cache, cm.as_bytes(), false)?;
    out.push(cache);

    Ok(out)
}

fn rewrite_defaults(existing: &str, slug: &str, mimes: &[&str]) -> String {
    let id = app_id(slug);
    let mut lines: Vec<String> = existing.lines().map(|l| l.to_string()).collect();
    let mut in_defaults = false;
    let mut seen: Vec<String> = Vec::new();
    for l in lines.iter_mut() {
        let t = l.trim();
        if t.starts_with('[') {
            in_defaults = t.eq_ignore_ascii_case("[Default Applications]");
            continue;
        }
        if !in_defaults {
            continue;
        }
        if let Some((k, _)) = t.split_once('=') {
            if mimes.contains(&k) {
                seen.push(k.to_string());
                *l = format!("{k}={id}.desktop;");
            }
        }
    }
    if !in_defaults || !existing.contains("[Default Applications]") {
        lines.push(String::new());
        lines.push("[Default Applications]".into());
    }
    for m in mimes {
        if !seen.iter().any(|s| s == m) {
            lines.push(format!("{m}={id}.desktop;"));
        }
    }
    let mut out = lines.join("\n");
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn rewrite_mimeinfo(existing: &str, desktop: &str, mimes: &[&str]) -> String {
    let mut out = String::from("[MIME Cache]\n");
    let mut map: Vec<(String, Vec<String>)> = Vec::new();
    // parse existing
    for l in existing.lines() {
        if let Some((k, v)) = l.split_once('=') {
            if k.eq_ignore_ascii_case("mime cache") {
                continue;
            }
            let entries: Vec<String> = v
                .split(';')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect();
            map.push((k.to_string(), entries));
        }
    }
    for m in mimes {
        match map.iter_mut().find(|(k, _)| k == m) {
            Some((_, v)) => {
                if !v.iter().any(|x| x == desktop) {
                    v.insert(0, desktop.to_string());
                }
            }
            None => map.push((m.to_string(), vec![desktop.to_string()])),
        }
    }
    map.sort_by(|a, b| a.0.cmp(&b.0));
    for (k, v) in map {
        out.push_str(&format!("{k}={}\n", v.join(";")));
    }
    out
}

/// Install the file-manager action scripts that give right-click entries in
/// Nautilus/Dolphin/Nemo/Caja. The `Actions=` block in the desktop file is the
/// primary mechanism; these scripts cover file managers that only read the
/// `file-manager-actions` schema (Nemo, Caja, older Nautilus) and the KDE
/// service-menu XML.
pub fn context_actions(app: &CraftApp) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("artcraft-launcher"));
    let id = app_id(app.slug);

    // --- Nautilus / Nemo / Caja / Thunar: a .desktop file with
    //     `X-Nemo-...` / `X-Action...` keys and a `file-manager-actions`
    //     schema companion.
    let fm_dir = paths::linux_applications_dir().join("artcraft-file-manager");
    let base = fm_dir.join(format!("{}-open.desktop", app.slug));
    let mut s = String::new();
    s.push_str("[Desktop Entry]\n");
    s.push_str("Type=Action\n");
    s.push_str(&format!("Name={}\n", app.open_verb));
    s.push_str(&format!("Icon={id}\n"));
    s.push_str(&format!(
        "Exec={} open {} -- %F\n",
        shell_quote(&exe),
        app.slug
    ));
    s.push_str(&format!("MimeTypes={};\n", app.mime.join(";")));
    s.push_str("X-Nemo-SingleFile=true\n");
    s.push_str("X-Nemo-ApplyToFiles=true\n");
    s.push_str("X-Action-Extensions=");
    s.push_str(&format!("{};\n", app.open_exts.join(";")));
    s.push_str("X-KDE-ServiceTypes=KonqPopupMenu/Plugin\n");
    icons::write_file(&base, s.as_bytes(), false)?;
    out.push(base);

    // A second action for "send to", shown when multiple files are selected.
    let send = fm_dir.join(format!("{}-send.desktop", app.slug));
    let mut s2 = String::new();
    s2.push_str("[Desktop Entry]\n");
    s2.push_str("Type=Action\n");
    s2.push_str(&format!("Name=Send to {}\n", app.name));
    s2.push_str(&format!("Icon={id}\n"));
    s2.push_str(&format!(
        "Exec={} open {} --send-to -- %F\n",
        shell_quote(&exe),
        app.slug
    ));
    s2.push_str(&format!("MimeTypes={};\n", app.mime.join(";")));
    s2.push_str("X-Nemo-MultipleFiles=true\n");
    icons::write_file(&send, s2.as_bytes(), false)?;
    out.push(send);

    // --- Dolphin / KDE service menu.
    let kdir = paths::state_home().join("kio").join("servicemenus");
    let kfile = kdir.join(format!("artcraft-{}.desktop", app.slug));
    let mut k = String::new();
    k.push_str("[Desktop Entry]\n");
    k.push_str("Type=Service\n");
    k.push_str(&format!("Name={}\n", app.open_verb));
    k.push_str("ServiceTypes=KonqPopupMenu/Plugin\n");
    k.push_str(&format!("MimeTypes={};\n", app.mime.join(";")));
    k.push_str(&format!(
        "Exec={} open {} -- %f\n",
        shell_quote(&exe),
        app.slug
    ));
    k.push_str("Icon=artcraft-launcher\n");
    icons::write_file(&kfile, k.as_bytes(), false)?;
    out.push(kfile);

    Ok(out)
}

/// Undo everything `integrate` wrote for one app.
pub fn remove(app: &CraftApp, settings: &Settings) -> Result<(), String> {
    let _ = settings;
    for p in [
        paths::linux_applications_dir().join(format!("{}.desktop", app.slug)),
        paths::linux_applications_dir().join(format!("{}-mimeapps.list", app.slug)),
        paths::linux_applications_dir()
            .join("artcraft-file-manager")
            .join(format!("{}-open.desktop", app.slug)),
        paths::linux_applications_dir()
            .join("artcraft-file-manager")
            .join(format!("{}-send.desktop", app.slug)),
        paths::state_home()
            .join("kio/servicemenus")
            .join(format!("artcraft-{}.desktop", app.slug)),
    ] {
        let _ = std::fs::remove_file(p);
    }
    let id = app_id(app.slug);
    for size in [16u32, 24, 32, 48, 64, 128, 256, 512] {
        let _ = std::fs::remove_file(
            paths::linux_icon_dir(size).join(format!("{id}.png")),
        );
    }
    let _ = std::fs::remove_file(
        paths::linux_scalable_icon_dir().join(format!("{id}.svg")),
    );
    Ok(())
}

fn shell_quote(p: &Path) -> String {
    let s = p.to_string_lossy().to_string();
    if s.chars().all(|c| c.is_ascii_alphanumeric() || "/._-".contains(c)) {
        s
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}
