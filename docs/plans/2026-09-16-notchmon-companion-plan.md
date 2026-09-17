# NotchMon: the PokeTokenBar companion inside the notch

Decided on 2026-09-16 from the design canvas (https://claude.ai/artifact/Bdc8qCuxiSZG9E6Yt68zAS).
This page is the design and the build order in one.

## What ships

1. **Token readers** (`tokens.rs`). Claude Code (`~/.claude/projects/**/*.jsonl` and the desktop
   mirrors the watcher already knows) and Codex (`~/.codex/sessions/**/*.jsonl`) read directly,
   incrementally (byte offset per file), deduplicated by `message.id|requestId` keeping the largest
   total, as PokeTokenBar does. `totalTokens` = input + output + cache write + cache read. Today,
   this week, this month and the last 35 days per tool; burn rate over the last 10 minutes.
   codeburn stays for dollars and analysis only.
2. **PokéAPI client** (`pokeapi.rs`). Species, evolution chain, base index (GraphQL, REST fallback),
   Pokémon details, all cached under `%APPDATA%\codenotch\pokeapi`; sprites (Gen V static PNG,
   animated GIF, shiny) under `sprites`, served to the pages as data URLs. Nothing bundled.
3. **Companion engine** (`companion.rs`), a port of PokeTokenBar's CompanionStore: egg (5M to
   hatch), weighted species roll by capture rate (collected lines at half weight), rarity from
   capture rate / legendary flag, 25 natures, shiny 1/64 (1/48 with the charm), Ditto disguise 1/128
   on common multi-form lines, evolution through the real tree with a planned branch, graduation
   at 750M / 1.875B / 3B / 6B split over the forms, growth boost ×2 for a line already graduated,
   Rare Candy (+100M) granted when a 5-hour ring (1) or a weekly ring (5) reaches 100 %, shop on
   spent tokens (Mint 100M, Rare Candy 500M, egg 1B / uncommon 2.5B / rare 4B, Shiny Charm 3B),
   individual profile (IVs, gender, ability, level, moves), Pokédex and catch log, difficulty
   10–200 % for growth and prices. Display state: egg, idle, working, focus, tired, sleep, levelUp.
   Persisted in `companion.json` beside the config.
4. **Pill**: companion cell (mode A, default) with the XP ring; mode B (hero on top) and mode C
   (no cell, floating pet) as a setting. Companion hover card. Rare Candy bubble when a ring fills.
5. **Panel** (consumo.html): tabs Uso | Companheiro | Loja | Mochila | Pokédex. Uso is the
   codeburn panel as it was. Pokédex holds the grid, the catch log and the individual profile.
6. **Floating pet** (pet.html): transparent always-on-top window, 48–384 px, hover callout,
   right-click menu, drag, limit bubble.
7. **Settings**: a Companion pane (pill mode, pet, size, representative, animation, limit display,
   notifications, difficulty) and Portuguese alongside English everywhere.
8. **Rebrand**: NotchMon name, README in EN and PT with the pill-states board.

## Build order

Each phase ends with `cargo test` green and one commit on `feat/notchmon-companion-20260916`.
All six shipped on 2026-09-16; the crates and folders were renamed `notchmon` / `notchmon-hook` and
the data folder moved to `%APPDATA%\notchmon` (copied once from `codenotch`). Left for later: OS
toast notifications (the bubbles stand in), Cursor and Antigravity tokens for the companion, save
export/import, Codex fork/subagent replay dedup, a one-time migration of an old "Codenotch" autostart
value (upgraders who had start-at-sign-in on re-enable it under Settings), and a cached tray sprite path
(`tray_sprite_file` builds a PokéAPI client every 2 s while the option is on).

1. tokens.rs with fixture tests.
2. pokeapi.rs with a fake-server test for parsing.
3. companion.rs engine with a fake provider and a seeded RNG; worker thread; commands.
4. Panel tabs, notch cell and card (mode A), settings pane, PT strings.
5. Pill modes B and C, pet window, tray sprite.
6. Rebrand and README.
