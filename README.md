# ArtCraft Launcher

A launcher, updater and manager for the seven ArtCraft Crafting Apps — PhotoCraft, VectorCraft, FilmCraft, LightCraft, PdfCraft, EffectCraft, DesignCraft.

**Built in Rust (rustc 1.98)** — pure binary, no Electron, no webview. Works on Linux (x86_64, aarch64), Windows (x64, arm64), macOS (universal), FreeBSD. 14 MB stripped (`artcraft-launcher`). Verified against the live site and the seven GitHub releases.

```
https://github.com/abdou-da0wew/ArtCraftLauncher
https://github.com/abdou-da0wew/ArtCraftLauncher/releases/tag/v0.1.3
```

## Build from source

```sh
git clone https://github.com/abdou-da0wew/ArtCraftLauncher.git
cd ArtCraftLauncher
cargo build --release         # binary: target/release/artcraft-launcher
```

Requires Rust 1.82+, `wgpu` (GPU backend, falls back gracefully if unavailable — CLI always works), and the fonts included in `assets/fonts/` (Archivo, Inter, Geist Mono, Instrument Serif).

For a **static binary with zero glibc dependency** (works on older Linux):

```sh
rustup target add x86_64-unknown-linux-musl
RUSTFLAGS='-C target-feature=+crt-static' cargo build --release --target x86_64-unknown-linux-musl
```

The `Dockerfile.musl` does this automatically; `Dockerfile.build` builds for standard Ubuntu 20.04 (glibc 2.31-compatible) with `rust:1.82-slim`.

## What's inside

The launcher manages all seven Crafting Apps, handles versions, channels (stable/beta/nightly), downloads from GitHub (with SHA256 checks), installs portable trees, rolls back instantly, and writes `.desktop`/`.desktop` Actions + MIME + icons + Start Menu + macOS bundle + Launch Services — all automatically at first run.

**Self-updater**: `artcraft-launcher self-update` checks `abdou-da0wew/ArtCraftLauncher/releases/latest` via GitHub API, reads the tag (`v0.1.3`), classifies (`Critical`/`Patch` = apply, `Minor`/`Major`/`Beta` = ask), downloads the correct platform asset, verifies checksum against `SHA256SUMS.txt`, and swaps the binary atomically (old → `.old`, new → current, then clean).

**Crash telemetry**: every crash/failed launch sends a JSON report to a configurable endpoint (or stays in `~/.cache/opencode/scratch/artcraft/ltest/state/telemetry/spool/`). Captures exit code, signal, pre-crash trail, device metrics (CPU, RAM, disk, uptime, cores), and inferred causes (OOM, GPU init failure, permissions, missing file, unsupported input, panic, broken-install <1.5s after launch).

**Headless CLI** (no GPU): `list`, `check`, `install`, `update`, `launch`, `rollback`, `backup`, `restore`, `wipe`, `register`, `export`, `import`, `telemetry`, `doctor`.

## Directories

- Launcher state: `~/.local/share/artcraft-launcher` (Linux) / `~/Library/Application Support/ArtCraft Studio` (macOS) / `%LOCALAPPDATA%\ArtCraft` (Windows)
- Per-app installs: `<launcher_home>/apps/<slug>/<version>/`
- Per-app data (matches what apps use): `~/.local/share/<slug>` / `~/Library/Application Support/<slug>` / `%APPDATA%\<slug>`
- Backups: `<launcher_home>/backups/<slug>/<timestamp>/`
- Logs (app): `<data_dir>/logs/<slug>.log`
- Spool (crash reports): `<launcher_home>/state/telemetry/spool/`

## Documentation

- `AGENTS.md` — agent instructions (this repo)
- `README.md` — this file
- `assets/shaders/hero.wgsl` — the hero field shader (port from site's GLSL, validated)
- `dist/flatpak-manifest.yaml` — Flatpak bundle definition
- `build.rs` — Windows icon embedding at build time via `winres`

## Icon

The launcher uses the official ArtCraft mark (`https://getartcraft.com/artcraft-icon.svg`) — the site's own SVG — rendered via `tiny_skia` (no image library needed). It reserves `.ico` (Windows), `.icns` (macOS), and PNG (Linux icons/hicolor) from that same source.

## License

MIT OR Apache-2.0. Third-party assets keep their own licenses (fonts: SIL OFL; icons: Lucide/ISC; artwork: public domain). See `NOTICE`, `ATTRIBUTION.md`.
