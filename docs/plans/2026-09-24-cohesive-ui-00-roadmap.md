# NotchMon roadmap: open source, cross-platform, one coherent app

Proposed on 2026-09-24 and extended on 2026-09-25.

- Redesign canvas: https://claude.ai/artifact/Eve6H3TaBYmXNve4Kei1Bk.
- Developer reference (tokens, components, screen and data map):
  https://claude.ai/artifact/T18BRUuUfXr4JdF1yyT62L.
- Upstream relationships: [`docs/UPSTREAM.md`](../UPSTREAM.md).
- Growth notes: [`docs/GROWTH.md`](../GROWTH.md).

This page is the index. Each slice has its own plan with the design and the build order in one.

## Goal

A popular open-source tool. Three things matter more than any feature:

- **installing is one command** for developers, straight from GitHub (`cargo install --git …`),
  with no Node and no extra CLI;
- **it does not break** when a tool it reads changes;
- **it runs on Windows and macOS**.

## Positioning (decision 1, to confirm)

The repo description still says *"Codenotch for Windows, NOVARC edition"*. That identity is gone:

- upstream Codenotch (vinzdg, 2.5k★) now ships its own Windows port from `windows/`, in the same
  Rust/Tauri stack, and it descends from the same Im-Midi code as this repo;
- the Codenotch name is not ours to headline (`NOTICE.md`).

What nobody else has:

- **the PokeTokenBar game on Windows**. PokeTokenBar (476★) is Swift, macOS only;
- **the notch, the numbers and the game in one app**;
- **multi-account Contas** and **the GitHub backup and profile card**.

**Proposed one-liner**: *"A pet that grows on your AI tokens, with your Claude, Codex, Cursor and
Antigravity limits on the screen edge. Windows and macOS."*

## Today's UI problem (slices 01–05)

Four surfaces that look like four apps:

- the pill with two hover cards;
- a 400 px docked panel, three filter levels deep;
- a separate settings window;
- the pet.

Two Claude rings both read "C", and the brand blue collides with the Codex blue.

## The shape after this work

- **Glance**: pill → one merged card → hub. The pet and the menu use the same card.
- **Hub**: one window with a left rail: Hoje · Uso · Parceiro (Visão / Loja / Mochila) · Pokédex ·
  Contas · GitHub · Ajustes.
- **Contas**: name, tag and color per login; the tag sits in the ring center.
- **GitHub**: private backup, plus an optional public profile card.
- **Install**: `cargo install --git` on both OSes, proven by CI on every PR. There are no
  installers, no signing and no package managers until non-developers ask.

## Slices, in build order

| Order | Plan | Depends on | Why this position |
|---|---|---|---|
| 1 | [06 install from GitHub, CI, OSS hygiene](2026-09-25-oss-06-distribution-plan.md) | — | CI on both OSes first: nothing else can be verified on a Mac without a runner, and CI proves the install command |
| 2 | [08 reliability](2026-09-25-oss-08-reliability-plan.md) | 06 CI | cheap, and it protects every later slice from upstream format drift |
| 3 | [01 design system](2026-09-24-cohesive-ui-01-design-system-plan.md) | — | `theme.css`, color rule, system fonts on both OSes |
| 4 | [02 hub window](2026-09-24-cohesive-ui-02-hub-plan.md) | 01 | retires `consumo` and `settings` |
| 5 | [07 macOS](2026-09-25-oss-07-macos-plan.md) | 02, 06, 08 | after the hub, there is half the UI left to port |
| 6 | [03 glance surfaces](2026-09-24-cohesive-ui-03-glance-plan.md) | 01, 02 | merged card, tagged rings |
| 6 | [04 accounts center](2026-09-24-cohesive-ui-04-accounts-plan.md) | 02 | runs in parallel with 03 |
| 7 | [05 GitHub backup](2026-09-24-cohesive-ui-05-github-plan.md) | 02, 04, 08 | starts with `/codex-review` (auth and user data) |

The **public launch gate** is after order 5 (06, 08, 01, 02, 07 shipped):

- the one-command install working on both OSes;
- a CI badge;
- the README with the pill-states board and the disclaimer.

Slices 03–05 then ship as visible updates, which gives the project a steady stream of release
notes.

## Decisions to confirm

Settled decisions from 2026-09-16 stay: pill A/B/C, tokens as currency, candy rule, one companion
for all Claude accounts, sprites from PokéAPI at runtime.

Proposed, to confirm before the slice named:

1. **Positioning** (above), before 06's README work.
2. **The hub rail replaces "panel A, five tabs"** (2026-09-16), before 02. The tab contents
   survive; the container changes.
3. **Built-in spend without codeburn**, before 06 phase 4. It reopens "Uso = codeburn panel
   untouched": the spend cell comes from `tokens.rs` × a price table, and Uso uses codeburn only
   for deep analysis.
4. ~~Code signing and Store~~: **settled on 2026-09-25**. Developers install from GitHub with
   `cargo install`; there are no installers, no signing, no updater and no package managers for
   now (06).
5. **Brand blue only for NotchMon.** Codex becomes silver `#d9dde5` (teal would clash with
   Antigravity `#5ad1a4`).
6. **Limit severity**: white, amber from 70 %, red from 90 %.
7. **Label lock**: "Tokens hoje" is a calendar day (`tokens.rs`); "Custo · 24 h" is rolling. This
   closes the open point from 2026-09-16.
8. **Fonts**: the system face on each OS, no bundled webfont. The canvas uses Geist only as a
   browser stand-in.
9. **Rail label "Parceiro"** (open): "Companheiro" does not fit 64 px.
10. **Russian in the hub** (open): translate the ported panel strings to ru, or drop ru on purpose.
11. **How macOS gets tested** (open): a maintainer Mac, or CI plus volunteer testers (see 07).

The reference page wins over the canvas where they differ.

## Where the canvas promises more than the data allows

- **There is no usage per account.** codeburn and `tokens.rs` do not split Claude accounts. Uso
  filters by provider; per-account data is limits only.
- **"Agora" rows have no account.** Sessions carry no login.
- **Contas covers all four providers.**

## Standing rules for every slice

These are also in the repo's `CLAUDE.md`.

- A feature ships on both OSes, or it is `cfg`-gated with a `doctor` line and a PENDINGS entry.
  It is never half-broken on one OS.
- A parser change comes with a fixture (08).
- Anything borrowed from upstream is credited in `NOTICE.md`, and `docs/UPSTREAM.md` records what
  was synced.
- Nothing leaves the user's machine unless the user connected it (GitHub) or it is listed in the
  README privacy table.

## Left for later

- Prebuilt binaries on Releases, then installers, signing and package managers, when
  non-developers ask.
- Linux.
- The notch-side layout on MacBooks.
- Tokens from a linked `CLAUDE_CONFIG_DIR/projects`.
- OS toast notifications.
- A built-in OAuth device flow for GitHub.
