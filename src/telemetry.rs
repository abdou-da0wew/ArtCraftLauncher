//! A very small, dependency-light telemetry tracer.
//!
//! What it does:
//!  * attaches to a launched app as a child process and records its exit
//!  * tails `<data>/logs/<slug>.log` so the last N lines travel with the report
//!  * samples device resource metrics at launch and at exit
//!  * records the launcher's own recent actions as *pre-crash state*
//!  * infers probable causes from the exit code and log patterns
//!  * spools every report as JSONL on disk and uploads them with backoff
//!
//! Design rules, in order of importance:
//!  1. It must never block or delay launching an app.
//!  2. It must never lose a report — if the network is down it stays spooled.
//!  3. It must be honest about what it did not capture.
//!  4. It must be possible to turn it off entirely, and to take a single app
//!     out of tracing without touching the rest.

use std::collections::VecDeque;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::config::TelemetryConfig;
use crate::paths;

pub const SPOOL_KEEP: usize = 500;

pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

// --------------------------------------------------------------- metrics

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeviceMetrics {
    pub unix: u64,
    pub os: String,
    pub arch: String,
    /// Whole-machine CPU load in 0..1, if available.
    pub cpu_load: Option<f32>,
    /// Physical memory total/available, bytes.
    pub mem_total: Option<u64>,
    pub mem_available: Option<u64>,
    /// Free/total bytes on the launcher's data volume.
    pub disk_free: Option<u64>,
    pub disk_total: Option<u64>,
    /// Number of running processes, if enumerable.
    pub procs: Option<u32>,
    /// Uptime in seconds.
    pub uptime: Option<u64>,
    /// `true` if we could not read anything (e.g. sandboxed).
    pub degraded: bool,
    pub cores: u32,
    pub gpu: Option<String>,
}

impl DeviceMetrics {
    pub fn sample() -> Self {
        let mut m = DeviceMetrics {
            unix: now_unix(),
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            cores: std::thread::available_parallelism()
                .map(|n| n.get() as u32)
                .unwrap_or(0),
            degraded: false,
            ..Default::default()
        };
        #[cfg(target_os = "linux")]
        {
            m.cpu_load = linux_cpu_load();
            let (t, a) = linux_mem();
            m.mem_total = t;
            m.mem_available = a;
            m.procs = linux_procs();
            m.uptime = linux_uptime();
            m.disk_free = statvfs_free();
            m.disk_total = statvfs_total();
        }
        #[cfg(target_os = "macos")]
        {
            m.cpu_load = macos_load();
            let (t, a) = macos_mem();
            m.mem_total = t;
            m.mem_available = a;
            m.uptime = macos_uptime();
            m.disk_free = statvfs_free();
            m.disk_total = statvfs_total();
        }
        #[cfg(target_os = "windows")]
        {
            // Windows metrics are gathered by the OS integration layer; we do
            // not shell out to PowerShell from a launcher that must stay light.
            m.degraded = true;
        }
        if m.cpu_load.is_none() && m.mem_total.is_none() {
            m.degraded = true;
        }
        m
    }

    /// Fraction of physical memory in use, if known.
    pub fn mem_used_frac(&self) -> Option<f32> {
        match (self.mem_total, self.mem_available) {
            (Some(t), Some(a)) if t > 0 => Some((t - a) as f32 / t as f32),
            _ => None,
        }
    }
}

#[cfg(target_os = "linux")]
fn linux_cpu_load() -> Option<f32> {
    let s = std::fs::read_to_string("/proc/stat").ok()?;
    let line = s.lines().next()?;
    let mut it = line.split_whitespace().skip(1);
    let mut total = 0f64;
    let mut idle = 0f64;
    let mut i = 0;
    for v in it.by_ref() {
        let n: f64 = v.parse().ok()?;
        if i == 3 || i == 4 {
            idle += n;
        }
        total += n;
        i += 1;
    }
    if total <= 0.0 {
        return None;
    }
    Some((1.0 - idle / total) as f32)
}

#[cfg(target_os = "linux")]
fn linux_mem() -> (Option<u64>, Option<u64>) {
    let s = match std::fs::read_to_string("/proc/meminfo") {
        Ok(s) => s,
        Err(_) => return (None, None),
    };
    let mut total = None;
    let mut avail = None;
    for l in s.lines() {
        if let Some(v) = l.strip_prefix("MemTotal:") {
            total = v.split_whitespace().next().and_then(|x| x.parse::<u64>().ok()).map(|k| k * 1024);
        } else if let Some(v) = l.strip_prefix("MemAvailable:") {
            avail = v.split_whitespace().next().and_then(|x| x.parse::<u64>().ok()).map(|k| k * 1024);
        }
    }
    (total, avail.or(total))
}

#[cfg(target_os = "linux")]
fn linux_procs() -> Option<u32> {
    let n = std::fs::read_dir("/proc")
        .ok()?
        .flatten()
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .chars()
                .all(|c| c.is_ascii_digit())
        })
        .count();
    Some(n as u32)
}

#[cfg(target_os = "linux")]
fn linux_uptime() -> Option<u64> {
    std::fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|s| s.split('.').next().and_then(|x| x.parse::<u64>().ok()))
}

#[cfg(target_os = "macos")]
fn macos_load() -> Option<f32> {
    // `sysctl -n vm.loadavg` → "{ 1.23 4.56 7.89 }"
    let out = sh("sysctl", &["-n", "vm.loadavg"])?;
    let inner = out.trim().trim_start_matches('{').trim_end_matches('}');
    let l1 = inner.split_whitespace().next()?.parse::<f32>().ok()?;
    Some((l1 / cores() as f32).clamp(0.0, 1.0))
}

#[cfg(target_os = "macos")]
fn macos_mem() -> (Option<u64>, Option<u64>) {
    let out = match sh("sysctl", &["-n", "hw.memsize"]) {
        Some(s) => s,
        None => return (None, None),
    };
    let total = out.trim().parse::<u64>().ok();
    let avail = sh("vm_stat", &[]).and_then(|s| {
        let free = s
            .lines()
            .find(|l| l.starts_with("Pages free:"))
            .and_then(|l| l.split(':').nth(1))
            .and_then(|v| v.trim().trim_end_matches('.').parse::<u64>().ok())?;
        let ps = s
            .lines()
            .find(|l| l.starts_with("Mach Virtual Memory Statistics"))
            .and_then(|_| Some(4096u64))?;
        Some(free * ps)
    });
    (total, avail)
}

#[cfg(target_os = "macos")]
fn macos_uptime() -> Option<u64> {
    let out = sh("sysctl", &["-n", "kern.boottime"])?;
    let sec = out.split("sec = ").nth(1)?.split(',').next()?.trim();
    let boot: u64 = sec.parse().ok()?;
    now_unix().checked_sub(boot)
}

#[cfg(any(target_os = "macos"))]
fn cores() -> u32 {
    std::thread::available_parallelism().map(|n| n.get() as u32).unwrap_or(1)
}

#[cfg(unix)]
fn statvfs_of(p: &Path) -> Option<(u64, u64)> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(p.as_os_str().as_bytes()).ok()?;
    unsafe {
        let mut st: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c.as_ptr(), &mut st) != 0 {
            return None;
        }
        let bsize = st.f_frsize as u64;
        Some((st.f_bfree as u64 * bsize, st.f_blocks as u64 * bsize))
    }
}

#[cfg(unix)]
fn statvfs_free() -> Option<u64> {
    statvfs_of(Path::new("/")).map(|(f, _)| f)
}
#[cfg(unix)]
fn statvfs_total() -> Option<u64> {
    statvfs_of(Path::new("/")).map(|(_, t)| t)
}

// ----------------------------------------------------------------- events

/// A single reported event. Errors and crashes share the envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub id: String,
    pub ts: u64,
    pub ts_ms: u128,
    pub kind: ReportKind,
    /// App slug, or `launcher` for the launcher itself.
    pub app: String,
    pub version: String,
    /// Launcher version that produced the report.
    pub launcher: String,
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub signal: Option<i32>,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub stack: Vec<String>,
    /// Last N lines of the app's log at the moment of the event.
    #[serde(default)]
    pub log_tail: Vec<String>,
    /// Launcher-side record of what the user did just before.
    #[serde(default)]
    pub pre_crash: Vec<String>,
    /// Device resource metrics sampled at exit.
    #[serde(default)]
    pub metrics: Option<DeviceMetrics>,
    /// Heuristic probable causes, most confident first.
    #[serde(default)]
    pub causes: Vec<Cause>,
    /// How long the app ran before the event, ms.
    #[serde(default)]
    pub ran_ms: u128,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReportKind {
    Crash,
    Error,
    LaunchFailed,
    Oom,
    UpdateFailed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cause {
    pub kind: String,
    pub confidence: f32,
    pub detail: String,
}

/// Ring of recent launcher actions, used as pre-crash state.
pub struct Trail {
    inner: VecDeque<(u128, String)>,
    cap: usize,
}

impl Default for Trail {
    fn default() -> Self {
        Self::new(24)
    }
}

impl Trail {
    pub fn new(cap: usize) -> Self {
        Self {
            inner: VecDeque::with_capacity(cap),
            cap,
        }
    }
    pub fn push(&mut self, s: impl Into<String>) {
        if self.inner.len() == self.cap {
            self.inner.pop_front();
        }
        self.inner.push_back((now_ms(), s.into()));
    }
    pub fn recent(&self, n: usize) -> Vec<String> {
        self.inner
            .iter()
            .rev()
            .take(n)
            .map(|(t, s)| format!("[{}] {}", t / 1000, s))
            .collect()
    }
}

/// A running app being watched.
pub struct Watched {
    pub slug: String,
    pub version: String,
    pub child: std::process::Child,
    pub started_ms: u128,
    pub metrics: DeviceMetrics,
    pub log_path: PathBuf,
    pub log_offset: u64,
    pub pre_crash: Vec<String>,
}

impl Watched {
    /// Poll the child; returns `Some(Report)` if it exited.
    pub fn poll(&mut self, cfg: &TelemetryConfig, trail: &Trail) -> Option<Report> {
        let status = match self.child.try_wait() {
            Ok(Some(s)) => s,
            _ => return None,
        };
        let code = status.code();
        #[cfg(unix)]
        let signal = {
            use std::os::unix::process::ExitStatusExt;
            status.signal()
        };
        #[cfg(not(unix))]
        let signal: Option<i32> = None;
        let ran = now_ms().saturating_sub(self.started_ms);
        let metrics = DeviceMetrics::sample();
        let log_tail = if cfg.log_tail {
            tail_file(&self.log_path, self.log_offset, self.started_ms, 40)
        } else {
            Vec::new()
        };
        let mut report = Report {
            id: format!("{}-{}-{}", self.slug, self.version, now_ms()),
            ts: now_unix(),
            ts_ms: now_ms(),
            kind: classify(code, signal),
            app: self.slug.clone(),
            version: self.version.clone(),
            launcher: env!("CARGO_PKG_VERSION").to_string(),
            exit_code: code,
            signal,
            message: String::new(),
            stack: Vec::new(),
            log_tail,
            pre_crash: {
                self.pre_crash = trail.recent(12);
                self.pre_crash.clone()
            },
            metrics: if cfg.metrics { Some(metrics) } else { None },
            causes: Vec::new(),
            ran_ms: ran,
        };
        report.message = summary(code, signal, &report.log_tail);
        report.stack = stack_from(&report.log_tail);
        report.causes = causes(code, signal, &report.log_tail, &report.metrics, ran);
        Some(report)
    }
}

fn classify(code: Option<i32>, signal: Option<i32>) -> ReportKind {
    match (code, signal) {
        (Some(0), _) => return ReportKind::Error, // handled by caller
        (_, Some(s)) if s == 9 || s == 6 => ReportKind::Crash,
        (_, Some(_)) => ReportKind::Crash,
        (Some(c), _) if c >= 128 => ReportKind::Crash,
        (Some(101), _) => ReportKind::Crash, // Rust panic → exit 101
        _ => ReportKind::Error,
    }
}

fn summary(code: Option<i32>, signal: Option<i32>, log: &[String]) -> String {
    if let Some(s) = signal {
        return format!("terminated by signal {s} ({})", signal_name(s));
    }
    if let Some(c) = code {
        if c == 101 {
            // Rust panics print "thread '<n>' panicked at ..."; find it.
            for l in log.iter().rev() {
                if l.contains("panicked at") || l.contains("panicked") {
                    return l.trim().to_string();
                }
            }
            return "process aborted with status 101 (panic)".into();
        }
        if c != 0 {
            return format!("exited with status {c}");
        }
    }
    "clean exit".into()
}

fn signal_name(s: i32) -> &'static str {
    match s {
        4 => "SIGILL (illegal instruction)",
        6 => "SIGABRT (abort)",
        7 => "SIGBUS (bus error)",
        9 => "SIGKILL (killed — often the OOM reaper)",
        11 => "SIGSEGV (segmentation fault)",
        13 => "SIGPIPE (broken pipe)",
        15 => "SIGTERM",
        _ => "unknown",
    }
}

/// Recover a stack trace from the log tail if the app printed one.
fn stack_from(log: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut in_stack = false;
    for l in log.iter().rev() {
        if l.contains("stack backtrace") || l.contains("   at ") || l.contains("RUST_BACKTRACE") {
            in_stack = true;
        }
        if in_stack && l.trim_start().starts_with("at ") {
            out.push(l.trim().to_string());
            if out.len() >= 24 {
                break;
            }
        }
    }
    out
}

/// Probable causes, most confident first. This is inference, not certainty,
/// so every cause carries its own confidence and the evidence for it.
fn causes(
    code: Option<i32>,
    signal: Option<i32>,
    log: &[String],
    metrics: &Option<DeviceMetrics>,
    ran_ms: u128,
) -> Vec<Cause> {
    let mut out: Vec<Cause> = Vec::new();
    let push = |out: &mut Vec<Cause>, kind: &str, confidence: f32, detail: String| {
        out.push(Cause {
            kind: kind.into(),
            confidence,
            detail,
        });
    };

    // OOM: SIGKILL with high memory pressure, or a log line about allocation.
    let mem = metrics.as_ref().and_then(|m| m.mem_used_frac()).unwrap_or(0.0);
    if signal == Some(9) && mem > 0.9 {
        push(
            &mut out,
            "oom",
            0.9,
            format!(
                "killed by SIGKILL while physical memory was at {:.0}%; the OOM reaper or a cgroup limit is the likely culprit",
                mem * 100.0
            ),
        );
    } else if signal == Some(9) {
        push(
            &mut out,
            "external-kill",
            0.5,
            "killed by SIGKILL with no memory pressure — something outside the app asked it to stop".into(),
        );
    }

    for l in log {
        let ll = l.to_ascii_lowercase();
        if ll.contains("cannot allocate") || ll.contains("out of memory")
            || ll.contains("memory allocation of")
        {
            push(
                &mut out,
                "oom",
                0.85,
                "the app's own log reports an allocation failure".into(),
            );
        }
        if ll.contains("no adapter") || ll.contains("failed to create device")
            || ll.contains("wgpu") && ll.contains("error")
        {
            push(
                &mut out,
                "gpu-init",
                0.8,
                "GPU initialisation failed; the app could not create a rendering device".into(),
            );
        }
        if (ll.contains("permission denied") || ll.contains("access is denied"))
            && (ll.contains("write") || ll.contains("create"))
        {
            push(
                &mut out,
                "permissions",
                0.75,
                "a write was refused — the data directory or a file is not writable".into(),
            );
        }
        if ll.contains("no such file") || ll.contains("not found") {
            push(
                &mut out,
                "missing-file",
                0.4,
                "the app references a file that is not present".into(),
            );
        }
        if ll.contains("invalid input") || ll.contains("unsupported") {
            push(
                &mut out,
                "unsupported-input",
                0.35,
                "an input file was not understood".into(),
            );
        }
    }

    // A crash within a second of launch is almost always a broken install.
    if ran_ms < 1500 && signal.is_some() {
        push(
            &mut out,
            "broken-install",
            0.6,
            format!(
                "crashed {ran_ms} ms after launch — before any user interaction; consider reinstalling"
            ),
        );
    }

    // Missing or unreadable system libraries is the usual Linux cause.
    if matches!(signal, Some(4) | Some(11) | Some(7)) {
        push(
            &mut out,
            "native-crash",
            0.7,
            format!("fatal signal {} immediately after a native operation", signal.unwrap()),
        );
    }

    if out.is_empty() {
        if code == Some(101) {
            push(
                &mut out,
                "panic",
                0.95,
                "a Rust panic unwound the app; the message above is the panic site".into(),
            );
        } else if code.is_some() && code != Some(0) {
            push(
                &mut out,
                "nonzero-exit",
                0.3,
                format!("the app chose to exit with status {}", code.unwrap()),
            );
        }
    }

    out.sort_by(|a, b| b.confidence.total_cmp(&a.confidence));
    out.truncate(6);
    out
}

// ------------------------------------------------------------------ spool

pub struct Spool {
    dir: PathBuf,
}

impl Spool {
    pub fn new() -> Self {
        Self {
            dir: paths::spool_dir(),
        }
    }

    pub fn put(&self, r: &Report) {
        let _ = std::fs::create_dir_all(&self.dir);
        let name = format!("{}.json", r.id.replace('/', "_"));
        if let Ok(s) = serde_json::to_string(r) {
            let _ = std::fs::write(self.dir.join(name), s);
        }
        self.enforce_cap();
    }

    pub fn take(&self, max: usize) -> Vec<(PathBuf, Report)> {
        let mut out = Vec::new();
        let Ok(rd) = std::fs::read_dir(&self.dir) else {
            return out;
        };
        let mut files: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
        files.sort();
        for f in files.into_iter().take(max) {
            if f.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Ok(s) = std::fs::read_to_string(&f) {
                if let Ok(r) = serde_json::from_str::<Report>(&s) {
                    out.push((f, r));
                }
            }
        }
        out
    }

    pub fn remove(&self, p: &Path) {
        let _ = std::fs::remove_file(p);
    }

    pub fn len(&self) -> usize {
        self.take(SPOOL_KEEP).len()
    }

    fn enforce_cap(&self) {
        let files = self.take(SPOOL_KEEP + 1);
        let over = files.len().saturating_sub(SPOOL_KEEP);
        if over > 0 {
            for (p, _) in files.into_iter().rev().take(over) {
                self.remove(&p);
            }
        }
    }
}

impl Default for Spool {
    fn default() -> Self {
        Self::new()
    }
}

/// Upload spooled reports. Retries with backoff are handled by the caller;
/// this returns how many were accepted.
pub fn flush(cfg: &TelemetryConfig, spool: &Spool) -> usize {
    if !cfg.enabled || cfg.endpoint.is_empty() {
        return 0;
    }
    let mut sent = 0;
    for (p, r) in spool.take(8) {
        let body = match serde_json::to_vec(&r) {
            Ok(b) => b,
            Err(_) => {
                spool.remove(&p);
                continue;
            }
        };
        let ok = post(cfg, &body);
        if ok {
            spool.remove(&p);
            sent += 1;
        } else {
            // Leave it spooled; the next tick will retry.
            break;
        }
    }
    sent
}

fn post(cfg: &TelemetryConfig, body: &[u8]) -> bool {
    
    let agent = AgentBuilder::new()
        .user_agent(&crate::github::user_agent())
        .timeout(std::time::Duration::from_secs(15))
        .build();
    let mut req = agent
        .post(&cfg.endpoint)
        .set("content-type", "application/json");
    if !cfg.token.is_empty() {
        req = req.set("authorization", &format!("Bearer {}", cfg.token));
    }
    match req.send_bytes(body) {
        Ok(r) => r.status() < 400,
        Err(_) => false,
    }
}

struct AgentBuilder;

/// Tiny builder so `post` reads cleanly without a use of `ureq::AgentBuilder`
/// (which exists, this just keeps the call site explicit).
impl AgentBuilder {
    pub fn new() -> ureq::AgentBuilder {
        ureq::AgentBuilder::new()
    }
}

/// Append to the launcher's own tracer log.
pub fn log_event(kind: &str, msg: &str) {
    let p = paths::telemetry_log();
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&p) {
        use std::io::Write;
        let _ = writeln!(f, "{} {} {}", now_unix(), kind, msg);
    }
}

/// Alias kept for callers that prefer a defaulting signature.
pub fn now_unix_or(d: u64) -> u64 {
    let v = now_unix();
    if v == 0 { d } else { v }
}

/// Last `n` lines of an app's log, taken as of the launch.
///
/// `from` is the byte offset captured *before* the app started and `since_ms`
/// the launch time in Unix ms. If the file was rotated or rewritten since
/// launch (its length shrank, or its mtime is newer than the launch), the
/// current file is read from the start — otherwise only what was appended is
/// returned. The Craft apps rotate their log per launch, so this matters.
pub fn tail_file(path: &Path, from: u64, since_ms: u128, n: usize) -> Vec<String> {
    let mut f = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };
    let md = match f.metadata() {
        Ok(m) => m,
        Err(_) => return Vec::new(),
    };
    let len = md.len();
    let mtime_ms = md
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let rotated = len < from;
    let rewritten = mtime_ms > since_ms;
    let start = if rotated || rewritten { 0 } else { from.min(len) };
    if f.seek(SeekFrom::Start(start)).is_err() {
        return Vec::new();
    }
    let mut s = String::new();
    if f.read_to_string(&mut s).is_err() {
        return Vec::new();
    }
    s.lines()
        .map(|l| l.to_string())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .take(n)
        .rev()
        .collect()
}
