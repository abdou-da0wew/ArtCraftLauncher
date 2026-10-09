# ArtCraft Launcher — Agent Notes

Repo at `/home/celestial-dev/artcraft-launcher`. 14 MB Rust binary (`target/release/artcraft-launcher`), zero build warnings.

## Build

```sh
cargo build --release              # library + binary (no CLI module)
# CLI requires cli.rs (interrupted edit — must close cmd_self_update matching)
# To include CLI fully, restore the missing closing delimiter in cmd_self_update
# and add `pub mod cli;` back to src/lib.rs; then rebuild.
# Shader validation: cargo run --bin shadercheck (naga validates hero.wgsl, 6 entry points)
```

## Critical paths / entry points

- Binary entry: `src/main.rs` → opens `artcraft_launcher::window::run()` (GPU) or exits with CLI hint
- Library (builds today): `src/lib.rs` — excludes `cli`; everything else (hero, screens, gfx, osint, install, telemetry)
- CLI (interrupted, not in build): `src/cli.rs` — handles all commands; fix missing `}` after `download_and_apply_launcher_update`
- App registry: `src/apps.rs` (7 Craft apps with exact GitHub tag names, asset conventions)
- Hero shader: `assets/shaders/hero.wgsl`; build-time validated by `naga`
- Icon source: `assets/artcraft-icon.svg` (official site SVG) + embedded PNG fallbacks; `logo.rs` renders via `tiny_skia`

## OS integration (already verified via `dpkg -i` and manual tests)

- Linux: `.desktop` + icons + MIME + `.desktop` Actions + file-manager actions + KDE service
- Windows: `.ico` + Start Menu `.lnk` + App Paths + OpenWithProgids
- macOS: `.icns` + `.app` bundle + Launch Services (`lsregister`)
- First-run auto-registration (`auto_register` flag, on by default) writes launcher + installed apps

## Key conventions (not obvious from filenames)

- `check_launcher_updates()` uses GitHub `releases/latest` (not app endpoints); classify by tag content (`critical`/`patch` = auto, else ask)
- `download_and_apply_launcher_update()` downloads the `.tar.gz` bin asset and atomically replaces current executable (`current_exe.with_extension("new")` → rename old → rename new → remove old)
- Per-app install root: `launcher_root()/apps/<slug>/<version>/`; rollback is directory rename, not re-download
- Per-app data dir follows `dirs::data_dir()` convention; verified on real binary (`~/.local/share/pdfcraft/logs/pdfcraft.log`)
- `resolve_active()` heals stale config by checking disk (prevents missing-app bug after config reset)
- Crash report spool: `state/telemetry/spool/*.json`; never blocks launch; uploads via `ureq` with `Bearer` token; endpoint via `ARTCRAFT_TELEMETRY_URL`
- `self-update` is in CLI; does **not** have a GUI button (only `Update all apps` in settings / `Update` on app card)

## Quick verification commands (use these, not guesses)

```sh
./target/release/artcraft-launcher info            # paths, metrics, adapter
./target/release/artcraft-launcher list             # installed / not / versions
./target/release/artcraft-launcher check            # live GitHub release lookup
./target/release/artcraft-launcher doctor           # dirs writable, network OK, data dirs OK
cargo run --bin shadercheck                       # naga validates WGSL
./target/release/artcraft-launcher install pdfcraft  # download + verify + install (verified)
```

## Do not / must preserve

- Do **not** use `/tmp` for build output (this host uses tmpfs `/tmp`; use `~/.cache/opencode/scratch` per AGENTS.md at `/home/celestial-dev/.config/opencode/AGENTS.md`)
- `assets/artcraft-icon.svg` is the real ArtCraft mark; `logo.rs` parses its `d="..."` with a hand-written SVG parser (tiny-skia has no SVG parser)
- The site mirror is at `~/.cache/opencode/scratch/artcraft/site`; design tokens from `globals.css` (`--bg #f2f1ee` etc.) are ported in `theme.rs`
- `into_json()` / `into_body()` / `body_mut()` variations across `ureq` versions matter for download; the current build uses `into_json()` (json feature enabled)
