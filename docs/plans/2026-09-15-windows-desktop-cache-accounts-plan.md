# Windows: Claude limits from Claude Desktop's cache, one ring per account

## Problem

The Windows port reads Claude's limits only through `~/.claude/.credentials.json`, which only
the standalone CLI writes. Someone who works in Claude Desktop's Code tab gets a ring for
whichever account last signed in on the CLI, and nothing for any other account Desktop is
signed into. Two subscriptions, one ring, and it is not necessarily the account in use.

## Deliberate divergence from the Mac app

The Mac app draws one ring per Claude Code *profile* (`~/.claude-<slug>`) and uses Desktop's
cache only to fill the ring whose organization matches. A Desktop user switching between two
accounts in one profile has one profile and two organizations, so under that model the second
organization never gets a ring. Here accounts are discovered from Desktop's own cache instead:
every organization with a live usage entry is an account.

## Source

Desktop is Chromium. On Windows its HTTP cache (`%APPDATA%\Claude\Cache\Cache_Data`) is the
**blockfile** backend, not the Simple Cache the Mac reader parses:

- `data_1`: 256-byte `EntryStore` blocks after an 8192-byte header. Key at +96 (`key_len` at
  +32, `long_key` at +36), stream sizes at +40, stream addresses at +56, `state` at +20.
- Header allocation bitmap at +80: an entry whose block bit is clear was freed and is ignored,
  as is any `state != 0`.
- `CacheAddr`: bit 31 initialized, bits 28–30 file type (0 external `f_%06x`, 2/3/4 =
  256 B/1 KB/4 KB blocks), bits 16–23 file number, bits 0–15 start block.
- Stream 0 holds the response headers (`\0date:` found by search), stream 1 the body,
  zstd-encoded, same JSON as the OAuth endpoint (`five_hour`, `seven_day`, `limits`), so
  `usage::parse_response` decodes it.
- Two keys per organization (`/usage`, `/usage?skip_spend=1`), refreshed alternately: pick
  the newest by `Date`, never by position.
- Desktop holds these files open with delete access: open with
  `share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)` or every read fails.
- Every size taken from the file is bounded before it is believed.

Validated on a live cache on 2026-09-15: both organizations present, entries live, the
active account refreshed every few minutes, the inactive one frozen at its last switch.

## Cells

- `claude` stays the account in `~/.claude.json` `oauthAccount.organizationUuid`. Desktop's
  reading for that organization, when under 30 minutes old, is used first and the network is
  skipped; otherwise the existing OAuth path runs unchanged.
- Every other organization in the cache is a cell `claude:<first 8 of the org>`, fed only
  from the cache, never from the network. `fetched_at` is the response's `Date`, so the
  existing stale dimming and "Updated N ago" apply unchanged.
- No CLI credential at all: the first organization in the cache (by UUID, so rings never
  swap) takes the `claude` cell.
- Labels: `Claude (<org8>)`. No name source yet; a name override is a later change.
- The session arc stays on `claude`: sessions carry no account.

## Touch list

`desktop_cache.rs` (new) · `usage.rs` (cache first for `claude`, extras thread) · `main.rs`
(`AppState`, `snapshot_of`, `ring_window`, `provider_label`, tray ids, refresh) · `tray.rs`
(refresh) · `glyphs` lookup (strip `:suffix`) · `notch.html` (`providers()`, card) ·
`doctor.rs` (one line) · `Cargo.toml` (`ruzstd`, pure-Rust zstd decoder).

## Check

One unit test on a synthetic `data_1`/`data_3` pair: header with bitmap, one live entry, one
freed entry, a zstd body. The live one is read, the freed one is not, and the newer of two
keys wins.
