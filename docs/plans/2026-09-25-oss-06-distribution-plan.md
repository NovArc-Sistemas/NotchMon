# OSS 06: install from GitHub, CI and open-source hygiene

Part of [the roadmap](2026-09-24-cohesive-ui-00-roadmap.md). This is the **first** slice to build:
nothing else can be verified on macOS, or trusted by contributors, without it.

## Decision: developers install from GitHub, not through an installer

Decided on 2026-09-25. NotchMon's users run Claude Code, Codex, Cursor or Antigravity, so they are
developers. They install with **one command from the GitHub repository**. This slice ships none
of these:

- installers (NSIS, `.dmg`);
- code signing or notarization;
- an auto-updater;
- winget, Scoop or Homebrew packages;
- a Microsoft Store listing.

That removes the costs and the SmartScreen/Gatekeeper friction, and it keeps the release process
at "push a tag".

## Goal

A developer with a Rust toolchain goes from the README to a running pill with one command, on
Windows or macOS. CI proves on every PR that this command still works on both.

## The install command

```sh
cargo install --git https://github.com/NovArc-Sistemas/NotchMon --locked notchmon notchmon-hook
notchmon
```

- **Both binaries land in `~/.cargo/bin`.** This already works: `hooks_install.rs` finds the
  hook helper next to `current_exe()`, and `autostart.rs` registers `current_exe()`.
- **Updating** is the same command with `--force`. `notchmon --version` prints the commit it was
  built from (`git rev-parse --short HEAD` baked in by `build.rs`), so bug reports say exactly what
  is running.
- **A pinned release** is `--tag vX.Y.Z`.
- **Prerequisites.** The README has a short table; each row links to the official install page.

  | OS | Needs |
  |---|---|
  | Windows | Rust via rustup, with the MSVC build tools rustup offers. WebView2 ships with Windows 10/11. |
  | macOS | Rust via rustup; Xcode Command Line Tools (`xcode-select --install`). |

- **First build time.** It takes a few minutes, because `rusqlite` bundles SQLite. The README
  says so, so nobody thinks it hung.
- **From a clone**, for contributors: `cargo run -p notchmon`.

## What ships

1. **CI** (`.github/workflows/ci.yml`) on `windows-latest` and `macos-latest`:
   - `cargo fmt --check`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace`;
   - **the exact README install command** against the PR's own commit
     (`cargo install --path notchmon --locked` + `--path notchmon-hook`), so the README can never
     go stale;
   - `Swatinem/rust-cache`;
   - until slice 07 lands, macOS runs `cargo check` over the `cfg(not(windows))` stubs.

   The job is required on `main`, and the README shows the badge.
2. **Releases are tags.** `vX.Y.Z` tags on `main`, with notes in GitHub Releases (generated notes,
   edited by hand). No binaries are attached. The README install command stays the same; the tag
   is only there for people who want a pinned version.
3. **`notchmon --version` and a startup check for updates.**
   - Once a day the app compares its baked-in commit with the latest tag (one GitHub API call,
     listed in the privacy table).
   - When a newer tag exists, it shows a dot on Ajustes → Sistema with the command to update, as
     selectable text with a copy button.
   - It never downloads anything itself.
   - It can be switched off.
4. **Zero-dependency core: spend without codeburn.** Developers may not have Node 22.13+, and
   "install another CLI" is friction.
   - The spend cell ("Custo · 24 h") is computed in Rust from `tokens.rs` × `prices.json`, which
     is bundled with the app and updated by PR.
   - A model with no known price shows `—`, never a guess.
   - codeburn stays for the Uso screen's deep analysis. When it is absent, Uso shows what
     `tokens.rs` knows, plus one line with `npm i -g codeburn` as selectable text.
   - **This reopens a settled decision** ("Uso = codeburn panel untouched", 2026-09-16). It needs
     the maintainer's yes before it is built.
5. **First run.** Because developers start from a terminal, the terminal does the onboarding:
   - `notchmon` prints three lines: where the data folder is, which tools were found, and how to
     open Ajustes.
   - The hub opens once, on Ajustes → Sistema. It shows the same list with the "Instalar hook"
     button, and "Iniciar com o sistema" is off until the user turns it on.
   - `notchmon doctor` stays the full report.
6. **Repository hygiene.**
   - **`CONTRIBUTING.md`**: build on each OS, run tests, the fixture rule (slice 08), commit
     style, `PENDINGS.md`.
   - **`SECURITY.md`**: private reporting via GitHub; what the app reads; what it never sends.
   - **`CODE_OF_CONDUCT.md`**: Contributor Covenant.
   - **`.github/ISSUE_TEMPLATE/`**:
     - `bug.yml` asks for OS, `notchmon --version` and the output of `notchmon doctor --json`
       (slice 08);
     - `feature.yml`;
     - `provider.yml` ("my tool is not read").
   - **`.github/pull_request_template.md`**: tested on which OS; fixtures added.
7. **README.**
   - The install block first: the command, the prerequisites table, the first-build note.
   - A 20-second GIF.
   - The pill-states board (repo presentation memory).
   - A privacy table listing every host contacted and what it receives, in PokeTokenBar's shape.
   - Credits, with links to `docs/UPSTREAM.md`.
   - **The Pokémon disclaimer**: unofficial fan project, not affiliated with Nintendo, Game Freak,
     Creatures or The Pokémon Company, no Pokémon assets in the repository or builds, sprites
     fetched at runtime from PokéAPI. It also goes in Ajustes → Sistema → Sobre. The repo name,
     description and topics never contain "Pokémon".
8. **Repository settings** (maintainer's clicks, listed in `docs/GROWTH.md`): description, topics,
   social preview image, Discussions on, private vulnerability reporting on.

## Build order

One commit per phase.

1. `ci.yml` green on both OSes, including the install-from-path step.
2. `--version` with the commit, the hygiene files, the issue templates, and the README restructure
   with the disclaimer.
3. Terminal onboarding, and the daily tag check with the copy-the-command dot.
4. Built-in spend from `prices.json` (after the maintainer's yes on item 4).

## Acceptance

- On a clean Windows machine with only rustup installed, the README command builds and `notchmon`
  shows the pill. The spend cell has a value without Node.
- The same on macOS once slice 07 lands.
- A PR that breaks `cargo install` fails CI.
- Re-running the command with `--force` keeps `config.json` and `companion.json`.

## Left for later

Add these only when non-developers show up asking.

- **Prebuilt binaries** on Releases (a zip per OS from the same CI). This is nearly free, since CI
  already builds, and it removes the Rust toolchain requirement. Without signing, they carry the
  SmartScreen and Gatekeeper warnings.
- Installers, signing, an auto-updater, and winget, Scoop, Homebrew or Store listings.
- Linux. Tauri supports it; `cargo install` may already work there once the `cfg` stubs cover it.
