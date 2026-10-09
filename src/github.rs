// GitHub releases — the single source of truth for every app version.
//
// ArtCraft ships one release per version with a fixed asset naming scheme,
// verified across all seven repos:
//
// ```text
// <slug>-<ver>-macos-universal.dmg
// <slug>-<ver>-linux-x86_64.tar.gz      <slug>-<ver>-linux-aarch64.tar.gz
// <slug>-<ver>-windows-x64-portable.zip <slug>-<ver>-windows-arm64-portable.zip
// <slug>-<ver>-freebsd-x86_64.tar.gz
// <slug>-<ver>-web-<ver>.zip
// SHA256SUMS.txt
// ```
// Portable archives unpack to `<slug>-<ver>-<triple>/bin/<slug>` and
// `bin/<slug>-cli`; the DMG contains `<Bundle> <ver>/<Bundle>.app`.

use std::cmp::Ordering;
use std::time::Duration;

use serde::Deserialize;

use crate::apps::{app, Arch, CraftApp, Platform};

const API: &str = "https://api.github.com";

pub fn user_agent() -> String {
    format!("artcraft-launcher/{}", env!("CARGO_PKG_VERSION"))
}

fn client(timeout: Option<Duration>) -> ureq::Agent {
    let mut b = ureq::AgentBuilder::new()
        .user_agent(&user_agent())
        .timeout_connect(Duration::from_secs(10));
    if let Some(t) = timeout {
        b = b.timeout(t);
    }
    b.build()
}

#[derive(Debug, Clone, Deserialize)]
pub struct Asset {
    pub name: String,
    pub size: u64,
    pub browser_download_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Release {
    pub tag_name: String,
    pub name: Option<String>,
    pub body: Option<String>,
    pub prerelease: bool,
    pub draft: bool,
    pub published_at: Option<String>,
    pub html_url: String,
    pub assets: Vec<Asset>,
}

impl Release {
    pub fn version(&self) -> String {
        self.tag_name.trim_start_matches('v').to_string()
    }

    /// The asset matching the requested platform/arch, or None.
    pub fn asset_for(&self, platform: Platform, arch: Arch, kind: GhKind) -> Option<Asset> {
        let a = self.assets.iter().find(|a| match kind {
            GhKind::AppBundle => a.name.ends_with("-macos-universal.dmg"),
            GhKind::Portable | GhKind::Package | GhKind::Msi => {
                match platform {
                    Platform::Macos => a.name.ends_with("-macos-universal.dmg"),
                    Platform::Windows => {
                        let suffix = match arch {
                            Arch::X64 => "windows-x64-portable.zip",
                            Arch::Arm64 => "windows-arm64-portable.zip",
                            Arch::X86 => "windows-x86-portable.zip",
                        };
                        a.name.ends_with(suffix)
                    }
                    _ => platform
                        .target_triple(arch)
                        .map(|t| a.name.ends_with(&format!("{t}.tar.gz")))
                        .unwrap_or(false),
                }
            }
        })?;
        Some(a.clone())
    }

    pub fn checksums(&self) -> Option<Asset> {
        self.assets
            .iter()
            .find(|a| a.name == "SHA256SUMS.txt")
            .cloned()
    }

    /// Parsed changelog lines, capped.
    pub fn notes(&self, max: usize) -> Vec<String> {
        self.body
            .as_deref()
            .unwrap_or("")
            .lines()
            .filter(|l| l.trim_start().starts_with('*'))
            .map(|l| {
                l.trim_start_matches('*')
                    .trim()
                    .split(" by @")
                    .next()
                    .unwrap_or("")
                    .trim_start_matches("Fix ")
                    .to_string()
            })
            .filter(|s| !s.is_empty())
            .take(max)
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GhKind {
    AppBundle,
    Portable,
    Package,
    Msi,
}

pub fn release(app: &CraftApp, tag: &str) -> Result<Release, String> {
    let url = format!("{API}/repos/{}/releases/tags/{}", app.repo, tag);
    let body = client(Some(Duration::from_secs(20)))
        .get(&url)
        .call()
        .map_err(|e| format!("{} {tag}: {e}", app.slug))?
        .into_string()
        .map_err(|e| e.to_string())?;
    serde_json::from_str(&body).map_err(|e| format!("bad JSON: {e}"))
}

pub fn releases(app: &CraftApp, pages: u32) -> Result<Vec<Release>, String> {
    let mut out = Vec::new();
    for p in 1..=pages {
        let url = format!("{API}/repos/{}/releases?per_page=30&page={p}", app.repo);
        let body = client(Some(Duration::from_secs(25)))
            .get(&url)
            .call()
            .map_err(|e| e.to_string())?
            .into_string()
            .map_err(|e| e.to_string())?;
        let v: Vec<Release> = serde_json::from_str(&body).map_err(|e| e.to_string())?;
        if v.is_empty() {
            break;
        }
        out.extend(v);
    }
    out.retain(|r| !r.draft);
    Ok(out)
}

pub fn latest(app: &CraftApp, include_prerelease: bool) -> Result<Release, String> {
    let url = if include_prerelease {
        format!("{API}/repos/{}/releases", app.repo)
    } else {
        format!("{API}/repos/{}/releases/latest", app.repo)
    };
    let body = client(Some(Duration::from_secs(25)))
        .get(&url)
        .call()
        .map_err(|e| e.to_string())?
        .into_string()
        .map_err(|e| e.to_string())?;
    if include_prerelease {
        let v: Vec<Release> = serde_json::from_str(&body).map_err(|e| e.to_string())?;
        v.into_iter()
            .find(|r| !r.draft && (!r.prerelease || include_prerelease))
            .ok_or_else(|| "no releases".into())
    } else {
        let r: Release = serde_json::from_str(&body).map_err(|e| e.to_string())?;
        if r.draft {
            return releases(app, 1)?
                .into_iter()
                .find(|r| !r.draft && !r.prerelease)
                .ok_or_else(|| "no releases".into());
        }
        Ok(r)
    }
}

/// `sha256` of a file, read in chunks.
pub fn sha256_file(path: &std::path::Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex::encode(h.finalize()))
}

/// Download `url` to `dest`, reporting progress through `cb`. Returns bytes.
pub fn download(
    url: &str,
    dest: &std::path::Path,
    mut cb: impl FnMut(u64, u64),
) -> Result<u64, String> {
    let part = dest.with_extension("part");
    let resp = client(None)
        .get(url)
        .call()
        .map_err(|e| format!("download failed: {e}"))?;
    let total = resp
        .header("content-length")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    let mut got = 0u64;
    let f = std::fs::File::create(&part).map_err(|e| e.to_string())?;
    let mut w = std::io::BufWriter::with_capacity(512 * 1024, f);
    {
        use std::io::Write;
        let mut r = resp.into_reader();
        let mut buf = vec![0u8; 256 * 1024];
        loop {
            let n = r.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            w.write_all(&buf[..n]).map_err(|e| e.to_string())?;
            got += n as u64;
            cb(got, total);
        }
        w.flush().map_err(|e| e.to_string())?;
    }
    drop(w);
    std::fs::rename(&part, dest).map_err(|e| e.to_string())?;
    Ok(got)
}

/// Fetch `SHA256SUMS.txt` and return the expected digest for `filename`.
pub fn expected_sum(sums_asset: &Asset, filename: &str) -> Result<String, String> {
    let body = client(Some(Duration::from_secs(20)))
        .get(&sums_asset.browser_download_url)
        .call()
        .map_err(|e| e.to_string())?
        .into_string()
        .map_err(|e| e.to_string())?;
    for line in body.lines() {
        let mut it = line.split_whitespace();
        let (Some(hash), Some(name)) = (it.next(), it.next()) else {
            continue;
        };
        let name = name.trim_start_matches('*');
        if name == filename || name.ends_with(&format!("/{filename}")) {
            return Ok(hash.trim().to_ascii_lowercase());
        }
    }
    Err(format!("{filename} not in SHA256SUMS"))
}

/// Compare two `x.y.z` strings.
pub fn cmp_ver(a: &str, b: &str) -> std::cmp::Ordering {
    let pa: Vec<u64> = a.split('.').map(|s| s.parse().unwrap_or(0)).collect();
    let pb: Vec<u64> = b.split('.').map(|s| s.parse().unwrap_or(0)).collect();
    let n = pa.len().max(pb.len());
    for i in 0..n {
        let x = pa.get(i).copied().unwrap_or(0);
        let y = pb.get(i).copied().unwrap_or(0);
        match x.cmp(&y) {
            std::cmp::Ordering::Equal => continue,
            o => return o,
        }
    }
    std::cmp::Ordering::Equal
}

/// Look up the app by slug, for CLI ergonomics.
pub fn app_by_slug(slug: &str) -> Result<&'static CraftApp, String> {
    app(slug).ok_or_else(|| format!("unknown app '{slug}'"))
}


/// Check for launcher updates from GitHub.
/// Returns (current_version, latest_version, update_type, release_notes) if update available.
pub fn check_launcher_updates() -> Result<Option<(String, String, UpdateType, String)>, String> {
    // Current version from Cargo.toml
    let current = env!("CARGO_PKG_VERSION");
    
    // Fetch latest release from GitHub
    let url = "https://api.github.com/repos/abdou-da0wew/ArtCraftLauncher/releases/latest";
    let resp = ureq::get(url)
        .set("User-Agent", "artcraft-launcher")
        .call()
        .map_err(|e| format!("GitHub API error: {e}"))?;
    
    if !(200..300).contains(&resp.status()) {
        return Ok(None);
    }
    
    let release: serde_json::Value = resp.into_json()
        .map_err(|e| format!("JSON parse error: {e}"))?;
    
    let latest_tag = release["tag_name"].as_str()
        .ok_or("no tag_name")?;
    let latest = latest_tag.trim_start_matches('v');
    
    // Compare versions
    let cmp = compare_versions(current, latest);
    if cmp != Ordering::Less {
        return Ok(None); // No update available
    }
    
    // Determine update type from tag
    let update_type = classify_update_type(latest_tag);
    let notes = release["body"].as_str().unwrap_or("").to_string();
    
    Ok(Some((current.to_string(), latest.to_string(), update_type, notes)))
}

/// Compare semantic versions (simple semver compare)
fn compare_versions(a: &str, b: &str) -> Ordering {
    let parse = |s: &str| {
        s.trim_start_matches('v')
         .split('.')
         .map(|p| p.parse::<u32>().unwrap_or(0))
         .collect::<Vec<_>>()
    };
    let a = parse(a);
    let b = parse(b);
    for i in 0..a.len().max(b.len()) {
        let av = a.get(i).copied().unwrap_or(0);
        let bv = b.get(i).copied().unwrap_or(0);
        match av.cmp(&bv) {
            Ordering::Equal => continue,
            o => return o,
        }
    }
    Ordering::Equal
}

/// Classify the update type from the release tag
fn classify_update_type(tag: &str) -> UpdateType {
    let t = tag.to_lowercase();
    if t.contains("critical") || t.contains("hotfix") || t.contains("security") {
        UpdateType::Critical
    } else if t.contains("beta") || t.contains("rc") || t.contains("alpha") {
        UpdateType::Beta
    } else if t.contains("major") {
        UpdateType::Major
    } else if t.contains("minor") {
        UpdateType::Minor
    } else if t.contains("patch") || t.contains("fix") || t.contains("hotfix") {
        UpdateType::Patch
    } else {
        // Default: check version bump
        // If major version increased -> Major
        // If minor increased -> Minor
        // If patch increased -> Patch
        UpdateType::Patch
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum UpdateType {
    Critical,
    Major,
    Minor,
    Patch,
    Beta,
}

impl UpdateType {
    pub fn auto_apply(self, settings: &crate::config::TelemetryConfig) -> bool {
        // Critical and patch always auto-apply
        // Minor/major/beta ask user (unless configured otherwise)
        matches!(self, UpdateType::Critical | UpdateType::Patch)
    }
    
    pub fn label(self) -> &'static str {
        match self {
            UpdateType::Critical => "Critical",
            UpdateType::Major => "Major",
            UpdateType::Minor => "Minor",
            UpdateType::Patch => "Patch",
            UpdateType::Beta => "Beta",
        }
    }
}
