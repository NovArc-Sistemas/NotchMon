# Cohesive UI 04: the accounts center (Contas)

Slice 4 of [the roadmap](2026-09-24-cohesive-ui-00-roadmap.md). Needs slice 2 (the hub). Canvas
artboards: Contas, Conta.

## Problem

- The only account metadata today is `claude_names: HashMap<org UUID, String>` (`config.rs:119`).
  It has no UI, so extra Claude rings read "Claude (3f2a8c91)".
- Codex, Cursor and Antigravity have no names at all.
- Nothing shows which login a ring belongs to or how it is configured.

## What ships

### 1. Data model (`config.rs`)

```rust
pub struct AccountMeta {
    /// Stable identity, provider-scoped. Never the ring id.
    pub key: String,          // "claude:org:<uuid>", "codex:acct:<account_id>", "cursor", "gemini"
    pub name: String,         // user label, 1..=32 chars
    pub tag: String,          // 1..=3 chars, unique among accounts, shown in the ring center
    pub color: String,        // "#rrggbb" from the tag palette (design system rule)
    pub config_dir: Option<PathBuf>, // linked folder for the config viewer
    pub hidden: bool,         // user dismissed a detected account
}
// Config gains: pub accounts: Vec<AccountMeta>   (order = display order in Contas)
```

- **Identity comes from data NotchMon already reads.**

  | Provider | Identity | Source |
  |---|---|---|
  | Claude, main ring | the CLI's organization | `cli_org()`, `usage.rs:112` (`~/.claude.json`, both OSes) |
  | Claude, extra rings | the organization of each desktop-cache reading | Desktop cache: `%APPDATA%\Claude` on Windows, `~/Library/Application Support/Claude` on macOS (slice 07) |
  | Codex | `tokens.account_id` | `~/.codex/auth.json` (both OSes) |
  | Cursor, Antigravity | one account each: the key is the provider id | — |

  Before building the identity resolver, read upstream Codenotch's `Sources/Providers/ClaudeProfile.swift`.
  It already discovers `CLAUDE_CONFIG_DIR` profiles and names them, and its rules are the tested
  prior art for "which Claude login is this".

  The ring ids (`claude`, `claude:<8>`, `codex`, …) do not change, so `notch_slots` and
  `tray_slots` keep working. A resolver maps ring id → identity → `AccountMeta`.
- **Migration** in `load()`: each `claude_names` entry becomes an `AccountMeta`, with the tag
  taken from its initials and deduplicated. The old field stays readable (serde default) and is
  no longer written.
- **Defaults** for an unknown identity:
  - name = the provider label, plus `(<first 8>)` when that provider already has an account;
  - tag and color = what slice 3 used before this slice.
- **Ring center, card and pet** read `name`, `tag` and `color` from one command. That replaces
  the page-side fallbacks from slice 3.

### 2. Commands and events

- `get_accounts() -> Vec<AccountView>`: one row per detected or saved account, carrying
  `{key, ring_id, provider, name, tag, color, plan, detected_via, identity_short, snap, config_dir,
  is_new}`. `plan` comes from what the readers already know (the Codex credential has `plan`;
  Claude reads its plan from `~/.claude.json` only if that is cheap; otherwise leave it empty,
  never guess).
- `set_account(key, name, tag, color)`: validates length, tag uniqueness and a palette color.
  It returns `Err` with a message the page shows next to the field.
- `link_config_dir(key, path)` and `unlink_config_dir(key)`. The path must exist and contain
  `settings.json` or `CLAUDE.md` (Claude), or `config.toml` (Codex).
- `hide_account(key)` / `unhide_account(key)`.
- Event `accounts`, emitted when a reader finds an identity with no saved `AccountMeta`
  (`is_new: true`). The Contas banner ("Nova conta detectada · Dar um nome · Ignorar") comes
  from it, and so does a dot on the Contas rail item.

### 3. The Contas screen

- The banner for new accounts.
- One card per account, holding:
  - the ring with its tag;
  - name, plan chip, provider, and the short identity (`org 3f2a…c91e`);
  - 5 h and week percentages;
  - **Detalhes ›**.
- **Inline edit** (the pencil, or Dar um nome): name, tag, and a color picked from the palette.
  Save or Cancelar. `Enter` saves and `Esc` cancels.
- Side column:
  - "Na pílula": the rings in order, each with a drag handle and a switch. It writes
    `notch_slots`.
  - "Como identificamos": the source for each provider, as in the table above.

### 4. The Conta screen (config viewer, read-only)

Shown only when the account has a `config_dir`, or it is the default folder for its provider
(`~/.claude`, or `$CODEX_HOME`/`~/.codex`). Otherwise the screen says why it is empty and offers
**Vincular pasta de config**.

| Section | Claude | Codex |
|---|---|---|
| Main settings | `settings.json`: `model`, `permissions.defaultMode`, `outputStyle`, `statusLine` type, `enabledPlugins` names, hook **event names and counts** | `config.toml`: `model`, `approval_policy`, `sandbox_mode`, `model_reasoning_effort` |
| Instructions | `CLAUDE.md`: line count, modified date, first heading | `AGENTS.md`: same |
| MCP servers | names and transport (`stdio` / `http`) from `~/.claude.json` `mcpServers` and `settings.json` | `[mcp_servers.*]` names and transport |
| Hooks | event, matcher, and the executable's file name only | — |
| Skills / plugins | folder names under `skills/`, `plugins/` | — |
| Health | NotchMon hook installed (`hooks_install`), credential found (`usage.rs` probe: the file on Windows, the login Keychain on macOS), transcripts present (`<dir>/projects`) | `auth.json` usable (`codex.rs` probe), sessions present |

**Opening a file** hands the path to the default editor (`open_path`, the same mechanism as
`open_data_dir`). NotchMon never writes to these files.

### 5. Redaction is a security boundary

- **Parsing happens in Rust.** A new module, `account_config.rs`, returns a typed
  `ConfigSummary`. The page never receives a raw file or a raw JSON value.
- **Allowlist, not denylist.** Only the fields named in the table above leave Rust.
- **What never leaves Rust:**
  - the values of `env`;
  - MCP `args`, `env`, `headers` and `url` (only the transport and the host are shown);
  - hook command arguments;
  - anything under `oauthAccount`, and every key matching `token|secret|key|password|auth`.
- **`env` shows as a count**: "4 variáveis · valores ocultos".
- **Tests**: fixtures with fake secrets in every field listed above. The serialized
  `ConfigSummary` must not contain any of them. This test is part of the slice's acceptance.
- **Codex needs a TOML parser.** Add the `toml` crate, the only new dependency in this slice.

## Build order

One commit per phase.

1. Data model, migration from `claude_names`, the resolver, and unit tests for migration and
   tag uniqueness.
2. Commands, the `accounts` event, and the Contas screen with inline edit and pill order.
   Slice 3's rings switch over to `get_accounts`.
3. `account_config.rs` with the redaction tests, then the Conta screen.

## Acceptance

- An upgrade with `claude_names` set shows the same names and assigns tags. No ring reorders.
- Renaming an account updates the pill, the card, the pet and the hub with no restart.
- The redaction test passes, and a manual look at the Conta screen shows no secret from a real
  config.

## Left for later

- Using a linked `config_dir/projects` as an extra token source for the companion (roadmap).
- Antigravity and Cursor config viewers (their user config lives in app state, not readable
  files).
- Plan detection for Claude when it is not cheap to read.
