# AGENTS.md

ROrganizer: Windows window manager for multi-account Dofus (Unity and
Rétro). Global keyboard/mouse shortcuts focus any running client. Rust,
eframe/egui, Win32.

This file holds only what the code does not tell you. Gotchas local to one
function are documented as comments next to that code — read them before
changing it.

**Working on `site/`? Read `AGENTS.site.md` first.** It lives at the root
because Pages publishes the whole `site/` directory.

## Hard rules

- `cargo test` green, `cargo build` with zero warnings.
- No network: zero outbound sockets. No HTTP client or async runtime in
  the Windows build: `cargo tree --target x86_64-pc-windows-msvc -i <crate>`
  prints nothing for `reqwest`, `hyper`, `tokio`. `Cargo.lock` is not the
  test: it also resolves wasm-only deps, and winit's `wasm-bindgen-futures`
  pulls `tokio` in there. Native threads only.
- No file logging, no temp files, never log keystrokes. The only file the
  app writes is `%APPDATA%\rorganizer\config.json`.
- Manifest stays `asInvoker`: LL hooks work from a user process as long as
  Dofus runs as user. `requireAdministrator` would cost a UAC prompt per
  launch for nothing.
- Never build and upload a release exe by hand: a local binary has no
  provenance attestation. See [Release](#release).
- All OS code lives in `src/platform/` (Win32: `platform/win32/`), behind
  the contract documented in `platform/mod.rs`. Elsewhere: no `windows::`,
  no `#[cfg(windows)]`.

## Build

Toolchain `stable-x86_64-pc-windows-msvc`, PowerShell:

```powershell
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
cargo build --release   # never distribute a debug build
```

Release builds set `windows_subsystem = "windows"`: no console,
`eprintln!` is lost. Debug with a plain `cargo build` run from a terminal.

Effective MSRV is 1.95, imposed by egui 0.37, and nothing pins it: an older
toolchain fails inside a dependency, not in this code. No
`rust-toolchain.toml` on purpose — it would force a rustc version on every
contributor for a benefit limited to release verification.

## Dependencies

- `eframe`: `default-features = false, features = ["glow", "default_fonts"]`.
  Don't strip further: `default-features = false` on `egui` breaks window
  creation (font lookup). Default features left off on purpose:
  `clipboard` (arboard; the app has no text input) and `system_fonts`
  (pulls `fontique` to list the system fonts; only buys colored emoji and
  non-Latin glyphs).
- `egui`: `monochrome_emoji_fonts` is required. Since 0.37 the bundled
  emoji/icon fonts are no longer part of `default_fonts`, and the UI uses
  glyphs only they carry (⟳ refresh, ⚠ and 📁 in About): without the
  feature they render as tofu boxes, with no build warning.
- `egui_extras`: `svg` only, no PNG loader compiled.
  `Image::from_bytes("bytes://*.png")` renders the red error placeholder.
  Decode with `image`, then `ctx.load_texture` (see `decode_about_icon`).
- `egui_dnd` tracks egui's version exactly: bump them together.
- egui drops every `#[deprecated]` API at the next minor (0.35 did it in one
  go). Fix deprecation warnings when they appear; `#[allow(deprecated)]` is
  a scheduled compile break.
- URLs open through `webbrowser`: `cmd /C start` flashes a console.

### Supply-chain audit before any bump

crates.io was hit by real campaigns in 2026 (TrapDoor, Miasma worm). The
vector is code run at build time — `build.rs` and proc-macros — not shipped
code. No blind `cargo update`.

```powershell
cargo install cargo-audit cargo-deny --locked   # --locked mandatory
cargo deny check advisories bans sources licenses
cargo audit
```

`deny.toml` key settings: `[graph] targets = ["x86_64-pc-windows-msvc"]`
scopes the audit to the ~240 crates compiled here instead of the ~390 in
the lockfile (advisories for never-compiled crates bury real ones);
`unknown-registry/unknown-git = "deny"` keeps every dependency on
crates.io.

By hand, on the lockfile diff:

- Read every `build.rs` new or version-changed in the Windows graph.
  Unchanged versions are checksum-verified by Cargo. Skip `tests/`,
  `benches/`, `examples/`: never compiled for a dependency.
- Grep those for IoCs: `std::net`, `TcpStream`, `reqwest`, `ureq`,
  `Command::new`, `home_dir`, `.ssh`, `.env`, `keystore`, `wallet`, `AWS_`,
  `GITHUB_TOKEN`, base64 blobs, writes outside `OUT_DIR`. Usual false
  positive: the `Command::new(rustc) --version` probe.
- Genuinely new crates: check `crates.io/api/v1/crates/<name>` and
  `/owners` — owner, repo, age, downloads. Recent, low volume, name close
  to a popular crate = typosquat.
- Second database, nothing to install: POST the list from
  `cargo tree --target x86_64-pc-windows-msvc` to
  `https://api.osv.dev/v1/querybatch`.

## Cross-cutting gotchas

### eframe parks its loop while the viewport is hidden

With `Visible(false)`, `request_repaint()` does not wake `App::ui`,
viewport commands pile up unprocessed, and channels drained in `App::ui`
are never drained. Tray handlers (run on the main thread) must therefore
act through the backend on the registered main window
(`platform/*/main_window.rs`). Windows:

- show: `ShowWindow(SW_SHOW)`, or `SW_RESTORE` when minimized to the
  taskbar, + `SetForegroundWindow`;
- close: `ShowWindow(SW_SHOW)` **then** `PostMessageW(WM_CLOSE)`, otherwise
  `WM_CLOSE` waits in the queue until the next wake-up.

macOS has the same parking (a hidden window is occluded), plus: egui-winit
never re-reads the minimized state there (deadlock workaround, egui#3494).
After `ViewportCommand::Minimized(true)`, a restore from the Dock leaves egui
believing the window is minimized and `App::ui` stops running for good.
Common code minimizes only through `platform::minimize_main_window`.

### LL hooks don't fire while our own window is foreground

Measured: zero keyboard-hook calls when our window has focus. Binding
capture therefore goes through egui events (`Event::Key` → `key_from_event`).
Rejected: hook on the main thread (no fix, freezes), defocusing during
capture (visible focus flicker). Accepted limits:

- `key_to_vk` is the whitelist: F21+ don't exist in `egui::Key`, and the
  Pause/lock/media/browser keys egui 0.37 added are not mapped;
- `Num0`–`Num9` conflate numpad and top row.

The macOS event tap does see our own window's keys. It lets them through
while ROrganizer is frontmost, which gives the same behaviour, and hands
the key code to the binding modal: winit reports both ISO keys `0x0A`/`0x32`
as `Backquote` and numpad digits as top-row ones, so egui alone would bind
the wrong key.

### Hook callbacks are on the global input path

`kb_proc` / `mouse_proc` (macOS: the event tap callback) run for every
input event on the machine. Any blocking there is input lag for the whole
system. macOS also disables a tap whose callback is slow: focusing a
client (AX calls, up to a Dock animation) runs on a worker thread.

### Window height is computed, not measured

`ctx.used_rect()` misses overflowing widgets, so `App::apply_dynamic_height`
uses tuned constants. Any new conditional UI element (banner, modal…) must
add its height there.

### Bindings outlive characters, by design

`bindings` is never purged: a shortcut set on a character that isn't running
stays in `config.json`, so players who alternate keep it. Conflict detection
must only count what `sync_state_to_hooks` actually wires (see
`compute_conflicts`).

### Detected clients

| Client | Exe | Window title |
|---|---|---|
| Dofus / Expérimental | `Dofus.exe` | `Pseudo - Iop - 2.75.4.13 - Release` |
| Dofus Rétro | `Dofus Retro.exe` | `Pseudo - Dofus Retro v1.48.21` |
| Dofus (macOS) | bundle `com.Ankama.Dofus`, exe `Dofus` | `Pseudo - Cra - 3.7.14.18 - Release` |

The exe filter runs before the title regex: a new client family needs both.
Rétro titles carry no class (`class: None`). Rétro is Electron, ~8
processes per client, a single visible titled window; before character
selection its title has no ` - `, so no ghost row. On macOS the login
screen is titled `Dofus 3.7.14.18 - Release` (no ghost row either), titles
are read through the Accessibility API, and Rétro was never observed: it is
matched by its exe name `Dofus Retro` only.

## Testing

Unit tests are inline `#[cfg(test)] mod tests` on pure functions.
Programmatic end-to-end checks on Windows:

- HWNDs: `EnumWindows` + `GetWindowThreadProcessId`. Tray hidden window
  class `tray_icon_app`, main window title `ROrganizer`.
- Keystrokes: `keybd_event` fires the LL hook; PowerShell
  `SendKeys.SendWait` does not.
- Screenshots: make the process DPI-aware before any graphics call
  (`SetProcessDpiAwarenessContext(-4)`), then `SetWindowPos(HWND_TOPMOST)`
  + `SetForegroundWindow` before `CopyFromScreen` — otherwise DWM renders
  accelerated surfaces black.
- Tray menu on Windows 11: no longer a `ToolbarWindow32`. Find the button
  `ROrganizer` with UI Automation in `Shell_TrayWnd`; an exe run from a new
  path lands in the overflow flyout instead: click the `SystemTrayIcon`
  chevron, then search `TopLevelWindowForOverflowXamlIsland`. Right-click
  it. UIA may list no `MenuItem` in the popup: read the visible `#32768`
  window of our PID with `MN_GETHMENU`, then `GetMenuStringW` +
  `GetMenuItemRect` per item. Compare labels exactly: `-like "*Activer*"`
  also matches `Désactiver`.
- Taskbar order of grouped Dofus windows: not exposed to UIA. Move the
  cursor onto the `Dofus.exe` button with `mouse_event` moves, wait ~2 s
  and screenshot the thumbnail flyout: its titles give the order.
- Screenshot diffs: try several vertical offsets. An egui bump can shift
  everything by 1 px (~11 % differing pixels, under 1 % at `dy=+1`).

Programmatic end-to-end checks on macOS:

- Grant Terminal both Accessibility (to post events, read AX) and Screen
  Recording (`screencapture`). A binary launched from Terminal inherits
  Terminal's grants: to see the permission banner, launch it from a
  throwaway `.app` bundle with `open`.
- Clicks and keys: `CGEventPost` at `.cghidEventTap`; synthetic keys reach
  the event tap ~130 ms after posting, real ones much faster.
- Did a key reach the game: a `listenOnly` tap at `tailAppendEventTap`
  sees what the other taps let through.
- Front window: first layer-0 entry of `CGWindowListCopyWindowInfo`.
  Dofus doesn't answer `AXFocusedApplication` on the system-wide element.
- Dock contents: AX children of the Dock process (the Dock may auto-hide).

## Performance baseline

| Metric | Stable value |
|---|---|
| Release binary | ~8.2 MiB |
| RAM working set, idle | ~73 MB |
| RAM private, idle | ~66-67 MB |
| CPU, idle | 0 % (over 40 s) |
| New Dofus window detection | ~150-200 ms |

Measured 2026-11 with 4 Unity clients open: ~82 MB working set, ~74 MB
private, and 0.4-0.7 s of CPU over 40 s idle (window shown or hidden),
identical on egui 0.36 and 0.37. The client count moves these numbers:
compare two builds under the same setup rather than against the table.

Measure RAM at least 15 s after launch: earlier, the font atlas and
textures aren't realized yet (~60 MB), which looks like a regression at the
next bump. Below ~30 MB is unreachable with eframe + glow + ICU.

macOS (Apple Silicon, arm64 only, 3 Unity clients, active and hidden):
release binary ~7.1 MiB, physical footprint (`vmmap --summary`) ~32 MB,
0.03 s of CPU over 30 s idle. A title change reaches the snapshot within
~250 ms (AX notification, then the 150 ms coalescing).

## Release

`.github/workflows/release.yml` runs on a `v*` tag: tests, build,
provenance attestation, **draft** release. The release body is the
annotated tag message; the workflow inserts the SHA256/provenance block
before the SmartScreen heading.

- **Why a draft.** Defender flags the exe (`Wacatac`) until it is submitted
  to Microsoft, and the submission clears one exact hash. One build,
  submit, then `gh release edit <tag> --draft=false`. Never rebuild in
  between. The attestation binds to the artifact hash, so it verifies while
  still a draft.
- **Tag with `git tag -a --cleanup=verbatim -F <notes>`.** The default
  `strip` deletes every line starting with `#`: all `###` headings vanish,
  including the SmartScreen anchor the insertion relies on.
- **Reproducibility holds only at an identical path.** With the remaps in
  `release.yml`, two builds at the same path are bit-identical, and CI
  always builds in `D:\a\ROrganizer\ROrganizer` (verified across runs at
  v1.3.0). At different paths they differ: `--remap-path-prefix` only
  affects rustc, while the MSVC linker orders its output by the absolute
  `.obj`/`.lib` paths it receives. Never tell users they can reproduce the
  SHA256 — the guarantee is the provenance attestation.
