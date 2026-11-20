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
  the shipped builds: `cargo tree --target <target> -i <crate>` prints
  nothing for `reqwest`, `hyper`, `tokio`, for `x86_64-pc-windows-msvc`,
  `aarch64-apple-darwin` and `x86_64-apple-darwin`. `Cargo.lock` is not the
  test: it also resolves wasm-only deps, and winit's `wasm-bindgen-futures`
  pulls `tokio` in there. Native threads only.
- No file logging, no temp files, never log keystrokes. The only file the
  app writes is its `config.json` (through a sibling `config.json.tmp`
  renamed over it): `%APPDATA%\rorganizer\` on Windows,
  `~/Library/Application Support/rorganizer/` on macOS.
- Manifest stays `asInvoker`: LL hooks work from a user process as long as
  Dofus runs as user. `requireAdministrator` would cost a UAC prompt per
  launch for nothing.
- Never build and upload a release exe or dmg by hand: a local binary has
  no provenance attestation. See [Release](#release).
- All OS code lives in `src/platform/` (`platform/win32/`,
  `platform/macos/`), behind the contract documented in `platform/mod.rs`.
  Elsewhere: no `windows::`, no `objc2`, no `#[cfg(windows)]` or
  `#[cfg(target_os = ...)]`.

## Build

Toolchain `stable-x86_64-pc-windows-msvc`, PowerShell:

```powershell
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
cargo build --release   # never distribute a debug build
```

Release builds set `windows_subsystem = "windows"`: no console,
`eprintln!` is lost. Debug with a plain `cargo build` run from a terminal.

macOS: rustup's `stable` plus both targets, and Xcode for the linker.
`cargo build` gives a bare binary, enough to debug from a terminal (it
inherits the terminal's Accessibility grant). The shipped `.app` and
`.dmg` come from `scripts/package-macos.sh`: universal binary (`lipo` of
`aarch64-apple-darwin` and `x86_64-apple-darwin`, macOS 11 minimum),
`Info.plist`, `.icns`, signature. The macOS icon is
`resources/icons/icon-macos.png`, drawn from `icon.png` by
`scripts/macos-icon.swift` in Apple's rounded-square grid (macOS 26 puts
any other shape in a grey tile): regenerate it when `icon.png` changes.
It is also the runtime window icon on macOS, which eframe pushes to the
Dock (`platform::WINDOW_ICON_PNG`).

```bash
rustup target add aarch64-apple-darwin x86_64-apple-darwin
MACOS_SIGN_IDENTITY=<sha1> MACOS_KEYCHAIN=<keychain> scripts/package-macos.sh
```

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

`deny.toml` key settings: `[graph] targets` lists the three shipped
targets (`x86_64-pc-windows-msvc`, `aarch64-apple-darwin`,
`x86_64-apple-darwin`), which scopes the audit to the ~260 crates compiled
for them instead of the ~350 in the lockfile (advisories for
never-compiled crates bury real ones); `unknown-registry/unknown-git =
"deny"` keeps every dependency on crates.io.

By hand, on the lockfile diff:

- Read every `build.rs` new or version-changed in the graph of any
  shipped target.
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
  `cargo tree --target <target>`, for each shipped target, to
  `https://api.osv.dev/v1/querybatch`.

## Cross-cutting gotchas

### eframe parks its loop while the viewport is hidden

With `Visible(false)`, `request_repaint()` does not wake `App::ui`, and
channels drained in `App::ui` are never drained. It does run `App::logic`
(egui 0.37), at most one pass per 100 ms, and viewport commands sent from
there are processed: the overlay and the debounced config save live in
`logic` for that reason. `App::ui` also runs, unpainted, while a child
viewport (the overlay) is visible. Tray handlers (run on the main thread)
still act through the backend on the registered main window
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

### The character overlay: egui viewports on Windows, native panels on macOS

`platform::OVERLAY_IS_EGUI` picks the renderer. Both draw from
`crate::overlay` (state, geometry, palette, gestures, menu model) and App
alone decides whether the bar shows (`ShowGate`) and persists its events.
Windows:

`ui::overlay` registers three deferred viewports (bar, tooltip,
right-click menu) on every root `ui` pass while the overlay is on and
ROrganizer active; `App::logic` owns the bar's visibility through
`ViewportCommand`s. Measured traps:

- winit rewrites the whole window style and extended style on every
  change of its own flags (`window_state.rs`), dropping
  `WS_EX_NOACTIVATE`: after one hide/show the bar activated on click and
  blinded the hooks. `platform::overlay::attach` subclasses the window and
  forces the bits in `WM_STYLECHANGING`, answers `MA_NOACTIVATE`, strips
  the caption (DWM drew its top edge and a ghost close button through the
  transparency), handles `WM_NCCALCSIZE` (winit shifts an undecorated
  window's client area 1 px down for its shadow) and skips the non-client
  paint (once activated, a window drew a classic caption with its title
  over the bar).
- `ViewportCommand::StartDrag` never moves a window that can't be
  activated: moves and resizes follow the global cursor and move the
  window natively.
- A hidden deferred viewport is never painted, so its callback can't
  attach itself: `logic` sends `RequestPaintWhileHidden` until it has.
- Commands queued for any viewport are processed at the end of the next
  pass of any viewport (egui `end_pass`): the bar drives the tooltip that
  way.
- Transparent pixels still catch clicks, and egui popups can't leave
  their window: the bar window is exactly the bar, the tooltip and the
  menu are viewports of their own.
- The menu is egui, not `TrackPopupMenu`: a native popup ignores the
  app's theme, and blocking in it froze the frame that hides the tooltip.
  It takes the focus (`attach(_, true)` + `overlay::activate`) so that a
  click elsewhere closes it, then gives the foreground back to Dofus.
- A head lights up on press and stays lit until its client is really in
  front (`FOCUS_GRACE`): the switch takes a few frames, and reading the
  foreground meanwhile gives the previous client back.

macOS can't use egui (spike, 2026-11): a click on any winit window
activates the app, it never shows over another app's fullscreen Space,
and `Visible(true)` from another thread never reaches a hidden viewport.
`platform/macos/overlay/` draws a borderless `NonactivatingPanel` (and a
second, click-through one for the tooltip) with AppKit, driven from
`App::logic` through `overlay::{show, hide, close}`. Measured traps:

- `setHidesOnDeactivate(false)` is mandatory: the app is `.accessory`,
  hence inactive, whenever the main window is hidden in the menu bar.
- `CanJoinAllSpaces | FullScreenAuxiliary` keeps the bar over a
  fullscreen client. Not solved, in the watcher: `AXWindows` only lists
  the windows of the Space in front, so a rescan made from a fullscreen
  client's Space drops every other client from the bar and the main
  list, and nothing rescans on a Space change (measured: back on the
  desktop, the bar stayed hidden over a client it no longer knew).
- `NSCursor::set` from an inactive app is ignored: the bar has no cursor
  shapes there.
- The window server hands a click on a fully transparent pixel to the
  window below (measured on a stand-in client: the bar's rounded corners
  clicked into it). `paint` fills the whole rectangle at alpha 1/255 first.
- The context menu is a real `NSMenu` with `darkAqua`/`aqua` set from the
  app's theme. Popped from the inactive app, it never activates it; it
  spins a nested run loop, during which `App::logic` may run: nothing
  holds the state lock across `popUpMenuPositioningItem`, nor across
  `orderFrontRegardless` (which can paint at once, and `drawRect:` locks
  the state). The menu swallows the bar's `mouseExited:`: the pointer is
  read back afterwards.
- The default panel animation kept a hidden bar on screen ~200 ms:
  `NSWindowAnimationBehavior::None`.
- While the main window is hidden, `logic` runs at most every 100 ms: the
  watcher's activation callback refreshes the lit head directly, and a
  menu pick applies its layout at once.

### LL hooks don't fire while our own window is foreground

Measured: zero keyboard-hook calls when our window has focus. Binding
capture therefore goes through egui events (`Event::Key` → `key_from_event`).
Rejected: hook on the main thread (no fix, freezes), defocusing during
capture (visible focus flicker). Accepted limits:

- `key_to_vk` is the whitelist: egui's F21–F35 and the
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

### The macOS Accessibility grant follows the code signature

macOS ties the grant to the app's designated requirement. Ad-hoc signed
(`codesign -s -`), that is the exact binary hash: every new build loses the
grant while System Settings keeps showing the old entry as enabled, and
nothing works. Signed with a certificate, it is the certificate: measured,
the grant survives a binary swap. Shipped builds must therefore always be
signed with the same certificate; changing it costs every player the grant.

While the grant is missing, the banner's button drops the always-on-top
level (`App::request_input_permission`): the system prompt and System
Settings are ordinary windows that ROrganizer would otherwise cover. The
level comes back once the grant arrives, or once the user returns to
ROrganizer without granting it.

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
- Overlay: windows titled `ROrganizer overlay`, `ROrganizer overlay
  tooltip` and `ROrganizer overlay menu`. A Dofus client comes to the
  front through its own shortcut (`mouse_event` XBUTTON for Mb4), not
  `SetForegroundWindow` from the test process, which the foreground lock
  refuses. Never click blind on a Dofus window: a click in the game moves
  the character or opens its menus.
- A synthetic click at the exact pixel where the cursor last left a window
  is lost: winit sends no `CursorMoved` for an unchanged position, so egui
  has no pointer when the button comes. Move by a pixel, or hover ~0.4 s,
  before pressing.

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
- Overlay: owner `rorganizer`, layer 3 (`NSFloatingWindowLevel`), no
  title; the bar is the larger of the two, the tooltip the other. The
  `NSMenu` is a layer-101 window of ours while open, positioned by AppKit:
  screenshot it to find its items. `NSWorkspace.frontmostApplication`
  goes stale in a process without a run loop: read the front from the
  window list. Clicks on the bar's heads land on the panel, never on the
  game under it; anything else on a Dofus window is a blind click.
- Stand-in clients: a Swift app whose executable is named `Dofus` (or
  `Dofus Retro`) and whose window carries a client title is detected like
  one. Build it with `-target arm64-apple-macos11`: by default `swiftc`
  targets the SDK's macOS (28 here, on a 27 system), and LaunchServices
  then refuses to `open` it.

## Performance baseline

| Metric | Stable value |
|---|---|
| Release binary | ~8.5 MiB |
| RAM working set, idle | ~73 MB |
| RAM private, idle | ~66-67 MB |
| CPU, idle | 0 % (over 40 s) |
| New Dofus window detection | ~150-200 ms |

Measured 2026-11 with 4 Unity clients open: ~82 MB working set, ~74 MB
private, and 0.4-0.7 s of CPU over 40 s idle (window shown or hidden),
identical on egui 0.36 and 0.37. The client count moves these numbers:
compare two builds under the same setup rather than against the table.
The overlay, shown, adds ~4 MB private and no measurable CPU (same 4
clients, overlay on vs off in the same binary).

Measure RAM at least 15 s after launch: earlier, the font atlas and
textures aren't realized yet (~60 MB), which looks like a regression at the
next bump. Below ~30 MB is unreachable with eframe + glow + ICU.

macOS (Apple Silicon, arm64 only, 3 Unity clients, active and hidden):
release binary ~7.1 MiB, physical footprint (`vmmap --summary`) ~32 MB,
0.03 s of CPU over 30 s idle. A title change reaches the snapshot within
~250 ms (AX notification, then the 150 ms coalescing). Shipped universal
binary ~15.5 MiB (both slices), `.dmg` ~8.5 MiB.

The overlay on macOS (release arm64 binary ~7.5 MiB, 2 Unity clients,
main window hidden, bar shown over Dofus vs overlay off, same binary,
measured 2026-11): 54.2 vs 51.7 MB physical footprint, 0.017 vs
0.016 s of CPU over 30 s idle. A run of the same setup gave 45 MB with
the overlay on: footprints swing by several MB between launches.

## Release

`.github/workflows/release.yml` runs on a `v*` tag. Jobs `windows` (exe)
and `macos` (universal dmg) each test, build, check for leaked paths and
attest; `publish` gathers both into one **draft** release. The release
body is the annotated tag message; the workflow inserts the
SHA256/provenance block, both files, before the SmartScreen heading.

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
- **The macOS signing certificate never changes.** Self-signed
  `CN=ROrganizer`, SHA-1 `9005666D41DA362DEA45BB462FA8B658C8251A7B`, valid
  until 2056, in the secrets `MACOS_CERT_P12` (base64) and
  `MACOS_CERT_PASSWORD`; `release.yml` pins the SHA-1 and fails on any
  other. The players' Accessibility grant is tied to it (see the gotcha
  above): a new certificate means every player re-grants, a lost one means
  the same, so the maintainer keeps an offline backup of the `.p12` and
  its password. No Apple Developer ID: Gatekeeper blocks the first launch
  of each downloaded version until "Open Anyway" in Privacy & Security.
  Nothing to submit anywhere, unlike the exe.
