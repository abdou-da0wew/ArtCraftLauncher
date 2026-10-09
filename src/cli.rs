//! The headless CLI. Every management operation is available without a GPU
//! or a display, so the launcher is scriptable and works over SSH.

use std::io::Write;
use std::path::PathBuf;

use crate::apps::{Arch, Channel, Distro, InstallKind, Platform, APPS};
use crate::config::{self, Settings};
use crate::github::{self, GhKind};
use crate::installer::{self, BackupKind};
use crate::paths;
use crate::telemetry::{self, DeviceMetrics, Report, ReportKind, Trail};

const HELP: &str = "\
artcraft-launcher — launcher, updater and manager for the ArtCraft apps

USAGE
  artcraft-launcher                       open the launcher window
  artcraft-launcher gui                   (same)
  artcraft-launcher list                  show every app, its version and state
  artcraft-launcher check [app…]          ask GitHub for the newest releases
  artcraft-launcher install <app> [-c stable|beta|nightly] [--version V]
  artcraft-launcher update [app…]          download + install newer versions
  artcraft-launcher launch <app> [-- args…]
  artcraft-launcher rollback <app>         switch back to the previous version
  artcraft-launcher versions <app>         list the versions installed
  artcraft-launcher backup <app> [-n note]
  artcraft-launcher restore <app> [stamp]
  artcraft-launcher backups <app>
  artcraft-launcher data <app>             print the app's data dir
  artcraft-launcher data-open <app>        reveal it in the file manager
  artcraft-launcher wipe <app>             (backs up first)
  artcraft-launcher register <app>         desktop entry / icon / shortcuts
  artcraft-launcher unregister <app>
  artcraft-launcher register-all
  artcraft-launcher export [-o FILE]
  artcraft-launcher import FILE
  artcraft-launcher telemetry status
  artcraft-launcher telemetry list [app]
  artcraft-launcher telemetry flush
  artcraft-launcher telemetry clear
  artcraft-launcher telemetry endpoint URL
  artcraft-launcher doctor
  artcraft-launcher info
  artcraft-launcher self-update
  artcraft-launcher --version | -h | --help

UPDATES ARE ALWAYS ASKED FOR. `update` will print what it found and wait for
`--yes`; `--yes` is the only thing that installs anything.
";

pub fn main() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        return 1;
    }

    // Resolve the app argument to an index, or print the options.
    let cmd = args[0].as_str();

    let out = match cmd {
        "-h" | "--help" | "help" => {
            print!("{HELP}");
            0
        }
        "--version" | "-V" => {
            println!("artcraft-launcher {}", env!("CARGO_PKG_VERSION"));
            0
        }
        "info" => cmd_info(),
        "doctor" => cmd_doctor(),
        "list" | "ls" => cmd_list(),
        "check" => cmd_check(args.get(1..).unwrap_or(&[])),
        "install" => cmd_install(&args[1..]),
        "update" => cmd_update(&args[1..]),
        "launch" | "open" => cmd_launch(&args[1..]),
        "rollback" => cmd_rollback(&args[1..]),
        "versions" => cmd_versions(&args[1..]),
        "backup" => cmd_backup(&args[1..]),
        "restore" => cmd_restore(&args[1..]),
        "backups" => cmd_backups(&args[1..]),
        "data" => cmd_data(&args[1..], false),
        "data-open" | "reveal" => cmd_data(&args[1..], true),
        "wipe" => cmd_wipe(&args[1..]),
        "register" => cmd_register(&args[1..], true),
        "register-all" => cmd_register(&[], true),
        "unregister" => cmd_register(&args[1..], false),
        "export" => cmd_export(&args[1..]),
        "import" => cmd_import(&args[1..]),
        "telemetry" => cmd_telemetry(&args[1..]),
        "self-update" => cmd_self_update(&args[1..]),
        "gui" | "ui" => {
            println!("Run the binary with no arguments to open the launcher window.");
            0
        }
        other => {
            eprintln!("unknown command: {other}\n");
            eprint!("{HELP}");
            2
        }
    };
    out
}

// ------------------------------------------------------------------ helpers

fn resolve_app(slug: &str) -> Result<&'static crate::apps::CraftApp, String> {
    crate::apps::app(slug).ok_or_else(|| {
        let names: Vec<&str> = APPS.iter().map(|a| a.slug).collect();
        format!("unknown app '{slug}' (have: {})", names.join(", "))
    })
}

fn human(b: u64) -> String {
    const U: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < 4 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{b} B")
    } else {
        format!("{v:.1} {}", U[i])
    }
}

fn flag(args: &[String], name: &str) -> Option<String> {
    let mut i = 0;
    while i < args.len() {
        if args[i] == name {
            return args.get(i + 1).cloned();
        }
        if let Some(v) = args[i].strip_prefix(&format!("{name}=")) {
            return Some(v.to_string());
        }
        i += 1;
    }
    None
}

fn has_flag(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

fn settings() -> Settings {
    Settings::load()
}

/// The app's active version: the recorded one if it is still on disk, else
/// the newest installed version. A reset config must never make the launcher
/// forget what is actually installed — and this heals it on the way through.
fn resolve_active(slug: &str) -> Option<String> {
    let mut s = settings();
    let recorded = s.app(slug).installed.clone();
    let on_disk = installer::installed_versions(slug);
    let resolved = match recorded {
        Some(ref v) if on_disk.iter().any(|x| x == v) => recorded.clone(),
        _ => on_disk.into_iter().next(),
    };
    if resolved != recorded {
        let e = s.app_mut(slug);
        e.installed = resolved.clone();
        let _ = s.save();
    }
    resolved
}

// ----------------------------------------------------------------- commands

fn cmd_info() -> i32 {
    println!("launcher      {} ({})", env!("CARGO_PKG_VERSION"), paths::LAUNCHER_NAME);
    println!("platform      {} {}", Platform::host().label(), Arch::host().label());
    #[cfg(target_os = "linux")]
    println!("distro        {:?}", Distro::detect());
    println!("app id        {}", paths::LAUNCHER_ID);
    println!("launcher home {}", paths::launcher_root().display());
    println!("config        {}", paths::config_file().display());
    println!("installs      {}", paths::install_root().display());
    println!("data          {}", paths::data_home().display());
    println!("config home   {}", paths::config_home().display());
    println!("cache         {}", paths::cache_home().display());
    println!("backups       {}", paths::backups_root().display());
    println!("telemetry     {}", paths::telemetry_dir().display());
    println!(
        "default asset {}",
        match Platform::host() {
            Platform::Macos => "macos-universal.dmg",
            Platform::Windows => "windows-x64-portable.zip",
            _ => "linux-x86_64.tar.gz",
        }
    );
    let m = DeviceMetrics::sample();
    println!(
        "metrics       cpu {:?} mem {:?}/{:?} cores {}",
        m.cpu_load.map(|c| format!("{c:.0}%")),
        m.mem_available.map(human),
        m.mem_total.map(human),
        m.cores
    );
    0
}

fn cmd_doctor() -> i32 {
    let mut ok = true;
    println!("ArtCraft launcher doctor");
    println!();
    // directories
    let dirs = [
        ("launcher home", paths::launcher_root()),
        ("installs", paths::install_root()),
        ("state", paths::state_dir()),
        ("spool", paths::spool_dir()),
        ("downloads", paths::downloads_dir()),
    ];
    for (name, p) in dirs {
        match std::fs::create_dir_all(&p) {
            Ok(_) => println!("  ok    {name}: {}", p.display()),
            Err(e) => {
                println!("  FAIL  {name}: {} ({e})", p.display());
                ok = false;
            }
        }
    }
    // config
    match Settings::load().save() {
        Ok(_) => println!("  ok    config is writable: {}", paths::config_file().display()),
        Err(e) => {
            println!("  FAIL  config: {e}");
            ok = false;
        }
    }
    // network
    print!("  ..    GitHub reachability…");
    let _ = std::io::stdout().flush();
    match github::latest(&APPS[0], false) {
        Ok(r) => println!("\r  ok    GitHub: latest {} is {}", APPS[0].slug, r.version()),
        Err(e) => {
            println!("\r  WARN  GitHub: {e}");
        }
    }
    // GPU
    println!(
        "  ok    graphics: {}",
        if crate::render::gpu_available() {
            "an adapter is present"
        } else {
            "none found — the CLI still works; the GUI needs a GPU"
        }
    );
    // app data dirs
    for a in APPS {
        let d = paths::app_data_dir(a.slug);
        let writable = std::fs::create_dir_all(&d)
            .map(|_| {
                let probe = d.join(".artcraft-write-test");
                let r = std::fs::write(&probe, b"ok");
                let _ = std::fs::remove_file(&probe);
                r.is_ok()
            })
            .unwrap_or(false);
        if writable {
            println!("  ok    {} data dir writable: {}", a.slug, d.display());
        } else {
            println!("  FAIL  {} data dir not writable: {}", a.slug, d.display());
            ok = false;
        }
    }
    // telemetry
    let s = crate::telemetry::Spool::new();
    println!("  ok    spool holds {} report(s)", s.len());
    let ep = settings().telemetry;
    if ep.endpoint.is_empty() {
        println!("  note  no telemetry endpoint set; reports stay local");
    } else {
        println!("  ok    telemetry endpoint: {}", ep.endpoint);
    }
    if !ep.enabled {
        println!("  note  telemetry is disabled");
    }
    println!();
    println!("{}", if ok { "doctor: healthy" } else { "doctor: problems found" });
    if ok {
        0
    } else {
        1
    }
}

fn cmd_list() -> i32 {
    let s = settings();
    let mut any = false;
    for a in APPS {
        let st = s.app(a.slug);
        let versions = installer::installed_versions(a.slug);
        let bytes = paths::dir_size(&paths::app_data_dir(a.slug));
        let last = st
            .installed
            .clone()
            .unwrap_or_else(|| "—".into());
        let marker = if st.installed.is_some() { "●" } else { "○" };
        any = true;
        println!(
            "{marker} {:<12} {:<8} {:<9} {:<10} data {:<8} channel {:<8} {} version(s)",
            a.name,
            a.category,
            last,
            a.status,
            human(bytes),
            st.channel.label(),
            versions.len()
        );
        if !versions.is_empty() {
            println!("    installed: {}", versions.join(", "));
        }
        if !st.params.args.is_empty() || !st.params.env.is_empty() {
            println!(
                "    params: {} arg(s), {} env var(s)",
                st.params.args.len(),
                st.params.env.len()
            );
        }
    }
    if !any {
        println!("no apps registered");
    }
    0
}

fn cmd_check(targets: &[String]) -> i32 {
    let list: Vec<&'static crate::apps::CraftApp> = if targets.is_empty() {
        APPS.iter().collect()
    } else {
        let mut v = Vec::new();
        for t in targets {
            if t == "all" {
                v.extend(APPS.iter());
                continue;
            }
            match resolve_app(t) {
                Ok(a) => v.push(a),
                Err(e) => {
                    eprintln!("{e}");
                    return 2;
                }
            }
        }
        v
    };
    let s = settings();
    for a in list {
        match github::latest(a, false) {
            Ok(r) => {
                let cur = s.app(a.slug).installed.clone().unwrap_or_else(|| "not installed".into());
                let rel = if cur == "not installed" {
                    "new"
                } else {
                    match github::cmp_ver(&r.version(), &cur) {
                        std::cmp::Ordering::Greater => "update available",
                        std::cmp::Ordering::Equal => "up to date",
                        std::cmp::Ordering::Less => "ahead of GitHub",
                    }
                };
                println!(
                    "{:<12} {:<10} {:<19} {}",
                    a.name,
                    cur,
                    r.version(),
                    rel
                );
            }
            Err(e) => println!("{:<12} error: {e}", a.name),
        }
    }
    0
}

fn cmd_install(args: &[String]) -> i32 {
    let Some(slug) = args.first() else {
        eprintln!("install: which app? ({} …)", APPS[0].slug);
        return 2;
    };
    let a = match resolve_app(slug) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    let s = settings();
    let st = s.app(a.slug);
    let channel = flag(args, "-c")
        .or_else(|| flag(args, "--channel"))
        .and_then(|c| match c.as_str() {
            "stable" => Some(Channel::Stable),
            "beta" => Some(Channel::Beta),
            "nightly" => Some(Channel::Nightly),
            _ => None,
        })
        .unwrap_or(st.channel);
    let version = flag(args, "--version").unwrap_or_else(|| {
        github::latest(a, channel != Channel::Stable)
            .map(|r| r.version())
            .unwrap_or_else(|_| st.installed.clone().unwrap_or_else(|| a.pinned.to_string()))
    });
    let kind = st.kind;
    println!(
        "installing {} {} for {} {}…",
        a.name,
        version,
        Platform::host().label(),
        Arch::host().label()
    );
    // Throttled so a fast link does not flood the terminal.
    let _last_pct = 0u32;
    let mut last_pct = 0u32;
    match installer::install(a, &version, kind, move |p| {
        if p.total > 0 {
            let pct = (p.frac() * 100.0) as u32;
            if pct >= last_pct + 5 || pct == 100 {
                last_pct = pct;
                eprint!(
                    "\r  {} {}% ({}/{})   ",
                    p.phase, pct, human(p.got), human(p.total)
                );
                let _ = std::io::stderr().flush();
            }
        }
    }) {
        Ok(root) => {
            println!();
            println!("  installed to {}", root.display());
            let mut s2 = settings();
            let e = s2.app_mut(a.slug);
            e.installed = Some(version.clone());
            e.channel = channel;
            e.kind = kind;
            e.last_good = Some(version.clone());
            let _ = s2.save();
            0
        }
        Err(e) => {
            eprintln!("  install failed: {e}");
            1
        }
    }
}

fn cmd_update(args: &[String]) -> i32 {
    let targets: Vec<&'static crate::apps::CraftApp> = if args.iter().any(|a| a == "all") || args.is_empty()
    {
        APPS.iter().collect()
    } else {
        let mut v = Vec::new();
        for t in args {
            match resolve_app(t) {
                Ok(a) => v.push(a),
                Err(e) => {
                    eprintln!("{e}");
                    return 2;
                }
            }
        }
        v
    };
    let s = settings();
    let mut plan: Vec<(&'static crate::apps::CraftApp, String)> = Vec::new();
    for a in &targets {
        match github::latest(a, false) {
            Ok(r) => {
                let cur = resolve_active(a.slug);
                let should = match &cur {
                    None => true,
                    Some(c) => github::cmp_ver(&r.version(), c) == std::cmp::Ordering::Greater,
                };
                if should {
                    plan.push((a, r.version()));
                } else {
                    println!("{:<12} up to date ({})", a.name, r.version());
                }
            }
            Err(e) => println!("{:<12} could not check: {e}", a.name),
        }
    }
    if plan.is_empty() {
        println!("nothing to update");
        return 0;
    }
    println!();
    println!("ready to install:");
    for (a, v) in &plan {
        println!("  {} {} → {}", a.name, s.app(a.slug).installed.clone().unwrap_or_else(|| "none".into()), v);
    }
    if !has_flag(args, "--yes") && !has_flag(args, "-y") {
        println!();
        println!("Re-run with --yes to install these. Nothing has been changed.");
        return 0;
    }
    let mut rc = 0;
    for (a, v) in plan {
        match cmd_install(&["install".into(), a.slug.to_string(), "--version".into(), v.clone()]) {
            0 => {}
            _ => rc = 1,
        }
    }
    rc
}

fn cmd_launch(args: &[String]) -> i32 {
    let Some(slug) = args.first() else {
        eprintln!("launch: which app?");
        return 2;
    };
    if slug == "--help" || slug == "-h" {
        println!("usage: artcraft-launcher launch <app> [-- extra args]");
        return 0;
    }
    let a = match resolve_app(slug) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    let mut new_window = false;
    let mut send_to = false;
    let mut files: Vec<String> = Vec::new();
    let mut after_ddash = false;
    for x in &args[1..] {
        if after_ddash {
            files.push(x.clone());
            continue;
        }
        match x.as_str() {
            "--" => after_ddash = true,
            "--new" | "-n" => new_window = true,
            "--send-to" => send_to = true,
            "-" => after_ddash = true,
            _ => {}
        }
    }
    let _ = (new_window, send_to);
    let mut trail = Trail::default();
    let s = settings();
    let mut st = s.app(a.slug);
    st.params.open_files.extend(files.clone());
    st.installed = resolve_active(a.slug);
    st.installed = st.installed.or_else(|| Some(a.pinned.to_string()));
    let version = st.installed.clone().unwrap_or_else(|| a.pinned.to_string());
    let log_path = paths::app_log_file(a.slug);
    let offset = std::fs::metadata(&log_path).map(|m| m.len()).unwrap_or(0);
    let started_ms = telemetry::now_ms();
    let pre = trail.recent(8);
    trail.push(format!("launch {} {}", a.slug, version));
    println!("launching {} {}…", a.name, version);
    match installer::spawn(a, &version, &st.params) {
        Ok(child) => {
            // Watch it so an abnormal exit becomes a report.
            let cfg = settings().telemetry;
            let mut w = crate::telemetry::Watched {
                slug: a.slug.to_string(),
                version: version.clone(),
                child,
                started_ms: started_ms,
                metrics: DeviceMetrics::sample(),
                log_path,
                log_offset: offset,
                pre_crash: pre,
            };
            let _ = std::io::stdout().flush();
            loop {
                if let Some(r) = w.poll(&cfg, &trail) {
                    // A clean exit (code 0, no signal) is not worth spooling;
                    // anything else is, and the print says so either way.
                    let abnormal = r.signal.is_some() || r.exit_code.unwrap_or(0) != 0;
                    if abnormal {
                        crate::telemetry::Spool::new().put(&r);
                    }
                    report_and_print(&r);
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(120));
            }
            0
        }
        Err(e) => {
            eprintln!("launch failed: {e}");
            let r = Report {
                id: format!("{}-{}-{}", a.slug, version, telemetry::now_ms()),
                ts: telemetry::now_unix(),
                ts_ms: telemetry::now_ms(),
                kind: ReportKind::LaunchFailed,
                app: a.slug.to_string(),
                version: version.clone(),
                launcher: env!("CARGO_PKG_VERSION").to_string(),
                exit_code: None,
                signal: None,
                message: e.clone(),
                stack: Vec::new(),
                log_tail: Vec::new(),
                pre_crash: trail.recent(8),
                metrics: Some(DeviceMetrics::sample()),
                causes: Vec::new(),
                ran_ms: 0,
            };
            crate::telemetry::Spool::new().put(&r);
            report_and_print(&r);
            1
        }
    }
}

#[allow(dead_code)]
fn cmd_launch_old(args: &[String]) -> i32 {
    let a = match resolve_app(args.first().map(|s| s.as_str()).unwrap_or("")) {
        Ok(x) => x,
        Err(_) => return 2,
    };
    let _ = a;
    0
}
fn cmd_rollback(args: &[String]) -> i32 {
    let Some(slug) = args.first() else {
        eprintln!("rollback: which app");
        return 2;
    };
    let a = resolve_app(slug).unwrap();
    let versions = installer::installed_versions(a.slug);
    let _s = settings();
    let cur = resolve_active(a.slug);
    if versions.len() < 2 {
        eprintln!(
            "only one version installed ({}); roll back by installing another",
            versions.first().cloned().unwrap_or_else(|| "none".into())
        );
        return 1;
    }
    let next = versions
        .into_iter()
        .find(|v| Some(v.as_str()) != cur.as_deref())
        .unwrap();
    let mut s2 = settings();
    let e = s2.app_mut(a.slug);
    e.previous = cur.clone();
    e.installed = Some(next.clone());
    let _ = s2.save();
    println!("rolled {} back to {next}", a.name);
    0
}

fn cmd_versions(args: &[String]) -> i32 {
    let Some(slug) = args.first() else {
        eprintln!("versions: which app");
        return 2;
    };
    let a = match resolve_app(slug) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    let _s = settings();
    let cur = resolve_active(a.slug);
    let v = installer::installed_versions(a.slug);
    if v.is_empty() {
        println!("{} has nothing installed", a.name);
        return 0;
    }
    for x in &v {
        println!("{}{}", x, if Some(x.as_str()) == cur.as_deref() { "   (active)" } else { "" });
    }
    0
}

fn cmd_backup(args: &[String]) -> i32 {
    let Some(slug) = args.first() else {
        eprintln!("backup: which app");
        return 2;
    };
    let a = match resolve_app(slug) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    let note = flag(args, "-n").or_else(|| flag(args, "--note")).unwrap_or_default();
    let version = resolve_active(a.slug).unwrap_or_else(|| a.pinned.to_string());
    match installer::backup_data(a, &version, BackupKind::Data, &note) {
        Ok(m) => {
            println!("backed up {} → {}", a.name, paths::backup_dir(a.slug, &m.created.to_string()).display());
            println!("  {} files, {}", m.entries, human(m.bytes));
            0
        }
        Err(e) => {
            eprintln!("backup failed: {e}");
            1
        }
    }
}

fn cmd_restore(args: &[String]) -> i32 {
    let Some(slug) = args.first() else {
        eprintln!("restore: which app");
        return 2;
    };
    let a = match resolve_app(slug) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    let stamp = args.get(1).cloned();
    match installer::restore_backup(a.slug, stamp.as_deref()) {
        Ok(m) => {
            println!("restored {} (backup from unix {})", a.name, m.created);
            0
        }
        Err(e) => {
            eprintln!("restore failed: {e}");
            1
        }
    }
}

fn cmd_backups(args: &[String]) -> i32 {
    let Some(slug) = args.first() else {
        eprintln!("backups: which app");
        return 2;
    };
    let a = match resolve_app(slug) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    let bs = installer::backups(a.slug);
    if bs.is_empty() {
        println!("{} has no backups", a.name);
        return 0;
    }
    for m in bs {
        println!(
            "unix {:<10} {:<8} {:<8} {} files {:<8} {}",
            m.created,
            m.version,
            match m.kind {
                BackupKind::Data => "data",
                BackupKind::Prefs => "prefs",
            },
            m.entries,
            human(m.bytes),
            m.note
        );
    }
    0
}

fn cmd_data(args: &[String], open: bool) -> i32 {
    let Some(slug) = args.first() else {
        eprintln!("data: which app");
        return 2;
    };
    let a = match resolve_app(slug) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    let d = paths::app_data_dir(a.slug);
    let _ = std::fs::create_dir_all(&d);
    if open {
        let _ = reveal(&d);
    }
    let bytes = paths::dir_size(&d);
    println!("{}", d.display());
    let _ = a;
    let _ = bytes;
    0
}

fn cmd_wipe(args: &[String]) -> i32 {
    let Some(slug) = args.first() else {
        eprintln!("wipe: which app");
        return 2;
    };
    let a = match resolve_app(slug) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    if !has_flag(args, "--yes") {
        println!(
            "this deletes {}  — a safety backup is taken first. Re-run with --yes.",
            paths::app_data_dir(a.slug).display()
        );
        return 0;
    }
    let version = resolve_active(a.slug).unwrap_or_default();
    match installer::wipe_data(a, &version) {
        Ok(_) => {
            println!("wiped {} data (safety copy kept in {})", a.name, paths::backups_root().display());
            0
        }
        Err(e) => {
            eprintln!("wipe failed: {e}");
            1
        }
    }
}

fn cmd_register(args: &[String], on: bool) -> i32 {
    let list: Vec<&'static crate::apps::CraftApp> = if args.is_empty() {
        APPS.iter().collect()
    } else {
        let mut v = Vec::new();
        for t in args {
            match resolve_app(t) {
                Ok(a) => v.push(a),
                Err(e) => {
                    eprintln!("{e}");
                    return 2;
                }
            }
        }
        v
    };
    let s = settings();
    let exe = std::env::current_exe().ok();
    let mut rc = 0;
    for a in list {
        let version = resolve_active(a.slug).unwrap_or_else(|| a.pinned.to_string());
        let res: Result<(), String> = if on {
            crate::osint::integrate(a, &version, &s, exe.as_deref()).map(|r| {
                println!(
                    "registered {:<14} {}",
                    a.slug,
                    r.reserved
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "no icon".into())
                );
                for n in r.notes {
                    println!("   note: {n}");
                }
            })
        } else {
            crate::osint::unregister(a, &s).map(|_| println!("unregistered {}", a.slug))
        };
        if let Err(e) = res {
            eprintln!("  {} failed: {e}", a.slug);
            rc = 1;
        }
    }
    rc
}

fn cmd_export(args: &[String]) -> i32 {
    let path = flag(args, "-o")
        .or_else(|| flag(args, "--output"))
        .map(PathBuf::from)
        .unwrap_or_else(|| paths::default_export_path());
    match config::export_to(&path, &settings()) {
        Ok(p) => {
            println!("exported → {}", p.display());
            0
        }
        Err(e) => {
            eprintln!("export failed: {e}");
            1
        }
    }
}

fn cmd_import(args: &[String]) -> i32 {
    let Some(p) = args.first() else {
        eprintln!("import: which file");
        return 2;
    };
    match config::import_from(&PathBuf::from(p)) {
        Ok(e) => match e.settings.save() {
            Ok(()) => {
                println!("imported settings from {p}");
                0
            }
            Err(er) => {
                eprintln!("import failed: {er}");
                1
            }
        },
        Err(er) => {
            eprintln!("import failed: {er}");
            1
        }
    }
}

fn cmd_self_update(args: &[String]) -> i32 {
    use crate::github::check_launcher_updates;
    match check_launcher_updates() {
        Ok(Some((current, latest, update_type, notes))) => {
            println!("Update available!");
            println!("  Current: {}", current);
            println!("  Latest:  {}", latest);
            println!("  Type:    {}", update_type.label());
            if !notes.is_empty() {
                println!("  Notes:   {}", notes.lines().next().unwrap_or(""));
            }
            let auto = update_type.auto_apply(&settings().telemetry);
            if auto {
                println!("  -> Auto-applying (type: {})...", update_type.label());
                // TODO: Actually download and apply
                println!("  [Auto-apply not yet implemented]");
            } else {
                println!("  -> Requires approval. Run with --yes to apply, or ignore.");
            }
            0
        }
        Ok(None) => {
            println!("Already up to date (v{}).", env!("CARGO_PKG_VERSION"));
            0
        }
        Err(e) => {
            eprintln!("Update check failed: {}", e);
            1
        }
    }
}

fn cmd_telemetry(args: &[String]) -> i32 {
    let sub = args.first().map(|s| s.as_str()).unwrap_or("status");
    let spool = crate::telemetry::Spool::new();
    match sub {
        "status" => {
            let s = settings();
            let n = spool.len();
            println!("spooled reports  {}", n);
            println!("uploading        {}", if s.telemetry.enabled && !s.telemetry.endpoint.is_empty() { "yes" } else { "no" });
            if s.telemetry.endpoint.is_empty() {
                println!("endpoint         (unset — reports stay on this machine)");
            } else {
                println!("endpoint         {}", s.telemetry.endpoint);
            }
            println!("metrics          {}", if s.telemetry.metrics { "included" } else { "excluded" });
            println!("log tail         {}", if s.telemetry.log_tail { "included" } else { "excluded" });
            println!("spool dir        {}", paths::spool_dir().display());
            println!("tracer log       {}", paths::telemetry_log().display());
            0
        }
        "list" => {
            let filter = args.get(1).cloned().unwrap_or_default();
            let all = spool.take(100);
            if all.is_empty() {
                println!("nothing spooled");
            }
            for (_, r) in all {
                if !filter.is_empty() && r.app != filter {
                    continue;
                }
                println!("{} {} {} — {}", r.app, r.version, r.ts, r.message);
            }
            0
        }
        "flush" => {
            let s = settings();
            if s.telemetry.endpoint.is_empty() {
                println!("no endpoint set; nothing to send");
                return 0;
            }
            let n = telemetry::flush(&s.telemetry, &spool);
            println!("sent {n} report(s)");
            0
        }
        "clear" => {
            let n = spool.len();
            for (p, _) in spool.take(1000) {
                spool.remove(&p);
            }
            println!("cleared {n} spooled report(s)");
            0
        }
        "endpoint" => match args.get(1) {
            Some(u) => {
                let mut s = settings();
                s.telemetry.endpoint = u.clone();
                match s.save() {
                    Ok(()) => {
                        println!("telemetry endpoint set to {u}");
                        0
                    }
                    Err(e) => {
                        eprintln!("could not save: {e}");
                        1
                    }
                }
            }
            None => {
                eprintln!("usage: artcraft-launcher telemetry endpoint URL");
                2
            }
        },
        other => {
            eprintln!("unknown telemetry subcommand: {other}");
            2
        }
    }
}

fn reveal(p: &std::path::Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(p).spawn().map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer").arg(p).spawn().map_err(|e| e.to_string())?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open").arg(p).spawn().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// A machine-readable asset plan, for debugging the asset-selection logic.
#[allow(dead_code)]
fn asset_plan() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for a in APPS {
        let rel = format!("v{}", a.pinned);
        let gh = GhKind::Portable;
        let name = match github::release(&a, &rel) {
            Ok(r) => r
                .asset_for(Platform::host(), Arch::host(), gh)
                .map(|x| x.name)
                .unwrap_or_else(|| "none".into()),
            Err(_) => "unreachable".into(),
        };
        out.push((a.slug.to_string(), name));
    }
    out
}

#[allow(dead_code)]
fn kind_of() -> InstallKind {
    InstallKind::default_for(Platform::host(), Distro::detect())
}

fn report_and_print(r: &Report) {
    if r.kind == ReportKind::Error && r.exit_code.unwrap_or(0) == 0 {
        println!("{} exited cleanly", r.app);
        return;
    }
    println!("{} {} — {}", r.app, r.version, r.message);
    for c in &r.causes {
        println!("  cause: {} ({:.0}%) — {}", c.kind, c.confidence * 100.0, c.detail);
    }
    println!("  report spooled: {}", r.id);
}
