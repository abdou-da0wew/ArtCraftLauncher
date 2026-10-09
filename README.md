# ArtCraft Studio

A launcher, updater and manager for the seven ArtCraft apps — written in Rust,
rendered with `wgpu`, and styled as a 1:1 port of [getartcraft.com](https://getartcraft.com).

```
~14 MB binary · no webview · no Electron · works headless or on the GPU
```

---

## What it does

**Manages all seven apps**

| # | App | GitHub |
|---|---|---|
| 01 | PhotoCraft | `storytold/photocraft` |
| 02 | VectorCraft | `storytold/vectorcraft` |
| 03 | FilmCraft | `storytold/filmcraft` |
| 04 | LightCraft | `storytold/lightcraft` |
| 05 | PdfCraft | `storytold/pdfcraft` |
| 06 | EffectCraft | `storytold/effectcraft` |
| 07 | DesignCraft | `storytold/designcraft` |

* Tracks installed versions, and resolves the newest release straight from
  GitHub (`/releases/latest`), so it is always current.
* Prefers the **portable** release asset (`<slug>-<ver>-linux-x86_64.tar.gz`,
  `…-windows-x64-portable.zip`), extracts it into a launcher-owned tree, and
  verifies the download against the release's `SHA256SUMS.txt`. Because the
  launcher owns the tree, switching versions is a directory rename and rolling
  back is instant.
* On macOS, where there is no portable asset, it mounts the `.dmg`, copies
  `<Bundle>.app` into `~/Applications/ArtCraft`, and parks the previous version
  as `<Bundle>-<version>.app` so a rollback is a rename too.
* Release channels: `stable` / `beta` / `nightly`, per app.
* Data directories are managed per app, following the same convention the apps
  themselves use (`~/.local/share/<slug>`, `~/Library/Application Support/<slug>`,
  `%APPDATA%\<slug>`) — verified by running PdfCraft with a sandboxed `$HOME`.

**Update policy: the launcher always asks.**

It never installs anything unasked. When a newer release exists it shows a
prompt with three answers:

* **Update now**
* **Not now** — deferred, asked again at the next launch
* **Don't ask** — that exact version is muted, permanently

`update` on the CLI prints the plan and waits for `--yes`.

**Per-app settings, backup and export**

* Back up an app's data dir (optionally with a note) into a timestamped
  directory with a manifest.
* Restore a backup (a pre-restore safety copy is taken first).
* Wipe a data dir — always after taking a safety copy.
* Launch parameters: extra CLI arguments, extra environment variables, and
  files to open. `artcraft-launcher open <app> -- --flag value`.
* Export writes one portable JSON file containing the launcher preferences,
  every app's launch parameters, the data-directory manifest and the backup
  index. Import applies it, rewriting paths to the target machine's
  conventions.

**Crash tracer**

A small, dependency-light tracer built for this job:

* attaches to each launched app as a child process and records its exit
* captures the exit status **and** the signal, and unwinds Rust's exit `101`
  into the panic message and a stack trace where the app printed one
* tails the app's own log (`<data>/logs/<slug>.log`) with rotation awareness,
  so the last N lines travel with the report
* records the launcher's recent actions as **pre-crash state**
* infers **probable causes** with a confidence each: OOM (SIGKILL under memory
  pressure, or an allocation failure in the log), GPU initialisation failure,
  read-only data dir, missing file, unsupported input, panic, and a crash
  within 1.5 s of launch, which is almost always a broken install
* samples **device resource metrics**: CPU load, memory total/available/used,
  disk free/total, process count, uptime, core count
* spools every report as JSONL on disk, then uploads with backoff and a
  `Bearer` token, retrying forever until the network returns. Nothing is ever
  lost and nothing ever blocks a launch or an app's exit.
* can be turned off entirely, and metrics or the log tail can be excluded
  independently

Set the endpoint with:

```sh
export ARTCRAFT_TELEMETRY_URL=https://telemetry.example.com/v1/crashes
export ARTCRAFT_TELEMETRY_TOKEN=…   # optional
```

or `artcraft-launcher telemetry endpoint https://…`.

**OS integration — icons reserved on every platform**

* **Linux** — a `.desktop` entry per app in `~/.local/share/applications`, the
  vendor's hicolor icons (SVG + every raster size the release ships) into
  `~/.local/share/icons`, `mimeapps.list` and `mimeinfo.cache` associations,
  and **right-click entries**: `Actions=` blocks (`Open with`, `Open in a new
  window`, `Send to`) plus `file-manager-actions` companion files and a KDE
  service menu, so Nautilus, Dolphin, Nemo, Caja and Thunar all offer them.
* **Windows** — a multi-size `.ico` per app in `%LOCALAPPDATA%\ArtCraft\icons`
  (one for the launcher itself is baked into the `.exe` at build time, so the
  taskbar and Explorer show the ArtCraft mark), Start Menu `.lnk` shortcuts
  pointing at the reserved icon, `App Paths` registry entries, and
  per-extension `OpenWithProgids` associations for Explorer's right-click.
* **macOS** — the `.icns` from each `.app` bundle reserved to
  `~/Library/Application Support/ArtCraft Studio/icons`, an `Info.plist` for
  the launcher's own `.app` wrapper with `CFBundleIconFile` and document types,
  ad-hoc signing, and `lsregister` so Launch Services sees everything
  immediately.

## The interface

The screen is a hand-written port of the site's own design system:

* The exact token set from `globals.css` — `--bg #f2f1ee`, `--ink #101014`,
  `--line rgba(16,16,20,.16)`, `--accent #2d81ff`, `--danger #b3261e`, and the
  whole dark counterpart — cross-fading over 0.4 s on theme switch exactly as
  the site's `@property` transitions do.
* Per-app accent colours, from `.app-<slug>`.
* The four real typefaces: Archivo at weight 620 / stretch 118 % for display,
  Inter for body, Geist Mono for the HUD labels, Instrument Serif for the
  italic accents. Variable-font instances are applied, not approximated.
* Zero corner radius, hairline grids, `.tick` crosshairs, sticky section
  eyebrows, the `36s linear infinite` capability ticker, `.invert-block` hover
  flips, and the scroll-ruler instrument with its frost rail, tick rows, accent
  needle and odometer readout.
* The hero field: a WebGL galaxy of bowed cards carrying the seven apps'
  screenshots, with the mouse bulge, four-slot click ripples, 12-tap Poisson
  blur, edge-quadratic chromatic aberration, SDF rounded frames, comet ribbon
  arms de-phased by the golden-ratio conjugate, and the intro's radial reveal
  wave — all ported from the site's GLSL into WGSL verbatim.
* The intro choreography: one monotonic clock against named thresholds
  (`copyAt`, `cardsAt`), `y 28 → 0`, `blur 8 → 0`, `power3.out` at 0.9 s, the
  rule wipe at 0.7 s `power2.out`, the tick draws at 0.35 s staggered 0.06 s,
  and the left-to-right group wash `0.18 × (childLeft − groupLeft)/groupWidth`.
* A frame-time EMA governor that lowers card counts rather than dropping
  frames on a weak GPU.
* Light and dark, both verified.

Every shader is validated at build time by `naga` — the same front-end and
validator wgpu uses — so a WGSL mistake is a compile error rather than a blank
window on someone else's machine:

```sh
cargo run --bin artcraft-shadercheck
# hero.wgsl: parse + validate OK (6 entry points)
```

## Building

```sh
cargo build --release
./target/release/artcraft-launcher            # GUI
./target/release/artcraft-launcher --help     # /tmp
```

Cross-compiling for the three platforms:

```sh
rustup target add x86_64-pc-windows-msvc aarch64-apple-darwin aarch64-unknown-linux-gnu
cargo build --release --target x86_64-pc-windows-msvc
cargo build --release --target aarch64-apple-darwin
```

The Windows build embeds `assets/launcher.ico` into the `.exe` via `build.rs`
(guarded on the *target*, not the host, so Linux and macOS builds are
untouched).

## CLI

Everything works without a GPU or a display:

```sh
artcraft-launcher info                       # paths, platform, live metrics
artcraft-launcher list                       # every app, version, data size
artcraft-launcher check                      # ask GitHub for the newest releases
artcraft-launcher install pdfcraft           # download + verify + extract
artcraft-launcher update --all --yes         # print the plan, then install
artcraft-launcher launch vectorcraft -- a.svg
artcraft-launcher versions pdfcraft          # what is installed, what is active
artcraft-launcher rollback pdfcraft
artcraft-launcher backup lightcraft -n "raw library"
artcraft-launcher restore lightcraft 1791558104
artcraft-launcher wipe pdfcraft --yes        # backs up first
artcraft-launcher data pdfcraft              # print the data dir
artcraft-launcher data-open pdfcraft         # reveal it in the file manager
artcraft-launcher register-all              # desktop entries, icons, right-click
artcraft-launcher unregister pdfcraft
artcraft-launcher export -o settings.json
artcraft-launcher import settings.json
artcraft-launcher telemetry status
artcraft-launcher telemetry list pdfcraft
artcraft-launcher telemetry flush
artcraft-launcher telemetry endpoint https://…
artcraft-launcher doctor
```

## Environment overrides

| Variable | Meaning |
|---|---|
| `ARTCRAFT_LAUNCHER_HOME` | Where the launcher keeps its own state |
| `ARTCRAFT_DATA_HOME` | Override the parent of every app's data dir |
| `ARTCRAFT_CONFIG_HOME`, `ARTCRAFT_CACHE_HOME`, `ARTCRAFT_STATE_HOME` | As the names say |
| `ARTCRAFT_ICON_DIR` | Where reserved icons are written |
| `ARTCRAFT_MAC_APPS` | Where macOS app bundles are installed |
| `ARTCRAFT_WIN_ROOT`, `ARTCRAFT_ICON_DIR` | Windows install/icon roots |
| `ARTCRAFT_TELEMETRY_URL`, `ARTCRAFT_TELEMETRY_TOKEN` | Crash upload endpoint and bearer token |
| `<SLUG>_DATA_DIR` | Per-app data dir override |
| `<SLUG>_LOG_DIR` | Per-app log dir override |
| `ARTCRAFT_PREFER_SYSTEM_PACKAGES=1` | Prefer deb/rpm/msi over a portable tree |

## Layout

```
src/
  theme.rs      the design tokens, verbatim from globals.css
  apps.rs       the seven apps, their copy, their asset naming
  gfx/canvas.rs a CPU canvas with subpixel-accurate hairlines
  gfx/text.rs   the four typefaces, variable instances, shaping, glyph cache
  anim.rs       the intro clock, reveals, easings, odometer, marquee, governor
  ui.rs         the widget kit — button.tsx, tick, hud-label, section shell
  hero.rs       the field layout, ripples, adaptive quality
  render.rs     wgpu setup and the two-pass frame
  screens.rs    the library, app, updates, settings and reports screens
  window.rs     winit shell, event loop, actions, crash watching
  github.rs     releases, checksums, downloads
  installer.rs  extract, install, launch, backup, restore, wipe
  config.rs     settings, launch params, export/import
  paths.rs      every location, per platform
  telemetry.rs  reports, causes, metrics, spool, upload
  osint/        icons (ICO/ICNS/PNG) + linux, windows, macos integration
  cli.rs        the headless CLI
assets/
  fonts/        Archivo, Inter, Geist Mono, Instrument Serif
  images/       the seven app icons and hero screenshots, from the site
  shaders/hero.wgsl   the GLSL → WGSL port
```

## Notes on the ports

Two details are worth calling out, because both were found by running the
thing rather than by reading it:

* **Swash delivers coverage row-major top-down.** An early version flipped it
  "to be safe", which rendered every glyph upside-down. The site's logo is a
  single SVG path, parsed by a small hand-written `M L H V Z` reader because
  `tiny-skia` does not ship one.
* **`wgpu`'s uniform layout must match the WGSL struct exactly.** A `vec3`
  inside a uniform block forces 16-byte alignment, which made the Rust mirror
  152 bytes where the shader expected 160, and `write_texture` rejects
  `rows_per_image` on a single 2D layer. Both are now checked by
  `artcraft-shadercheck` and by the alignment comments next to the struct.

## Licence

MIT OR Apache-2.0.
