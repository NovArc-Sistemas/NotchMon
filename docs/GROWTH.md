# Growth notes

A working document, not a plan. Record what we tried, the date and the result. Numbers are from
the day they were taken.

## Where we stand (2026-09-25)

| Repo | Stars | Forks | Notes |
|---|---|---|---|
| NovArc-Sistemas/NotchMon | 3 | 0 | description still says "Codenotch for Windows" |
| vinzdg/codenotch | 2,493 | 383 | macOS + official Windows port since v1.18.0 (2026-09-24) |
| getagentseal/codeburn | 11,238 | 861 | CLI + desktop on Windows, macOS, Linux; Microsoft Store |
| chattymin/PokeTokenBar | 476 | 131 | macOS only; [issue #299](https://github.com/chattymin/PokeTokenBar/issues/299) asks for a Windows/Tauri version |

The pull of these projects (quick glance, dollars, a pet) is proven. Our unique angle is **the pet
on Windows** first, and **all three in one app** on both OSes.

## Launch gate

Do not promote the project before all of these are true. A first impression spent on a broken
install does not come back.

- [ ] Positioning confirmed; repo description, topics and social preview updated.
- [ ] The one-line `cargo install --git` works from a clean machine on both OSes (slices 06, 07).
- [ ] CI badge green on both OSes (06).
- [ ] README: a 20-second GIF at the top, the pill-states board, a privacy table, credits and the
      disclaimer.
- [ ] Issue templates plus "Copiar diagnóstico" (08), so the first wave of bug reports is useful.

## First audiences, in order

1. **PokeTokenBar users on Windows.** Issue #299 is literally that request. After launch, leave
   one short, respectful comment there: what NotchMon is, that it ports their game faithfully
   with credit, and a link. Ask the PokeTokenBar maintainer if they would mention it in their
   README. Never spam their tracker.
2. **Upstream Codenotch.** Contribute reader fixes back as PRs (see `UPSTREAM.md`). Goodwill
   there is worth more than any post.
3. **Claude Code / Codex communities.** Places where people share Claude Code tools. Show the pet
   hatching, not a feature list. Lead with the GIF.
4. **Brazilian dev community.** The app ships in Portuguese. Post in PT-BR, where the author has
   a network.
5. **codeburn users.** NotchMon uses codeburn for deep analysis. Ask to be listed among
   codeburn's integrations once the Uso screen is solid.

## Tactics worth trying

- **The GitHub profile card (slice 05) is a growth loop.** Every user who puts their pet on their
  profile README advertises NotchMon, with a link back. Make the card beautiful and link it to the
  repo.
- **Releases as content.** Each slice shipped after launch (03, 04, 05) gets a short release note
  with a GIF. Post it where the first announcement was.
- **Good first issues.** A new provider reader with its fixture is a perfect first contribution;
  label a few.
- **Respond fast.** In the first weeks, answering issues within a day matters more than features.

## What not to do

- Headline "Codenotch" or "Pokémon" anywhere. The first is not ours; the second is an IP risk
  that grows with visibility.
- Promote before the launch gate.
- Buy stars or post the same text in many places.

## Log

| Date | What | Result |
|---|---|---|
| 2026-09-25 | This document; roadmap reordered for open source | — |
| 2026-09-25 | Distribution decided: developers install from GitHub with `cargo install`; no installers yet | Rust toolchain is the entry cost; revisit with prebuilt zips if non-developers ask |
