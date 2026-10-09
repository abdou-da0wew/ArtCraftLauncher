//! The seven Craft apps, with the copy taken verbatim from getartcraft.com/apps
//! and the release/asset conventions observed on `github.com/storytold/<slug>`.

use crate::theme::AppAccent;

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum Platform {
    Macos,
    Windows,
    Linux,
    Freebsd,
}

impl Platform {
    pub fn host() -> Platform {
        if cfg!(target_os = "macos") {
            Platform::Macos
        } else if cfg!(target_os = "windows") {
            Platform::Windows
        } else if cfg!(target_os = "freebsd") {
            Platform::Freebsd
        } else {
            Platform::Linux
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Platform::Macos => "macOS",
            Platform::Windows => "Windows",
            Platform::Linux => "Linux",
            Platform::Freebsd => "FreeBSD",
        }
    }

    /// Asset suffix, e.g. `linux-x86_64`.
    pub fn target_triple(self, arch: Arch) -> Option<&'static str> {
        Some(match (self, arch) {
            (Platform::Macos, _) => "macos-universal",
            (Platform::Windows, Arch::X64) => "windows-x64",
            (Platform::Windows, Arch::Arm64) => "windows-arm64",
            (Platform::Windows, Arch::X86) => "windows-x86",
            (Platform::Linux, Arch::X64) => "linux-x86_64",
            (Platform::Linux, Arch::Arm64) => "linux-aarch64",
            (Platform::Linux, Arch::X86) => return None,
            (Platform::Freebsd, Arch::X64) => "freebsd-x86_64",
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum Arch {
    X64,
    Arm64,
    X86,
}

impl Arch {
    pub fn host() -> Arch {
        if cfg!(target_arch = "aarch64") {
            Arch::Arm64
        } else if cfg!(target_arch = "x86") {
            Arch::X86
        } else if cfg!(target_arch = "x86_64") {
            Arch::X64
        } else {
            Arch::X64
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Arch::X64 => "x86_64",
            Arch::Arm64 => "arm64",
            Arch::X86 => "x86",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum Distro {
    /// Ubuntu/Debian & derivatives — `dpkg` + APT repository exists.
    Deb,
    /// Fedora/RHEL/openSUSE — `rpm`.
    Rpm,
    /// Any distro with a desktop — AppImage + tarball.
    Generic,
}

impl Distro {
    pub fn detect() -> Distro {
        if let Ok(id) = std::fs::read_to_string("/etc/os-release") {
            let id = id.to_ascii_lowercase();
            if id.contains("debian") || id.contains("ubuntu") || id.contains("mint")
                || id.contains("pop") || id.contains("elementary") || id.contains("zorin")
                || id.contains("fedora") == false && id.contains("\nid_like=") == false
            {
                // fallthrough to the id_like check below
            }
            let id_like = id
                .lines()
                .find(|l| l.starts_with("ID_LIKE="))
                .map(|l| l.trim_start_matches("ID_LIKE=").trim_matches('"'))
                .unwrap_or("")
                .to_string();
            let id_name = id
                .lines()
                .find(|l| l.starts_with("ID="))
                .map(|l| l.trim_start_matches("ID=").trim_matches('"'))
                .unwrap_or("")
                .to_string();
            let blob = format!("{} {}", id_name, id_like);
            if blob.contains("debian") || blob.contains("ubuntu") {
                return Distro::Deb;
            }
            if blob.contains("fedora") || blob.contains("rhel") || blob.contains("suse")
                || blob.contains("centos")
            {
                return Distro::Rpm;
            }
        }
        Distro::Generic
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub enum Channel {
    #[default]
    Stable,
    Beta,
    Nightly,
}

impl Channel {
    pub const ALL: [Channel; 3] = [Channel::Stable, Channel::Beta, Channel::Nightly];
    pub fn label(self) -> &'static str {
        match self {
            Channel::Stable => "Stable",
            Channel::Beta => "Beta",
            Channel::Nightly => "Nightly",
        }
    }
    pub fn stable() -> Channel {
        Channel::Stable
    }

    /// ArtCraft tags are plain `vMAJOR.MINOR.PATCH` with no pre-release suffix,
    /// so every release is treated as stable unless its body says otherwise.
    pub fn from_tag(tag: &str) -> Channel {
        let t = tag.trim_start_matches('v');
        if t.contains("alpha") || t.contains("beta") || t.contains("rc") {
            Channel::Beta
        } else {
            Channel::Stable
        }
    }
}

#[derive(Clone, Debug)]
pub struct CraftApp {
    pub slug: &'static str,
    /// PascalCase bundle name, as it appears in the DMG volume and `.app`.
    pub bundle: &'static str,
    pub name: &'static str,
    pub category: &'static str,
    pub pitch: &'static str,
    pub lede: &'static str,
    pub headline: (&'static str, &'static str, &'static str),
    pub status: &'static str,
    /// The version the user asked to start from.
    pub pinned: &'static str,
    pub repo: &'static str,
    pub accent: AppAccent,
    /// MIME types this app can open (drives Linux right-click + file association).
    pub mime: &'static [&'static str],
    /// Short noun used in the file-manager context menu, e.g. "Open with VectorCraft".
    pub open_verb: &'static str,
    /// Formats shown as right-click sub-actions.
    pub open_exts: &'static [&'static str],
}

pub const APPS: [CraftApp; 7] = [
    CraftApp {
        slug: "photocraft",
        bundle: "PhotoCraft",
        name: "PhotoCraft",
        category: "Image editor",
        pitch: "The open-source image editor you already know how to use.",
        lede: "Layers, masks, adjustment layers, layer styles, type, vectors and brushes in a native app written entirely in Rust. Open source, offline, and yours.",
        headline: ("The image editor you already ", "know", " how to use."),
        status: "Early alpha",
        pinned: "0.5.0",
        repo: "storytold/photocraft",
        accent: crate::theme::ACCENTS[0],
        mime: &[
            "image/png", "image/jpeg", "image/webp", "image/tiff", "image/bmp", "image/gif",
            "image/x-imgbox", "application/x-photoshop", "image/vnd.adobe.photoshop",
        ],
        open_verb: "Open with PhotoCraft",
        open_exts: &["psd", "png", "jpg", "jpeg", "webp", "tif", "tiff", "bmp", "gif"],
    },
    CraftApp {
        slug: "vectorcraft",
        bundle: "VectorCraft",
        name: "VectorCraft",
        category: "Vector illustration",
        pitch: "Fast, open-source vector illustration, reimagined in pure Rust.",
        lede: "The pen, panels and shortcuts working illustrators expect, in a native app that keeps up with you. Open source, and in your browser too.",
        headline: ("Vector illustration, ", "reimagined", " in pure Rust."),
        status: "In development",
        pinned: "0.7.0",
        repo: "storytold/vectorcraft",
        accent: crate::theme::ACCENTS[1],
        mime: &[
            "image/svg+xml", "application/pdf", "application/postscript",
            "application/illustrator", "image/x-eps", "application/eps",
        ],
        open_verb: "Open with VectorCraft",
        open_exts: &["svg", "ai", "eps", "pdf"],
    },
    CraftApp {
        slug: "filmcraft",
        bundle: "FilmCraft",
        name: "FilmCraft",
        category: "Video editor",
        pitch: "Professional, open-source video editing, rebuilt from scratch in Rust.",
        lede: "Source and program monitors, a real timeline and every trim mode, in a native editor written in pure Rust — down to its own codecs.",
        headline: ("Professional video editing, ", "rebuilt", " from scratch."),
        status: "In development",
        pinned: "0.4.0",
        repo: "storytold/filmcraft",
        accent: crate::theme::ACCENTS[2],
        mime: &[
            "video/mp4", "video/quicktime", "video/x-msvideo", "video/webm",
            "video/x-matroska", "video/mpeg", "application/vnd.apple.mpegurl",
        ],
        open_verb: "Open with FilmCraft",
        open_exts: &["mp4", "mov", "avi", "mkv", "webm", "m4v", "mpg", "mpeg"],
    },
    CraftApp {
        slug: "lightcraft",
        bundle: "LightCraft",
        name: "LightCraft",
        category: "Photo library & raw developer",
        pitch: "An open-source photo library and raw developer that runs on your machine.",
        lede: "A fast photo library and non-destructive raw developer, written from scratch in pure Rust. No account, no cloud, no telemetry, no subscription.",
        headline: ("Your photos. Your pixels. Your ", "machine", "."),
        status: "In development",
        pinned: "0.4.0",
        repo: "storytold/lightcraft",
        accent: crate::theme::ACCENTS[3],
        mime: &[
            "image/x-canon-cr2", "image/x-canon-cr3", "image/x-nikon-nef", "image/x-sony-arw",
            "image/x-adobe-dng", "image/x-panasonic-rw2", "image/x-fuji-raf",
            "image/x-olympus-orf", "image/jpeg", "image/tiff",
        ],
        open_verb: "Open with LightCraft",
        open_exts: &["dng", "cr2", "cr3", "nef", "arw", "raf", "orf", "rw2", "jpg", "tif"],
    },
    CraftApp {
        slug: "pdfcraft",
        bundle: "PdfCraft",
        name: "PdfCraft",
        category: "PDF workbench",
        pitch: "The open-source PDF workbench: read, organize, combine, split and secure.",
        lede: "Read, organize, combine, split and secure PDFs in a fast, native app written in Rust from the ground up. No account, no telemetry, no cloud.",
        headline: ("The ", "open-source", " PDF workbench."),
        status: "Early alpha",
        pinned: "0.4.0",
        repo: "storytold/pdfcraft",
        accent: crate::theme::ACCENTS[4],
        mime: &["application/pdf", "application/x-pdf", "application/x-bzpdf"],
        open_verb: "Open with PdfCraft",
        open_exts: &["pdf"],
    },
    CraftApp {
        slug: "effectcraft",
        bundle: "EffectCraft",
        name: "EffectCraft",
        category: "Motion graphics & VFX",
        pitch: "Open-source motion graphics and visual effects, built in pure Rust.",
        lede: "Compositions, layers, keyframes, 259 effects, expressions, 3D cameras and lights, and a render queue in a native compositor written in pure Rust. Young, moving fast, and already usable.",
        headline: ("Motion graphics and visual ", "effects", ", in pure Rust."),
        status: "In development",
        pinned: "0.6.0",
        repo: "storytold/effectcraft",
        accent: crate::theme::ACCENTS[5],
        mime: &["video/mp4", "video/quicktime", "image/png", "image/jpeg", "image/svg+xml"],
        open_verb: "Open with EffectCraft",
        open_exts: &["mp4", "mov", "png", "jpg", "svg", "exr"],
    },
    CraftApp {
        slug: "designcraft",
        bundle: "DesignCraft",
        name: "DesignCraft",
        category: "Page layout & publishing",
        pitch: "Open-source page layout and publishing, rebuilt in pure Rust.",
        lede: "Spreads and parent pages, threaded stories, styles, swatches and text wrap in a native layout app with a professional paragraph composer. No subscription, no licence server, no telemetry.",
        headline: ("Page layout and ", "publishing", ", rebuilt in pure Rust."),
        status: "In development",
        pinned: "0.4.0",
        repo: "storytold/designcraft",
        accent: crate::theme::ACCENTS[6],
        mime: &[
            "application/pdf", "image/svg+xml", "application/postscript",
            "application/vnd.oasis.opendocument.text", "text/plain",
        ],
        open_verb: "Open with DesignCraft",
        open_exts: &["indd", "pdf", "svg", "idml", "txt", "rtf"],
    },
];

pub fn app(slug: &str) -> Option<&'static CraftApp> {
    APPS.iter().find(|a| a.slug == slug)
}

/// `ai.storyteller.<slug>` — the AppStream/desktop id used inside the releases.
pub fn app_id(slug: &str) -> String {
    format!("ai.storyteller.{}", slug)
}

/// e.g. `photocraft-0.5.0-linux-x86_64.tar.gz`
pub fn asset_filename(slug: &str, version: &str, platform: Platform, arch: Arch) -> Option<String> {
    let triple = platform.target_triple(arch)?;
    Some(format!("{}-{}-{}.tar.gz", slug, version, triple))
}

/// e.g. `photocraft-0.5.0-windows-x64-portable.zip`
pub fn asset_filename_win(slug: &str, version: &str, arch: Arch) -> Option<String> {
    let triple = platform_zip_triple(arch)?;
    Some(format!("{}-{}-{}-portable.zip", slug, version, triple))
}

/// e.g. `photocraft-0.5.0-macos-universal.dmg`
pub fn asset_filename_dmg(slug: &str, version: &str) -> String {
    format!("{}-{}-macos-universal.dmg", slug, version)
}

fn platform_zip_triple(arch: Arch) -> Option<&'static str> {
    Some(match arch {
        Arch::X64 => "windows-x64",
        Arch::Arm64 => "windows-arm64",
        Arch::X86 => "windows-x86",
    })
}

/// Which install strategy to use for a given host.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub enum InstallKind {
    #[default]
    SelfManaged,
    /// System package (deb/rpm) — registers with the OS package manager.
    Package,
    /// MSI via the OS installer.
    Msi,
    /// DMG mounted, `.app` copied to ~/Applications/ArtCraft.
    AppBundle,
}

impl InstallKind {
    pub fn default_kind() -> InstallKind {
        InstallKind::SelfManaged
    }

    pub fn portable_default() -> InstallKind {
        InstallKind::SelfManaged
    }

    pub fn default_for(p: Platform, d: Distro) -> InstallKind {
        match p {
            Platform::Macos => InstallKind::AppBundle,
            Platform::Windows => InstallKind::SelfManaged,
            Platform::Linux | Platform::Freebsd => {
                if d == Distro::Deb {
                    InstallKind::SelfManaged
                } else {
                    InstallKind::SelfManaged
                }
            }
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            InstallKind::SelfManaged => "Portable",
            InstallKind::Package => "System package",
            InstallKind::Msi => "Installer",
            InstallKind::AppBundle => "App bundle",
        }
    }
    pub fn note(self) -> &'static str {
        match self {
            InstallKind::SelfManaged => {
                "Self-contained tree owned by the launcher. Instant switch and rollback."
            }
            InstallKind::Package => "Registers with your system package manager.",
            InstallKind::Msi => "Uses the official installer and Add/Remove Programs.",
            InstallKind::AppBundle => {
                "Copies the signed .app into ~/Applications/ArtCraft and registers with Launch Services."
            }
        }
    }
}
