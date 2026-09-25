# Cohesive UI 05: GitHub backup and profile card

Slice 5 of [the roadmap](2026-09-24-cohesive-ui-00-roadmap.md). Needs slices 2 and 4. Canvas
artboards: GitHubConectar, GitHub, Cartao. It also closes the `PENDINGS.md` item "Companion save
export/import".

**Step 0: run `/codex-review` on this plan before writing code.** The slice handles auth and pushes
user data off the machine, which falls under the global rule for high-stakes plans.

## Decision: GitHub CLI only; NotchMon stores no token

The canvas mixed two ideas: signing in with `gh`, and a token in Windows Credential Manager. Pick
one: **everything goes through the installed `gh` CLI**, and NotchMon never holds a GitHub
credential.

- `gh auth status --hostname github.com` tells whether the user is signed in and as whom.
- Every GitHub call is `gh api …` as a child process, with JSON over stdin and stdout, the same
  way `codeburn.rs` runs its CLI. On Windows it uses `CREATE_NO_WINDOW`; on macOS `gh` is found
  through the login shell's `PATH` (`/opt/homebrew/bin` is not on a GUI app's `PATH`).
  This is the shared helper from slice 08.
- No local clone and no `git` binary. A backup is **one commit through the Git Data API**:
  create blobs → create a tree on top of the current `HEAD` tree → create a commit → move
  `refs/heads/main`.
- Without `gh`, or when not signed in, the screen says exactly that and shows `gh auth login` as
  selectable text with a copy button. A built-in OAuth device flow is later work.

This leaves no secret to store, rotate or leak, and the user can revoke access with `gh auth logout`.

## What ships

### 1. Config (`config.rs`)

```rust
pub struct GithubPrefs {
    pub repo: Option<String>,        // "owner/notchmon-vault"
    pub include: GithubInclude,      // save, metrics, settings: true; projects, account_configs: false
    pub schedule: String,            // "daily" (default) | "6h" | "manual"
    pub daily_at: String,            // "03:00" local
    pub on_events: bool,             // true: also after hatch, evolution, graduation
    pub profile_card: bool,          // false by default: public
    pub last_backup: Option<u64>,    // ms epoch
    pub last_commit: Option<String>, // short sha
    pub last_error: Option<String>,
}
```

### 2. What is written to the vault

```
notchmon-vault/
  README.md                    what this repository is, and how to restore
  save/companion.json          the companion save as is (data_dir()/companion.json)
  config/notchmon.json         config.json minus `github` and any path under the user's home
  config/accounts.json         AccountMeta list (names, tags, colors; no config_dir paths)
  metrics/YYYY/MM/DD.json      per day: tokens by provider and model (tokens.rs); cost, calls,
                               sessions and cache by provider (codeburn `days`); projects only
                               when `include.projects`
  accounts/<tag>/summary.json  only when `include.account_configs`: the redacted ConfigSummary
                               from slice 4, never a raw settings.json, CLAUDE.md or config.toml
```

- **Metrics are rewritten** for the last 35 days on each backup, and older days are never
  touched. Re-running a backup is idempotent: a tree with no change makes no commit.
- **Never written:**
  - credentials, `.credentials.json`, `auth.json` and `~/.claude.json`;
  - `env` values;
  - conversation content;
  - raw config files.

### 3. Connect flow (GitHubConectar)

1. **Status**: the `gh` login, or instructions.
2. **Repository**: a name (default `notchmon-vault`).
   - A new repository is created with `gh repo create <name> --private`.
   - An existing repository is accepted **only if it is private**. A public one is refused, with
     that reason shown.
3. **What to back up**: the five switches, with defaults as above.
4. **When**: the schedule and `on_events`.
5. **Create and back up**: runs the first backup and shows the commit.

### 4. Worker (`github.rs`)

- A thread like the other readers. It wakes at `daily_at` or every 6 h, and on companion
  `Event::{Hatched, Evolved, Graduated}` (debounced 10 min).
- It backs up, then emits `github` with the status.
- On failure it waits 5, 15, then 60 min before retrying, writes `last_error` and `applog`, and
  never loops faster than that.
- Commands:
  - `github_status()`, `github_connect(repo, create: bool)`, `github_backup_now()`;
  - `github_history(n)` (commits on `main`: sha, message, date);
  - `github_restore(sha)`, `github_disconnect()` (forgets the repository and never deletes it);
  - `github_set_prefs(prefs)`.

### 5. Restore

- Fetches `save/companion.json` and `config/*.json` at that sha.
- **Validates with serde** into `companion::State` and `Config` before touching anything.
- Copies the current files to `*.bak-<timestamp>`, then replaces them and reloads the engine and
  config the way a normal load does.
- A file that fails validation stops the whole restore and reports which file failed.
- The page asks for confirmation in its own dialog. The message says what gets replaced and that
  the `.bak` copies stay.

### 6. Profile card (off by default, public)

- An SVG built in Rust from a string template (495 × 195, the GitHub profile card size). It
  shows the sprite as an embedded PNG from the local PokéAPI cache, name, rarity, level, form,
  tokens over 30 days, active days, what is left to graduate, and a 18-week × 7-day activity
  grid from `tokens.rs`.
- **Aggregates only.** It never shows cost, projects, accounts or models.
- Committed to the user's profile repository `<login>/<login>` at `notchmon/card.svg`. That
  repository is public by GitHub's design, which the switch says in its description.
- NotchMon **never edits the user's README**. The screen shows the one line to paste
  (`![NotchMon](notchmon/card.svg)`) with a copy button.
- If `<login>/<login>` does not exist, the screen explains it and links to GitHub's docs. It does
  not create the repository.

### 7. The GitHub screen

- A status card with **Backup agora**.
- The switches.
- History with **Restaurar** on each row.
- The profile card switch and a preview.
- Connection: login, **Trocar repositório**, **Desconectar**.

The tray's **Fazer backup agora** (slice 3) appears once `repo` is set.

## Build order

One commit per phase.

0. `/codex-review` on this plan; fold the verdict in.
1. `gh` wrapper (spawn, parse, errors) with a fake-`gh` test harness (a script on `PATH` in a temp
   directory), plus `GithubPrefs`.
2. Vault writer: build the file map, create the Git Data API commit, check idempotency. Tests use
   the fake `gh`.
3. Worker, scheduler, events, and the connect and main screens.
4. Restore with serde validation and `.bak` files, with a round-trip test.
5. Profile card SVG and its commit.

## Acceptance

- With `gh` signed out, the screen explains and nothing crashes.
- A public repository name is refused.
- Two backups with no change make one commit.
- Restore of a corrupted `companion.json` changes nothing on disk.
- `rg -i "token|secret|password" ` over a real vault finds only the words in `README.md`.
- The profile card contains no cost or project name.

## Left for later

- A built-in OAuth device flow, for users without `gh`.
- Using the private repository as a sync between two machines (merging saves is a design of its
  own).
- Remove the "Companion save export/import" item from `PENDINGS.md` when phase 4 ships.
