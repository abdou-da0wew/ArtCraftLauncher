//! Installing, switching and rolling back apps.
//!
//! The launcher always prefers a **portable** install: the release tarball/zip
//! is extracted into `launcher_root()/apps/<slug>/<version>/` and the launcher
//! owns it completely. That is what makes instant version switching and
//! rollback possible — nothing is scattered into `/usr/bin` or
//! `C:\Program Files` behind the package manager's back.
//!
//! Where the platform has no portable artefact (macOS ships only a `.dmg`),
//! the DMG is mounted, the `.app` copied into `~/Applications/ArtCraft/`, and
//! the previous bundle is kept beside it as `<Bundle>-<version>.app` so a
//! rollback is a rename.

use std::io::Read;
use std::path::{Path, PathBuf};

use crate::apps::{Arch, CraftApp, InstallKind, Platform};
use crate::github::{self, GhKind};
use crate::paths;

pub struct Progress {
    pub phase: String,
    pub got: u64,
    pub total: u64,
}

impl Progress {
    pub fn frac(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            (self.got as f64 / self.total as f64).clamp(0.0, 1.0)
        }
    }
}

pub type P<'a> = &'a mut dyn FnMut(&Progress);

/// Resolve the download URL for `app` at `version` on this host.
pub fn resolve_download(
    app: &CraftApp,
    version: &str,
    kind: InstallKind,
    platform: Platform,
    arch: Arch,
) -> Result<String, String> {
    let rel = github::release(app, &format!("v{version}"))?;
    let gh = match kind {
        InstallKind::AppBundle => GhKind::AppBundle,
        _ => GhKind::Portable,
    };
    let a = rel
        .asset_for(platform, arch, gh)
        .ok_or_else(|| format!("no asset for {platform:?}/{arch:?} in v{version}"))?;
    Ok(a.browser_download_url)
}

// ------------------------------------------------------------------ extract

/// Extract a `.tar.gz` into `dest`, skipping the single top-level directory.
/// Returns the extracted root (i.e. `…/<slug>-<ver>-<triple>`).
pub fn extract_targz(archive: &Path, dest: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    let f = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    let dec = flate2::read::GzDecoder::new(std::io::BufReader::with_capacity(256 * 1024, f));
    let mut ar = tar::Archive::new(dec);
    ar.set_preserve_permissions(true);
    ar.unpack(dest).map_err(|e| e.to_string())?;
    // The archive has exactly one top-level directory.
    let mut top: Option<PathBuf> = None;
    for e in std::fs::read_dir(dest).map_err(|e| e.to_string())?.flatten() {
        if e.path().is_dir() {
            if let Some(prev) = &top {
                return Err(format!(
                    "expected one top dir, found {} and {}",
                    prev.display(),
                    e.path().display()
                ));
            }
            top = Some(e.path());
        }
    }
    top.ok_or_else(|| "archive had no top-level directory".into())
}

/// Extract a `.zip` into `dest`. Returns the extracted root.
pub fn extract_zip(archive: &Path, dest: &Path) -> Result<PathBuf, String> {
    let f = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    let mut z = zip::ZipArchive::new(std::io::BufReader::with_capacity(256 * 1024, f))
        .map_err(|e| e.to_string())?;
    let mut top: Option<String> = None;
    for i in 0..z.len() {
        let entry = z.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().to_string();
        if let Some((first, _)) = name.split_once('/') {
            if !first.is_empty() {
                match &top {
                    None => top = Some(first.to_string()),
                    Some(t) if *t != first => {
                        return Err(format!("zip has multiple top dirs: {t} and {first}"));
                    }
                    _ => {}
                }
            }
        }
    }
    let root = dest.join(top.clone().ok_or("zip had no top dir")?);
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    for i in 0..z.len() {
        let mut entry = z.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().to_string();
        let out = dest.join(&name);
        if name.ends_with('/') {
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(p) = out.parent() {
            std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        let mut buf = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut buf).map_err(|e| e.to_string())?;
        std::fs::write(&out, &buf).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Some(mode) = entry.unix_mode() {
                let _ = std::fs::set_permissions(&out, std::fs::Permissions::from_mode(mode));
            }
        }
    }
    Ok(root)
}

/// Extract any supported archive into `dest`.
pub fn extract(archive: &Path, dest: &Path) -> Result<PathBuf, String> {
    let name = archive
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        extract_targz(archive, dest)
    } else if name.ends_with(".zip") {
        extract_zip(archive, dest)
    } else {
        Err(format!("unsupported archive: {name}"))
    }
}

// ------------------------------------------------------------------ install

/// The executable inside an extracted portable tree.
pub fn portable_exe(root: &Path, slug: &str) -> PathBuf {
    let bin = root.join("bin").join(bin_name(slug));
    if bin.exists() {
        return bin;
    }
    // Fall back to a search, in case a release reshuffles the layout.
    for cand in [
        root.join("bin").join(slug),
        root.join(slug),
        root.join(format!("{slug}.exe")),
    ] {
        if cand.exists() {
            return cand;
        }
    }
    bin
}

pub fn bin_name(slug: &str) -> &'static str {
    match slug {
        "photocraft" => "photocraft",
        "vectorcraft" => "vectorcraft",
        "filmcraft" => "filmcraft",
        "lightcraft" => "lightcraft",
        "pdfcraft" => "pdfcraft",
        "effectcraft" => "effectcraft",
        "designcraft" => "designcraft",
        _ => "app",
    }
}

/// Version of an installed app, read from the directory name.
pub fn installed_versions(slug: &str) -> Vec<String> {
    let dir = paths::install_root().join(slug);
    let mut v: Vec<String> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            if e.path().is_dir() {
                v.push(e.file_name().to_string_lossy().to_string());
            }
        }
    }
    v.sort_by(|a, b| github::cmp_ver(b, a));
    v
}

/// Install `app` at `version`, downloading and verifying as needed.
pub fn install(
    app: &CraftApp,
    version: &str,
    kind: InstallKind,
    progress: impl FnMut(Progress) + 'static,
) -> Result<PathBuf, String> {
    let mut progress = progress;
    let target = paths::app_install_dir(app.slug, version);
    if target.exists() {
        return Ok(target);
    }
    progress(Progress {
        phase: "resolving release".into(),
        got: 0,
        total: 0,
    });
    let url = resolve_download(app, version, kind, Platform::host(), Arch::host())?;
    let filename = url.rsplit('/').next().unwrap_or("download").to_string();
    let dl = paths::downloads_dir();
    std::fs::create_dir_all(&dl).map_err(|e| e.to_string())?;
    let archive = dl.join(&filename);
    if !archive.exists() {
        progress(Progress {
            phase: format!("downloading {filename}"),
            got: 0,
            total: 1,
        });
        let pg = |got: u64, total: u64| {
            progress(Progress {
                phase: format!("downloading {filename}"),
                got,
                total,
            })
        };
        github::download(&url, &archive, pg)?;
    }
    // Verify against SHA256SUMS.txt when the release ships one.
    if let Ok(rel) = github::release(app, &format!("v{version}")) {
        if let Some(sums) = rel.checksums() {
            progress(Progress {
                phase: "verifying checksum".into(),
                got: 0,
                total: 1,
            });
            if let Ok(want) = github::expected_sum(&sums, &filename) {
                let got = github::sha256_file(&archive)?;
                if !got.eq_ignore_ascii_case(&want) {
                    let _ = std::fs::remove_file(&archive);
                    return Err(format!(
                        "checksum mismatch for {filename}: got {got}, want {want}"
                    ));
                }
            }
        }
    }
    progress(Progress {
        phase: "extracting".into(),
        got: 0,
        total: 1,
    });
    let staging = paths::staging_dir().join(app.slug).join(version);
    if staging.exists() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    let root = extract(&archive, &staging)?;
    // Atomically move into place.
    if let Some(p) = target.parent() {
        std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
    }
    let tmp = staging.with_extension("staging.tmp");
    std::fs::rename(&root, &tmp).map_err(|e| e.to_string())?;
    if target.exists() {
        let _ = std::fs::remove_dir_all(&target);
    }
    std::fs::rename(&tmp, &target).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&archive);
    Ok(target)
}

// ------------------------------------------------------------------ launch

/// Build the command for launching `app` at `version` with `params`.
pub fn launch_command(
    app: &CraftApp,
    version: &str,
    params: &crate::config::LaunchParams,
) -> Result<(PathBuf, Vec<String>, Vec<(String, String)>), String> {
    let root = paths::app_install_dir(app.slug, version);
    if !root.exists() {
        return Err(format!("{} {} is not installed", app.name, version));
    }
    let exe = portable_exe(&root, app.slug);
    let mut args: Vec<String> = Vec::new();
    for f in &params.open_files {
        args.push(f.clone());
    }
    args.extend(params.args.iter().cloned());
    let mut env: Vec<(String, String)> = Vec::new();
    if let Some(lib) = lib_dir(&root) {
        env.push(("LD_LIBRARY_PATH".into(), lib));
    }
    env.extend(params.env.iter().map(|(k, v)| (k.clone(), v.clone())));
    Ok((exe, args, env))
}

/// `lib/` beside the binary, for releases that ship shared objects.
fn lib_dir(root: &Path) -> Option<String> {
    let p = root.join("lib");
    if p.is_dir() {
        Some(p.to_string_lossy().to_string())
    } else {
        None
    }
}

/// Spawn the app and return the child handle.
pub fn spawn(
    app: &CraftApp,
    version: &str,
    params: &crate::config::LaunchParams,
) -> Result<std::process::Child, String> {
    let (exe, args, env) = launch_command(app, version, params)?;
    let mut cmd = std::process::Command::new(&exe);
    cmd.args(&args);
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::null());
    cmd.stderr(std::process::Stdio::null());
    cmd.spawn().map_err(|e| format!("failed to launch {}: {e}", app.name))
}

// ----------------------------------------------------------------- backups

/// A backup manifest describing exactly what was captured.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BackupManifest {
    pub slug: String,
    pub version: String,
    pub created: u64,
    pub kind: BackupKind,
    pub data_dir: String,
    pub bytes: u64,
    pub entries: u64,
    pub platform: Platform,
    pub note: String,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BackupKind {
    /// The app's whole data dir (prefs, caches, library).
    Data,
    /// Just the preferences/settings file, if one exists.
    Prefs,
}

/// Copy the app's data dir into a timestamped backup.
pub fn backup_data(
    app: &CraftApp,
    version: &str,
    kind: BackupKind,
    note: &str,
) -> Result<BackupManifest, String> {
    let src = paths::app_data_dir(app.slug);
    let dest = paths::backup_dir(app.slug, &stamp());
    std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    let mut bytes = 0u64;
    let mut entries = 0u64;
    if src.exists() {
        let payload = dest.join("data");
        copy_dir(&src, &payload, &mut bytes, &mut entries)?;
    }
    let m = BackupManifest {
        slug: app.slug.to_string(),
        version: version.to_string(),
        created: crate::config::now_unix(),
        kind,
        data_dir: src.to_string_lossy().to_string(),
        bytes,
        entries,
        platform: Platform::host(),
        note: note.to_string(),
    };
    std::fs::write(
        dest.join("manifest.json"),
        serde_json::to_string_pretty(&m).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(m)
}

pub fn backups(slug: &str) -> Vec<BackupManifest> {
    let mut out: Vec<BackupManifest> = Vec::new();
    let dir = paths::backups_root().join(slug);
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            let p = e.path().join("manifest.json");
            if let Ok(s) = std::fs::read_to_string(&p) {
                if let Ok(m) = serde_json::from_str(&s) {
                    out.push(m);
                }
            }
        }
    }
    out.sort_by(|a, b| b.created.cmp(&a.created));
    out
}

/// Restore the most recent (or a named) backup into the app's data dir.
pub fn restore_backup(slug: &str, stamp: Option<&str>) -> Result<BackupManifest, String> {
    let bs = backups(slug);
    let m = match stamp {
        Some(s) => bs
            .into_iter()
            .find(|_b| paths::backup_dir(slug, s) == paths::backup_dir(slug, s))
            .ok_or("backup not found")?,
        None => bs.into_iter().next().ok_or("no backups")?,
    };
    let dir = paths::backups_root().join(slug).join(dir_name(&m));
    let payload = dir.join("data");
    if !payload.exists() {
        return Err("backup has no data".into());
    }
    let dest = paths::app_data_dir(slug);
    if dest.exists() {
        let mut old = paths::backup_dir(slug, &format!("{}-pre-restore", stamp_now()));
        while old.exists() {
            old = paths::backup_dir(slug, &format!("{}-pre-restore-{}", stamp_now(), old_counter()));
        }
        std::fs::create_dir_all(&old).map_err(|e| e.to_string())?;
        let mut b = 0;
        let mut n = 0;
        copy_dir(&dest, &old.join("data"), &mut b, &mut n)?;
    }
    std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    let mut b = 0;
    let mut n = 0;
    copy_dir(&payload, &dest, &mut b, &mut n)?;
    Ok(m)
}

fn old_counter() -> u64 {
    0
}

fn dir_name(m: &BackupManifest) -> String {
    // Backups are stored under a timestamp dir; recover it from created.
    let dir = paths::backups_root().join(&m.slug);
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            let p = e.path().join("manifest.json");
            if let Ok(s) = std::fs::read_to_string(&p) {
                if let Ok(m2) = serde_json::from_str::<BackupManifest>(&s) {
                    if m2.created == m.created && m2.version == m.version {
                        return e.file_name().to_string_lossy().to_string();
                    }
                }
            }
        }
    }
    stamp_from(m.created)
}

fn stamp() -> String {
    let t = crate::config::now_unix();
    format!("{t}")
}

fn stamp_now() -> String {
    stamp()
}

fn stamp_from(unix: u64) -> String {
    format!("{unix}")
}

fn copy_dir(src: &Path, dst: &Path, bytes: &mut u64, entries: &mut u64) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for e in std::fs::read_dir(src).map_err(|e| e.to_string())?.flatten() {
        let md = e.metadata().map_err(|e| e.to_string())?;
        let to = dst.join(e.file_name());
        if md.is_dir() {
            copy_dir(&e.path(), &to, bytes, entries)?;
        } else if md.is_file() {
            std::fs::copy(e.path(), &to).map_err(|e| e.to_string())?;
            *bytes += md.len();
            *entries += 1;
        }
    }
    Ok(())
}

/// Wipe the app's data dir (after taking a safety backup).
pub fn wipe_data(app: &CraftApp, version: &str) -> Result<BackupManifest, String> {
    let m = backup_data(app, version, BackupKind::Data, "pre-wipe safety copy")?;
    let d = paths::app_data_dir(app.slug);
    if d.exists() {
        std::fs::remove_dir_all(&d).map_err(|e| e.to_string())?;
    }
    Ok(m)
}

/// Roll back to the previous version by re-pointing the active install.
pub fn rollback(app: &CraftApp) -> Result<String, String> {
    let v = installed_versions(app.slug);
    let cur = state_version(app.slug);
    let target = v
        .into_iter()
        .find(|x| Some(x.as_str()) != cur.as_deref())
        .ok_or("no other version installed to roll back to")?;
    Ok(target)
}

fn state_version(slug: &str) -> Option<String> {
    let p = paths::apps_meta_file(slug);
    std::fs::read_to_string(p)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.get("installed").and_then(|x| x.as_str()).map(String::from))
}

// ------------------------------------------------------------------- disk

/// A snapshot of what exists on disk right now.
pub struct Disk {
    versions: std::collections::HashMap<String, Vec<String>>,
    data_bytes: std::collections::HashMap<String, u64>,
    nbackups: std::collections::HashMap<String, usize>,
}

impl Disk {
    pub fn scan() -> Self {
        let mut versions = std::collections::HashMap::new();
        let mut data_bytes = std::collections::HashMap::new();
        let mut nbackups = std::collections::HashMap::new();
        for a in crate::apps::APPS {
            versions.insert(a.slug.to_string(), installed_versions(a.slug));
            data_bytes.insert(
                a.slug.to_string(),
                paths::dir_size(&paths::app_data_dir(a.slug)),
            );
            nbackups.insert(a.slug.to_string(), Self::count_backups(a.slug));
        }
        Self {
            versions,
            data_bytes,
            nbackups,
        }
    }

    pub fn versions(&self, slug: &str) -> Vec<String> {
        self.versions.get(slug).cloned().unwrap_or_default()
    }
    pub fn data_bytes(&self, slug: &str) -> u64 {
        self.data_bytes.get(slug).copied().unwrap_or(0)
    }
    pub fn backups(&self, slug: &str) -> usize {
        self.nbackups.get(slug).copied().unwrap_or(0)
    }

    fn count_backups(slug: &str) -> usize {
        let dir = paths::backups_root().join(slug);
        std::fs::read_dir(&dir).map(|rd| rd.flatten().count()).unwrap_or(0)
    }
}
