# Upstream projects

NotchMon stands on four projects. Each relationship is different, and so is what we owe it and
what we watch.

| Project | Relationship | License | What we take | What we owe |
|---|---|---|---|---|
| [Im-Midi/codenotch-windows](https://github.com/Im-Midi/codenotch-windows) | **Fork lineage.** This repo's Rust/Tauri code descends from it. | MIT text in the file (GitHub’s license detector shows NOASSERTION) | the whole app skeleton: readers, pill, tray, hook helper | keep their copyright line in `LICENSE` |
| [vinzdg/codenotch](https://github.com/vinzdg/codenotch) | **Design lineage and sibling.** The original macOS notch (Swift). Its `windows/` folder now carries the same Im-Midi-derived port, actively maintained. | MIT; the name and design belong to the author | the notch design and rings; provider reader logic; the macOS Swift readers as the reference for slice 07 | never headline "Codenotch"; credit it in README and `NOTICE.md`; icons derived from theirs are listed in `NOTICE.md` |
| [getagentseal/codeburn](https://github.com/getagentseal/codeburn) | **Runtime dependency (optional).** Spawned as a CLI, never vendored. | MIT | dollars per project, model and activity for the Uso screen | credit; do not break when it is absent; parse only tested versions (`compat.toml`, slice 08) |
| [chattymin/PokeTokenBar](https://github.com/chattymin/PokeTokenBar) | **Port.** The companion game, re-implemented in Rust with the same mechanics and numbers. | MIT | game rules and numbers; the "License & disclaimer" wording for Pokémon IP | credit as the game's author; keep the numbers faithful, or say where they differ |

Data sources that are not projects we build on, but that we depend on: PokéAPI (species, sprites,
fetched at runtime and never bundled) and each AI tool's own files and endpoints.

## Syncing fixes from upstream Codenotch `windows/`

Upstream fixes the same provider readers we have (`usage.rs`, `codex.rs`, `cursor.rs`,
`antigravity.rs`, `agy_cli.rs`, `activity.rs`, `focus.rs`, …). This is the cheapest source of
stability we have.

- **Once a month**, or when a provider breaks:
  1. list upstream commits touching `windows/` since the last sync;
  2. port the reader fixes that apply;
  3. record the upstream commit below.
- **Port behaviour, not files.** Our modules have diverged (multi-account, tokens, companion).
- **Credit a port** in the commit message: `Ported from vinzdg/codenotch@<sha>`.
- **Give fixes back** that help them too (for example a Desktop cache edge case) as PRs upstream.
  It is good citizenship, and it builds goodwill with the largest related audience.

| Module | Last upstream commit reviewed | Date |
|---|---|---|
| all readers | not yet synced; the fork point is Im-Midi's last commit before 2026-09-14 | — |

## Pokémon intellectual property

Pokémon names, characters and sprites belong to Nintendo, Game Freak, Creatures and The Pokémon
Company. The more visible NotchMon gets, the more this matters.

What we do now, the same as PokeTokenBar:

- no Pokémon assets in the repository or the binaries;
- sprites and data fetched at runtime from PokéAPI and cached on the user's machine;
- a clear "unofficial fan project, not affiliated" disclaimer in README and in the app;
- "Pokémon" never appears in the repo name, description or topics;
- no monetization tied to the game.

A possible future hedge, not planned: the game engine is species-agnostic enough that a
"creature pack" with original art could become the default one day. Record the idea here; do not
build it until there is a reason.
