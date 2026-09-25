# OSS 08: reliability

Part of [the roadmap](2026-09-24-cohesive-ui-00-roadmap.md). Build **second**, right after 06's CI.
It is cheap, and it protects every later slice.

## Why

NotchMon reads files and endpoints it does not own:

- Claude Code transcripts, Desktop's Chromium cache, Codex rollouts, Cursor's SQLite, the
  Antigravity bridge;
- codeburn's JSON;
- PokéAPI.

Any of these can change shape in an upstream release. For an open-source tool, "it broke after I
updated Claude Code" is the most common issue it will get. The way to survive that is to fail
**small and loud**: one cell shows a dash, the log says why, the rest keeps working, and the bug
report arrives with the evidence.

## Today

- About 100 inline `#[test]`s. There is no `tests/` folder and no fixtures of real files.
- `applog` appends forever to `run.log`, with no timestamps.
- There is no panic hook. A panic in a reader thread kills that reader silently until restart.
- `config.json` has no schema version. Migrations are ad hoc in `load()`, and a hand-edited or
  corrupted file falls back to defaults without keeping the old file.

## What ships

1. **Every reader thread is supervised.** Each worker loop body runs inside
   `std::panic::catch_unwind`.
   - **On a panic**:
     - log it with the reader's name;
     - set that provider's snapshot to `status: "error"` with `note: "reader crashed, see log"`;
     - back off 30 s, then 2 min, then 10 min;
     - retry.
   - One helper, `supervise(name, interval, body)`, replaces the hand-written loops.
2. **Panic hook and log.**
   - `std::panic::set_hook` writes the panic, thread name and backtrace to the log.
   - `applog` gains an ISO timestamp and a level (`info`, `warn`, `error`).
   - The log rotates at 1 MB, keeping `run.log` and `run.1.log`.
3. **Config safety.**
   - `config.json` and `companion.json` get `"schemaVersion": N`.
   - Migrations become a numbered list, `fn migrate_1_to_2(v: Value) -> Value`, tested one by one.
   - Before any migration or any failed parse, the old file is copied to
     `config.json.bak-<version>` (keep the last 3).
   - `companion.json` is written atomically (write to `.tmp`, then rename), so a crash mid-save
     never loses the Pokédex.
4. **Fixture tests are the merge gate for parsers.**
   - `notchmon/tests/fixtures/<reader>/<case>.*` holds real, **scrubbed** samples: transcript
     lines, a Desktop cache entry, a Codex rollout, a codeburn JSON, a PokéAPI reply, a
     `state.vscdb` built by the test.
   - Every parser has at least a happy case, an unknown-field case and a truncated case.
   - **Rule** (in `CONTRIBUTING.md` and `CLAUDE.md`): a PR that changes a parser adds or updates a
     fixture, or it is not merged.
5. **Contracts with upstream tools.**
   - `codeburn.rs` checks `codeburn --version`, and parses only versions in a tested range kept
     in `compat.toml`.
   - Outside the range, it still tries, but `doctor` warns "codeburn X.Y não testado".
   - The same file lists the tested Claude Code, Codex and Cursor versions.
   - `docs/UPSTREAM.md` explains how to refresh them.
6. **Diagnostics for bug reports.**
   - `notchmon doctor --json` and **Ajustes → Sistema → Copiar diagnóstico** produce the same
     text:
     - OS and version, app version;
     - each reader with its status, source path and last error;
     - tool versions from `compat.toml` checks;
     - the last 50 log lines.
   - Home paths are redacted to `~`. Nothing token-shaped is included (the same redaction
     helper as slice 04).
   - The bug issue template asks for this text.
7. **One helper for child processes.**
   - `spawn_tool(cmd, args, timeout)` is shared by codeburn, `agy`, `claude` and `gh`
     (slice 05).
   - It always sets a timeout and captures stderr. It sets `CREATE_NO_WINDOW` on Windows, and on
     macOS resolves the login-shell `PATH`.
   - It replaces the three ad hoc copies.
8. **Network hygiene.**
   - Every `ureq` call gets a connect and read timeout (5 s and 15 s).
   - A failure sets `backoff_until` the way `usage.rs` already does.
   - PokéAPI failures never block the companion: the sprite shows the placeholder frame, and XP
     keeps counting.

## Build order

One commit per phase.

1. `supervise`, the panic hook and the log format. Convert the reader loops.
2. Schema version, numbered migrations, `.bak` files, atomic companion save, with tests.
3. The `tests/fixtures` layout and fixtures for `tokens.rs`, `desktop_cache.rs`, `codex.rs` and
   `codeburn.rs` (moving the inline samples out), plus the CI rule.
4. `compat.toml`, `spawn_tool`, and `doctor --json` with the copy button.

## Acceptance

- A test that panics inside a fake reader shows that reader as `error`, and the others keep
  updating.
- A truncated `companion.json` on disk is left as `.bak`, the app starts, and the log says why.
- `doctor --json` from a real machine contains no string found in `~/.claude/.credentials.json`
  or `~/.codex/auth.json`.

## Left for later

- Opt-in crash reporting to a service. Not now: "nothing leaves your machine" is part of the
  pitch, and the copy-diagnostics flow covers bug reports.
