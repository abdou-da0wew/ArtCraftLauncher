//! macOS integration.
//!
//! * **Reserved icon** — the `.app` bundle ships its own `AppIcon.icns`; we
//!   copy it to `~/Library/Application Support/ArtCraft Studio/icons/<slug>.icns`
//!   so the launcher owns a stable, version-independent copy, and the
//!   launcher's own bundle gets `Assets.car`/`AppIcon.icns` from it.
//! * **App bundle** — apps are installed into `~/Applications/ArtCraft`, which
//!   is auto-created. The previous version is parked as
//!   `<Bundle>-<version>.app` so a rollback is a rename, not a re-download.
//! * **Launch Services** — `lsregister` so "Open with" and the Dock see the
//!   app immediately, plus a `Info.plist` for the launcher's own `.app`
//!   wrapper with `CFBundleIconFile` and the app's document types.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::apps::CraftApp;
use crate::paths;

pub fn is_macos() -> bool {
    cfg!(target_os = "macos")
}

/// Copy the app's `.icns` out of its bundle into the launcher's icon store.
pub fn reserve_icon(app: &CraftApp, version: &str) -> Result<PathBuf, String> {
    let bundle = paths::mac_app_bundle(app.slug);
    let resources = bundle.join("Contents/Resources");
    let src = find_icns(&resources);
    let dst = paths::reserved_icon_dir()
        .join(format!("{}.icns", app.slug));
    if let Some(p) = dst.parent() {
        std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
    }
    match src {
        Some(p) => {
            std::fs::copy(&p, &dst).map_err(|e| e.to_string())?;
            Ok(dst)
        }
        None => Err(format!(
            "no .icns in {} for {bundle:?} (v{version})",
            resources.display()
        )),
    }
}

fn find_icns(dir: &Path) -> Option<PathBuf> {
    let rd = std::fs::read_dir(dir).ok()?;
    let mut found = None;
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) == Some("icns") {
            // Prefer a name containing "appicon", else take the first.
            let name = p.file_name().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
            if name.contains("appicon") || found.is_none() {
                found = Some(p.clone());
            }
        }
    }
    found
}

/// Install an app bundle into `~/Applications/ArtCraft/<Bundle>.app`,
/// parking the previous version aside for rollback.
pub fn install_bundle(
    dmg: &Path,
    app: &CraftApp,
    version: &str,
) -> Result<PathBuf, String> {
    let dest = paths::mac_apps_dir().join(format!("{}.app", app.bundle));
    if let Some(p) = dest.parent() {
        std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
    }
    // Mount the dmg read-only.
    let mount = mount_dmg(dmg)?;
    let res = (|| {
        let vol = mount.clone();
        // The volume name is "<Bundle> <version>".
        let src = vol.join(format!("{} {}", app.bundle, version)).join(format!("{}.app", app.bundle));
        let src = if src.exists() {
            src
        } else {
            find_app(&vol, app.bundle).ok_or("no .app inside the disk image")?
        };
        if dest.exists() {
            let parked = paths::mac_apps_dir().join(format!(
                "{}-{}.app",
                app.bundle, version
            ));
            let _ = std::fs::remove_dir_all(&parked);
            std::fs::rename(&dest, &parked).map_err(|e| e.to_string())?;
        }
        copy_dir_recursive(&src, &dest)?;
        Ok(dest)
    })();
    let _ = unmount_dmg(&mount);
    res
}

fn mount_dmg(dmg: &Path) -> Result<PathBuf, String> {
    let out = Command::new("hdiutil")
        .arg("attach")
        .arg("-nobrowse")
        .arg("-readonly")
        .arg("-noverify")
        .arg("-noautoopen")
        .arg(dmg)
        .output()
        .map_err(|e| format!("hdiutil: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "hdiutil attach failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let s = String::from_utf8_lossy(&out.stdout);
    // "…/dev/disk4s1  GUID_partition_scheme  /Volumes/<name>"
    let vol = s
        .lines()
        .filter_map(|l| l.split('\t').nth(2))
        .find(|p| p.starts_with("/Volumes/"))
        .map(|p| PathBuf::from(p.trim()))
        .ok_or_else(|| "could not find the mounted volume".to_string())?;
    Ok(vol)
}

fn unmount_dmg(vol: &Path) -> Result<(), String> {
    let out = Command::new("hdiutil")
        .arg("detach")
        .arg(vol)
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        let _ = Command::new("hdiutil").arg("detach").arg("-force").arg(vol).output();
        Ok(())
    }
}

fn find_app(vol: &Path, bundle: &str) -> Option<PathBuf> {
    let direct = vol.join(format!("{bundle}.app"));
    if direct.exists() {
        return Some(direct);
    }
    let rd = std::fs::read_dir(vol).ok()?;
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            let inner = p.join(format!("{bundle}.app"));
            if inner.exists() {
                return Some(inner);
            }
        }
    }
    None
}

/// Copy a directory tree, preserving symlinks.
pub fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    let md = std::fs::symlink_metadata(src).map_err(|e| e.to_string())?;
    if md.file_type().is_symlink() {
        let target = std::fs::read_link(src).map_err(|e| e.to_string())?;
        if dst.exists() {
            let _ = std::fs::remove_dir_all(dst);
        }
        std::os::unix::fs::symlink(&target, dst).map_err(|e| e.to_string())?;
        return Ok(());
    }
    std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for e in std::fs::read_dir(src).map_err(|e| e.to_string())?.flatten() {
        let to = dst.join(e.file_name());
        copy_dir_recursive(&e.path(), &to)?;
    }
    Ok(())
}

/// Register with Launch Services so "Open with" and the Dock update now.
pub fn launch_services_register(app: &CraftApp) -> Result<(), String> {
    let bundle = paths::mac_app_bundle(app.slug);
    launch_services_register_launcher(&bundle)
}

/// The launcher's own `.app` wrapper, written on demand so the Dock icon is
/// reserved even when the GUI is launched from a bare binary.
pub fn write_launcher_bundle(exe: &Path) -> Result<PathBuf, String> {
    let dir = paths::mac_apps_dir().join("ArtCraft Studio.app");
    let contents = dir.join("Contents");
    let macos = contents.join("MacOS");
    let resources = contents.join("Resources");
    std::fs::create_dir_all(&macos).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&resources).map_err(|e| e.to_string())?;

    // The wrapper exec just re-execs the real binary.
    let sh = macos.join("ArtCraft Studio");
    std::fs::write(
        &sh,
        format!("#!/bin/sh\nexec '{}' \"$@\"\n", exe.to_string_lossy()),
    )
    .map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&sh, std::fs::Permissions::from_mode(0o755));
    }

    let icns = resources.join("AppIcon.icns");
    if !icns.exists() {
        let (rgba, w, h) = crate::osint::launcher_icon_source().owned();
        let mut imgs: Vec<(u32, Vec<u8>)> = Vec::new();
        for s in [16u32, 32, 64, 128, 256, 512] {
            let img = crate::osint::IconSource::from_rgba(rgba.clone(), w, h).resized(s, s);
            imgs.push((s, crate::osint::icons::encode_png(&img.rgba, s, s)));
        }
        std::fs::write(&icns, crate::osint::icons::icns_bytes(&imgs))
            .map_err(|e| e.to_string())?;
    }

    let plist = contents.join("Info.plist");
    let apps: Vec<String> = crate::apps::APPS
        .iter()
        .map(|a| {
            format!(
                "\t\t<dict><key>CFBundleTypeName</key><string>{}</string>\
                 <key>CFBundleTypeRole</key><string>Editor</string>\
                 <key>LSItemContentTypes</key><array>{}</array></dict>",
                a.category,
                a.mime
                    .iter()
                    .map(|m| format!("<string>{m}</string>"))
                    .collect::<Vec<_>>()
                    .join("")
            )
        })
        .collect();
    let plist_s = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleName</key><string>ArtCraft Studio</string>
	<key>CFBundleDisplayName</key><string>ArtCraft Studio</string>
	<key>CFBundleIdentifier</key><string>{}</string>
	<key>CFBundleVersion</key><string>{}</string>
	<key>CFBundleShortVersionString</key><string>{}</string>
	<key>CFBundleExecutable</key><string>ArtCraft Studio</string>
	<key>CFBundleIconFile</key><string>AppIcon.icns</string>
	<key>CFBundlePackageType</key><string>APPL</string>
	<key>LSMinimumSystemVersion</key><string>11.0</string>
	<key>NSHighResolutionCapable</key><true/>
	<key>NSSupportsAutomaticGraphicsSwitching</key><true/>
	<key>LSApplicationCategoryType</key><string>public.app-category.graphics-design</string>
	<key>CFBundleDocumentTypes</key>
	<array>
{}
	</array>
</dict>
</plist>
"#,
        crate::paths::LAUNCHER_ID,
        env!("CARGO_PKG_VERSION"),
        env!("CARGO_PKG_VERSION"),
        apps.join("\n")
    );
    std::fs::write(&plist, plist_s).map_err(|e| e.to_string())?;

    // Ad-hoc sign so Gatekeeper treats it as a coherent bundle.
    let out = Command::new("codesign")
        .arg("--force")
        .arg("--deep")
        .arg("-s")
        .arg("-")
        .arg(&dir)
        .output();
    let _ = out;

    Ok(dir)
}

/// Register the launcher's own `.app` wrapper with Launch Services.
pub fn launch_services_register_launcher(bundle: &Path) -> Result<(), String> {
    if !bundle.exists() {
        return Ok(());
    }
    let lsregister = "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister";
    if !Path::new(lsregister).exists() {
        return Ok(());
    }
    let out = Command::new(lsregister).arg("-f").arg(bundle).output();
    // `lsregister` is unsupported API; a non-zero exit is not an error.
    let _ = out;
    Ok(())
}

/// Per-app document-type entries, for the launcher's Info.plist.
pub fn doc_types() -> Vec<(String, Vec<String>)> {
    crate::apps::APPS
        .iter()
        .map(|a| {
            (
                a.category.to_string(),
                a.mime.iter().map(|s| s.to_string()).collect(),
            )
        })
        .collect()
}

pub fn remove(_app: &CraftApp) -> Result<(), String> {
    Ok(())
}
