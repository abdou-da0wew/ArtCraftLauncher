//! Icon generation for the three platforms, and the OS integration itself.
//!
//! * **Linux** — `.desktop` entries under `~/.local/share/applications`,
//!   icon themes under `~/.local/share/icons/hicolor/...`, MIME associations,
//!   and `Actions=` blocks so the file manager gets right-click entries.
//! * **Windows** — a reserved `.ico` per app (plus one for the launcher itself)
//!   laid into `%LOCALAPPDATA%\ArtCraft\icons`, Start Menu `.lnk` shortcuts,
//!   and `App Paths` + file-association registry entries.
//! * **macOS** — a reserved `.icns` per app, an `Info.plist` for the launcher
//!   bundle, and `lsregister` so Launch Services picks the apps up.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::apps::{app, CraftApp};

pub mod icons;
pub mod linux;
pub mod macos;
pub mod windows;
pub use icons::{
    ico_bytes, icns_bytes,
};

// ------------------------------------------------------------------ icon gen

/// A rasterised icon source: RGBA + size.
pub struct IconSource {
    pub rgba: Vec<u8>,
    pub w: u32,
    pub h: u32,
}

impl IconSource {
    /// Decode a PNG using the vendored `png` crate.
    pub fn from_png(path: &Path) -> Option<IconSource> {
        let f = std::fs::File::open(path).ok()?;
        let mut d = png::Decoder::new(f).read_info().ok()?;
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
        Some(IconSource {
            rgba,
            w: info.width,
            h: info.height,
        })
    }

    /// From a raw RGBA buffer already in memory.
    pub fn from_rgba(rgba: Vec<u8>, w: u32, h: u32) -> Self {
        Self { rgba, w, h }
    }

    /// Owned `(rgba, w, h)` clone, so callers can resize it repeatedly.
    pub fn owned(&self) -> (Vec<u8>, u32, u32) {
        (self.rgba.clone(), self.w, self.h)
    }

    /// Box-filtered resize.
    pub fn resized(&self, nw: u32, nh: u32) -> IconSource {
        let mut out = vec![0u8; (nw * nh * 4) as usize];
        let sx = self.w as f32 / nw as f32;
        let sy = self.h as f32 / nh as f32;
        for y in 0..nh {
            for x in 0..nw {
                let x0 = (x as f32 * sx) as u32;
                let y0 = (y as f32 * sy) as u32;
                let x1 = (((x + 1) as f32 * sx) as u32).min(self.w).max(x0 + 1);
                let y1 = (((y + 1) as f32 * sy) as u32).min(self.h).max(y0 + 1);
                let mut acc = [0f32; 4];
                let mut n = 0f32;
                for yy in y0..y1 {
                    for xx in x0..x1 {
                        let i = ((yy * self.w + xx) * 4) as usize;
                        for c in 0..4 {
                            acc[c] += self.rgba[i + c] as f32;
                        }
                        n += 1.0;
                    }
                }
                let o = ((y * nw + x) * 4) as usize;
                for c in 0..4 {
                    out[o + c] = (acc[c] / n) as u8;
                }
            }
        }
        IconSource {
            rgba: out,
            w: nw,
            h: nh,
        }
    }
}

// ------------------------------------------------------------ integration

/// What happened during an integration run. Kept per-app so the UI can show it.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IntegrationReport {
    pub desktop_entry: Option<PathBuf>,
    pub context_actions: Vec<PathBuf>,
    pub icons: Vec<PathBuf>,
    pub shortcuts: Vec<PathBuf>,
    pub mime_packages: Vec<PathBuf>,
    pub bundle: Option<PathBuf>,
    pub reserved: Option<PathBuf>,
    pub notes: Vec<String>,
}


/// Run every enabled integration for one app. Idempotent.
pub fn integrate(
    a: &CraftApp,
    version: &str,
    settings: &crate::config::Settings,
    launcher_exe: Option<&Path>,
) -> Result<IntegrationReport, String> {
    let mut rep = IntegrationReport::default();

    // Every platform starts by reserving a copy of the app's icon, from the
    // vendor's own artwork inside the install tree when we can get it.
    match reserve_icon(a, version) {
        Ok(p) => rep.reserved = Some(p),
        Err(e) => rep.notes.push(format!("icon: {e}")),
    }

    #[cfg(target_os = "linux")]
    {
        if settings.integrations.linux_desktop_entries {
            let p = crate::osint::linux::desktop_entry(a, launcher_exe)?;
            rep.desktop_entry = Some(p.clone());
            rep.icons.extend(crate::osint::linux::install_icons(a, version)?);
            rep.mime_packages
                .extend(crate::osint::linux::mime_packages(a)?);
            if settings.integrations.linux_context_actions {
                rep.context_actions
                    .extend(crate::osint::linux::context_actions(a)?);
            }
        }
    }
    #[cfg(target_os = "windows")]
    {
        if settings.integrations.windows_icons {
            rep.icons.extend(crate::osint::windows::install_icon(a, version)?);
        }
        if settings.integrations.windows_start_menu {
            rep.shortcuts
                .extend(crate::osint::windows::start_menu_shortcut(a)?);
            if let Some(e) = launcher_exe {
                if settings.integrations.windows_start_menu {
                    rep.shortcuts
                        .extend(crate::osint::windows::launcher_shortcut(e)?);
                }
            }
        }
        if settings.integrations.windows_associations {
            rep.notes
                .extend(crate::osint::windows::register_associations(a)?);
        }
        crate::osint::windows::register_app_paths(a)?;
    }
    #[cfg(target_os = "macos")]
    {
        if settings.integrations.macos_icons {
            rep.bundle = crate::osint::macos::reserve_icon(a, version).ok();
        }
        if settings.integrations.macos_launch_services {
            crate::osint::macos::launch_services_register(a)?;
        }
    }

    Ok(rep)
}

/// Reserved icon formats per platform, newest first.
pub fn reserved_sizes() -> &'static [u32] {
    if cfg!(target_os = "windows") {
        &[256, 64, 48, 32, 24, 16]
    } else if cfg!(target_os = "macos") {
        &[512, 256, 128, 32, 16]
    } else {
        &[256, 128, 64, 48, 32, 24, 16]
    }
}

/// Reserve the app's icon in the launcher's own store, and install it into
/// the platform's icon locations.
fn reserve_icon(a: &CraftApp, version: &str) -> Result<PathBuf, String> {
    // Prefer the vendor's own artwork from the extracted install tree — but
    // only a raster one; the SVG in `share/icons/.../scalable` cannot be
    // decoded by the PNG reader, so it must not shadow the fallback.
    let root = crate::paths::app_install_dir(a.slug, version);
    let src = vendor_icon_png(&root, a.slug)
        .and_then(|p| IconSource::from_png(&p))
        .or_else(|| embedded_icon(a.slug))
        .ok_or_else(|| format!("no icon artwork available for {}", a.slug))?;

    let dir = crate::paths::reserved_icon_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    #[cfg(target_os = "windows")]
    {
        let out = dir.join(format!("{}.ico", a.slug));
        let imgs: Vec<(u32, Vec<u8>)> = reserved_sizes()
            .iter()
            .map(|s| (*s, icons::encode_png(&src.resized(*s, *s).rgba, *s, *s)))
            .collect();
        std::fs::write(&out, ico_bytes(&imgs)).map_err(|e| e.to_string())?;
        return Ok(out);
    }
    #[cfg(target_os = "macos")]
    {
        let out = dir.join(format!("{}.icns", a.slug));
        let imgs: Vec<(u32, Vec<u8>)> = [512u32, 256, 128, 32, 16]
            .iter()
            .map(|s| (*s, icons::encode_png(&src.resized(*s, *s).rgba, *s, *s)))
            .collect();
        std::fs::write(&out, icns_bytes(&imgs)).map_err(|e| e.to_string())?;
        return Ok(out);
    }
    #[allow(unreachable_code)]
    {
        let out = dir.join(format!("{}-256.png", a.slug));
        std::fs::write(&out, icons::encode_png(&src.rgba, src.w, src.h))
            .map_err(|e| e.to_string())?;
        Ok(out)
    }
}

/// The icon artwork the launcher ships itself, as a fallback.
fn embedded_icon(slug: &str) -> Option<IconSource> {
    let bytes: &[u8] = match slug {
        "photocraft" => include_bytes!("../../assets/images/photocraft-icon.png"),
        "vectorcraft" => include_bytes!("../../assets/images/vectorcraft-icon.png"),
        "filmcraft" => include_bytes!("../../assets/images/filmcraft-icon.png"),
        "lightcraft" => include_bytes!("../../assets/images/lightcraft-icon.png"),
        "pdfcraft" => include_bytes!("../../assets/images/pdfcraft-icon.png"),
        "effectcraft" => include_bytes!("../../assets/images/effectcraft-icon.png"),
        "designcraft" => include_bytes!("../../assets/images/designcraft-icon.png"),
        _ => include_bytes!("../../assets/images/pdfcraft-icon.png"),
    };
    let mut d = png::Decoder::new(std::io::Cursor::new(bytes))
        .read_info()
        .ok()?;
    let mut buf = vec![0u8; d.output_buffer_size()];
    let info = d.next_frame(&mut buf).ok()?;
    let b = &buf[..info.buffer_size()];
    match info.color_type {
        png::ColorType::Rgba => Some(IconSource::from_rgba(b.to_vec(), info.width, info.height)),
        _ => None,
    }
}

/// A *raster* icon from the release, if it shipped one. The SVG is handled
/// separately by `linux::install_icons`, which copies it as-is.
fn vendor_icon_png(root: &Path, slug: &str) -> Option<PathBuf> {
    let id = crate::apps::app_id(slug);
    for size in ["256x256", "128x128", "64x64", "48x48"] {
        let cand = root
            .join("share/icons/hicolor")
            .join(size)
            .join("apps")
            .join(format!("{id}.png"));
        if cand.exists() {
            return Some(cand);
        }
    }
    None
}

pub fn lookup(slug: &str) -> Option<&'static CraftApp> {
    app(slug)
}

pub fn launcher_icon_source() -> IconSource {
    // The ArtCraft mark, rasterised at 1024 so every size is a downscale.
    let (rgba, w, h) = crate::logo::mark_rgba(1024).unwrap_or((vec![0u8; 4], 1, 1));
    IconSource::from_rgba(rgba, w, h)
}

/// Register the launcher itself: its own reserved icon, its own desktop
/// entry / shortcut / bundle, and (on Linux) its own file-manager actions.
///
/// Called automatically the first time the launcher runs, so a fresh install
/// shows up in the app launcher with the right icon without the user doing
/// anything.
pub fn register_launcher(exe: &Path) -> Result<IntegrationReport, String> {
    let mut rep = IntegrationReport::default();

    // 1. Reserve the launcher's own icon in the launcher's icon store.
    match reserve_launcher_icon() {
        Ok(p) => rep.reserved = Some(p),
        Err(e) => rep.notes.push(format!("icon: {e}")),
    }

    #[cfg(target_os = "linux")]
    {
        let p = crate::osint::linux::launcher_desktop_entry(exe)?;
        rep.desktop_entry = Some(p.clone());
        rep.icons.extend(crate::osint::linux::install_launcher_icons()?);
        rep.context_actions
            .extend(crate::osint::linux::launcher_context_actions(exe)?);
    }
    #[cfg(target_os = "windows")]
    {
        rep.shortcuts
            .extend(crate::osint::windows::launcher_shortcut(exe)?);
        rep.icons.extend(crate::osint::windows::install_launcher_icon()?);
    }
    #[cfg(target_os = "macos")]
    {
        let bundle = crate::osint::macos::write_launcher_bundle(exe)?;
        rep.bundle = Some(bundle.clone());
        crate::osint::macos::launch_services_register_launcher(&bundle)?;
    }
    Ok(rep)
}

/// Write the launcher's own icon in the platform's reserved format.
fn reserve_launcher_icon() -> Result<PathBuf, String> {
    let (rgba, w, h) = crate::logo::mark_rgba(1024)
        .ok_or_else(|| "could not rasterise the ArtCraft mark".to_string())?;
    let src = IconSource::from_rgba(rgba, w, h);
    let dir = crate::paths::reserved_icon_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    #[cfg(target_os = "windows")]
    {
        let out = dir.join("artcraft-launcher.ico");
        let imgs: Vec<(u32, Vec<u8>)> = reserved_sizes()
            .iter()
            .map(|s| (*s, icons::encode_png(&src.resized(*s, *s).rgba, *s, *s)))
            .collect();
        std::fs::write(&out, ico_bytes(&imgs)).map_err(|e| e.to_string())?;
        return Ok(out);
    }
    #[cfg(target_os = "macos")]
    {
        let out = dir.join("artcraft-launcher.icns");
        let imgs: Vec<(u32, Vec<u8>)> = [512u32, 256, 128, 32, 16]
            .iter()
            .map(|s| (*s, icons::encode_png(&src.resized(*s, *s).rgba, *s, *s)))
            .collect();
        std::fs::write(&out, icns_bytes(&imgs)).map_err(|e| e.to_string())?;
        return Ok(out);
    }
    #[allow(unreachable_code)]
    {
        let out = dir.join("artcraft-launcher-256.png");
        std::fs::write(&out, icons::encode_png(&src.rgba, src.w, src.h))
            .map_err(|e| e.to_string())?;
        Ok(out)
    }
}

/// Undo everything `integrate` wrote for one app. Idempotent.
pub fn unregister(a: &CraftApp, settings: &crate::config::Settings) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    crate::osint::linux::remove(a, settings)?;
    #[cfg(target_os = "windows")]
    crate::osint::windows::remove(a)?;
    #[cfg(target_os = "macos")]
    crate::osint::macos::remove(a)?;
    #[allow(unreachable_patterns)]
    let _ = settings;
    Ok(())
}
