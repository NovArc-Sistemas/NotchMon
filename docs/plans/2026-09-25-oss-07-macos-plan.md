# OSS 07: macOS

Part of [the roadmap](2026-09-24-cohesive-ui-00-roadmap.md). Build **after slice 02**: the hub
replaces `consumo.html` and `settings.html`, so the UI to verify on a Mac is half what it is today.
Needs slice 06's CI (a `macos-latest` runner) and slice 08's platform helpers.

## Goal

The same NotchMon on macOS 13 and later, Apple Silicon and Intel. That means the pill, the hub,
the pet, the menu bar item, all four providers and the companion. It is one codebase: the web UI
is shared, and Rust differs only behind `cfg`.

## Reference implementation

Upstream Codenotch's macOS app (`vinzdg/codenotch`, Swift, MIT) already reads every provider on
macOS. **Port its logic, not memory.** Before writing each reader, read the matching file:

| Reader | Upstream Swift to read first |
|---|---|
| Claude credential | `Sources/Providers/ClaudeCredentials.swift`, `ClaudeProfile.swift`. The token lives in the **login Keychain**, not in `~/.claude/.credentials.json`. The service name gets a suffix per `CLAUDE_CONFIG_DIR` profile, several items can accumulate, so read the **newest**, and never prompt on a timer. |
| Claude Desktop cache | the Desktop cache reader beside those files |
| Cursor | `CursorCredentials.swift`: `~/Library/Application Support/Cursor/User/globalStorage/state.vscdb` and `~/.cursor/cli-config.json` |
| Antigravity | `AntigravityCredentials.swift` |
| Keychain partitions | `Scripts/fix-keychain-partitions.sh`: why a rebuilt, unsigned app loses Keychain access, and how upstream recovers |

## Work, module by module

The Windows-specific code is what the `cfg(windows)`, `windows::` and `%APPDATA%` greps find
today. The `cfg(not(windows))` stubs that exist are named, so none is forgotten.

| Module | Windows today | macOS |
|---|---|---|
| `config.rs` `data_dir()` | `%APPDATA%\notchmon` | `~/Library/Application Support/NotchMon` (through `dirs::config_dir()`, which already resolves there) |
| `usage.rs` (Claude) | `~/.claude/.credentials.json` | Keychain, newest item, no UI prompt unless the user pressed "Conectar"; then `/usage` from the installed `claude`, as upstream orders it |
| `desktop_cache.rs` | `%APPDATA%\Claude\Cache\Cache_Data` | `~/Library/Application Support/Claude/Cache/Cache_Data` (same Chromium simple cache format; verify on a real machine, and add a fixture) |
| `cursor.rs` | `%APPDATA%\Cursor\…\state.vscdb` | the Library path above |
| `codex.rs`, `tokens.rs`, `watcher.rs` | `~/.codex`, `~/.claude/projects` | same paths; only the watcher backend changes (FSEvents, through `notify`) |
| `antigravity.rs`, `agy_cli.rs` | Credential Manager (`CredReadW`), named pipes, process scan | Keychain item, Unix socket or port discovery, `ps`. Stubs at `antigravity.rs:198`, `:378` and `agy_cli.rs:484` |
| `activity.rs` | ToolHelp process scan | `sysinfo` or `ps` (stubs at `:460`, `:587`) |
| `focus.rs` | Win32 window focus | activate the owning terminal app with `NSRunningApplication` through `objc2`, or `osascript` as the lazy first version (stubs at `:117`, `:271`) |
| `glyphs.rs` | icons from installed `.exe` resources | icons from `.app/Contents/Resources/*.icns`, or the bundled glyphs only (stub at `:249`) |
| `autostart.rs` | the `HKCU\…\Run` value | a LaunchAgent plist pointing at `~/.cargo/bin/notchmon` (`tauri-plugin-autostart` does this; it can replace both OS paths) |
| `main.rs` window placement | work area, DPI, `topmost` | the screen's `visibleFrame`, menu bar height, and the notch safe area on MacBooks. `alwaysOnTop` plus `visibleOnAllWorkspaces`; transparent windows need Tauri's `macOSPrivateApi` flag (verify the current v2 setting name) |
| `tray.rs` / `trayicon.rs` | tray icon with numbers | menu bar item; the numbers icon becomes a template image plus a short title (`$586 · 20%`) |
| `hooks_install.rs`, `notchmon-hook` | `notchmon-hook.exe` next to `current_exe()`, path in `~/.claude/settings.json` | the same sibling lookup without the hard-coded `.exe` suffix (`std::env::consts::EXE_SUFFIX`), so `~/.cargo/bin/notchmon-hook` is found |
| `i18n.rs` | `GetUserDefaultLocaleName` | `sys-locale` crate (can replace both) |
| `diag.rs`, `doctor.rs` | Windows paths | per-OS path list from one table (slice 08) |
| `codeburn.rs` | `%APPDATA%\npm\codeburn.cmd` | `PATH` from the login shell (`$SHELL -lc 'command -v codeburn'`), plus `/opt/homebrew/bin` and `/usr/local/bin` |

**Rule for this slice**: a feature that cannot work on macOS yet stays `cfg`-gated. It shows one
`doctor` line ("Antigravity: não suportado no macOS ainda") and one PENDINGS entry. It never
crashes and never shows a broken cell.

## Running as a bare binary

On macOS NotchMon runs as `~/.cargo/bin/notchmon`, **not as an `.app` bundle** (slice 06: install
from GitHub). Consequences to handle:

- **No Dock icon, only the menu bar**: set the activation policy to *accessory* at startup.
- **Keychain.** The login Keychain remembers "Always Allow" per binary signature. An unsigned
  binary rebuilt by `cargo install --force` looks new, so the first Claude read after an update
  may ask again. Document this in the README, and read only when the user clicked "Conectar"
  (never on a timer), so a prompt never appears out of nowhere.
- **Transparent always-on-top windows** need Tauri's `macos-private-api` feature on the `tauri`
  crate and the matching config flag. Behind `cfg(target_os = "macos")`, that is fine for a
  binary nobody submits to the App Store.

## Placement on a Mac

- Default: the **screen edge**, as on Windows. Same pill, same modes A, B and C.
- Option, later: beside the notch on MacBooks. Upstream just shipped this ("the notch beside your
  Mac's own"); it is theirs to own, so this slice does not copy it.

## How it gets verified

**The maintainer's machine is Windows.** Unless a Mac is available, this slice is CI-driven:

- `cargo test` on `macos-latest`, with a fixture for every mac path (slice 08);
- the README install command, run by CI on `macos-latest` (slice 06), so testers can install
  from any commit;
- a "Mac testers" pinned Discussion that asks for `doctor` output.

Each reader lands only with its fixture, and one tester's `doctor` output on real hardware.
State this plainly in the README until a maintainer runs a Mac daily.

## Build order

One commit per reader, because each is independently testable.

1. `cargo build` on macOS with every stub. The window shows, and the pill and hub render with
   no data.
2. Paths (`data_dir`, Desktop cache, Cursor, codeburn discovery), plus `tokens.rs` and the
   watcher: the companion works.
3. Claude credential via Keychain: the Claude ring works.
4. Codex and Cursor rings.
5. Menu bar item, autostart, the hook helper in the bundle.
6. Focus, activity, glyphs.
7. Antigravity (last: most OS-specific, smallest audience).
8. Switch the macOS CI job from `cargo check` to the full install step, and remove "Windows
   only" from the README.

## Acceptance

- On a clean Mac with rustup and the Xcode tools: the README command builds, `notchmon` shows the
  pill in the menu bar and on the screen edge, the Claude, Codex and Cursor rings read, and the
  egg counts tokens.
- No Keychain prompt appears unless the user clicked something that says it will ask.
- Every item in the module table is either done or has its `doctor` line and PENDINGS entry.

## Left for later

- The notch-side layout on MacBooks.
- An `.app` bundle, signing and notarization, if prebuilt binaries ever ship (slice 06, later).
