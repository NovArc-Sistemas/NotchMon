# Codenotch for Windows

A Windows port of [Codenotch](https://github.com/vinzdg/codenotch) — the usage notch that
sits on the edge of your screen and answers two questions at a glance:
**how much of my AI allowance is left**, and **is Claude still working**.

Same design language as the macOS original (inverse-rounded pill, colour-graded rings,
hover card with per-window bars), rebuilt for Windows in Rust + Tauri 2 / WebView2.
No code is copied from the Swift app; the providers are reimplemented from their
documented behaviour and the wire formats.

## What it shows

| Cell | Source | How it reads it |
|---|---|---|
| **Claude** | `GET https://api.anthropic.com/api/oauth/usage` with the token Claude Code keeps in `~/.claude/.credentials.json` | Session / weekly windows, 429 back-off with a persisted deadline, stale readings dimmed with their age. A thin arc spins inside the ring while a Claude session is working, and pulses amber when one is waiting on you (Claude Code hooks + transcript watcher, desktop app included). |
| **Codex** | `GET https://chatgpt.com/backend-api/wham/usage` with the session Codex keeps in `~/.codex/auth.json` (read only, never refreshed), falling back to the `rate_limits` snapshot in the newest rollout log | Live primary/secondary windows (5h + weekly on paid plans, a monthly window on free) while Codex is signed in; otherwise the last snapshot, marked stale by its own timestamp. |
| **Cursor** | The editor's own session from `state.vscdb` → `cursor.com/api/usage-summary` | Included usage / API usage / on-demand, reset at billing-cycle end. Nothing to sign into: it borrows the editor's session, so there is only ever one account. |
| **Antigravity** | Official `agy` CLI `/usage` print when installed; otherwise the existing local `language_server` bridge, Google Cloud Code API, or transcript model count | Official four quota rows (Gemini & Claude/GPT 5h/weekly) without running the full IDE. When CLI is absent, falls back to legacy local bridge/API. |

Providers that are not installed simply do not get a cell.

### Antigravity

- **Official CLI (Preferred)**: When the official Antigravity CLI (`agy.exe`) is installed (`%LOCALAPPDATA%\agy\bin\agy.exe` or on `PATH`) and signed in, Codenotch reads official quotas directly without keeping the full IDE running.
- **Execution**: Runs the official CLI in a hidden Windows pseudo-console, with a 70-second timeout and cleanup of its process tree. It does not need PowerShell scripts or a separate service.
- **Refresh**: Checks at startup and on hover/explicit request when readings are at least five minutes old; failed attempts are also limited to once per five minutes. It keeps previous readings on failure, without switching to legacy APIs. The CLI is not launched periodically while idle.
- **Fallback**: When the official CLI is not installed, Codenotch preserves the legacy local bridge (`language_server`), Credential Manager, and transcript model turn counting to maintain compatibility with existing installations.
- **Official CLI Reference**: Standalone `/usage` printing is described in the [official Antigravity CLI documentation](https://www.antigravity.google/docs/cli/headless). Note: no categorical Terms of Service guarantee is made.

Restart Codenotch after installing or removing `agy`: the source is selected at startup.
The CLI's text report is parsed defensively; an unsupported format or failed sign-in
shows an error or the last reading marked stale. Codenotch does not automate sign-in.

## Install / build

Prerequisites: Rust (MSVC toolchain), WebView2 runtime (ships with Windows 11).

```powershell
# from this directory (the repo root here; `windows/` inside the upstream repo)
cargo build --release
.\target\release\codenotch.exe          # pill appears on the right edge of the primary monitor
.\target\release\codenotch.exe doctor   # self-diagnosis: credentials, data sources, icons, hooks
```

Tray menu: refresh now, reset position, open data folder (`%APPDATA%\codenotch` — logs,
persisted readings, icon overrides), start with Windows, install/uninstall Claude Code hooks.

### Icons

Provider marks are the SVGs from [`@lobehub/icons-static-svg`](https://github.com/lobehub/lobe-icons)
(MIT), embedded unmodified — see `codenotch/glyphs/NOTICE.md`. Drop your own
`claude|codex|cursor|gemini.svg` (or `.png`) into `%APPDATA%\codenotch\glyphs\` to override.
The marks remain the trademarks of their owners.

## Layout

```
.
├── codenotch/          Tauri 2 app: window, tray, providers (usage.rs, codex.rs, cursor.rs, antigravity.rs),
│   ├── src/            session engine (watcher.rs, state.rs, focus.rs), glyphs.rs, doctor.rs
│   ├── ui/notch.html   the pill + hover card (single file, no framework)
│   └── glyphs/         provider marks (+ NOTICE.md)
└── codenotch-hook/     <5 ms hook messenger Claude Code calls; forwards events to the app
```

## Relationship to upstream

This port follows the upstream design spec (`docs/specs/2026-08-28-usage-notch-design.md`)
and provider semantics. It is developed at
[Im-Midi/codenotch-windows](https://github.com/Im-Midi/codenotch-windows) and offered to the
upstream project as its `windows/` tree; the two are kept in sync. The session-detection engine
originated in [Im-Midi/Pac-Man](https://github.com/Im-Midi/Pac-Man) (MIT).

## License

MIT — see `LICENSE`. The Codenotch design and name belong to the upstream author.
