# NotchMon: project rules

NotchMon is an open-source desktop app (Rust + Tauri 2, web UI in `notchmon/ui/`). It shows AI
usage limits on the screen edge, spend and analysis, and a pet that grows on your tokens. The goal
is **a popular open-source tool for developers**, so three properties outrank any feature: a
one-command install from GitHub (`cargo install --git … notchmon notchmon-hook`), stability, and
Windows + macOS support. There are no installers, signing or package managers until
non-developers ask; CI keeps the README install command working on both OSes.

- Roadmap and slice plans: `docs/plans/2026-09-24-cohesive-ui-00-roadmap.md` (the index).
- Where the code and ideas come from, and what we owe: `docs/UPSTREAM.md`.
- Growth notes and the launch gate: `docs/GROWTH.md`.
- Open items left by past plans: `PENDINGS.md` (every plan writes there; follow its format).

## Build and check

- `cargo build -p notchmon` and `cargo test --workspace`.
- `cargo clippy --workspace -- -D warnings` and `cargo fmt --check` (CI runs all four on Windows
  and macOS).
- `notchmon doctor` prints what every reader found. Run it after touching a reader.

## Rules

- **Both OSes or gated.** A feature works on Windows and macOS. If it can't yet, it is
  `cfg`-gated and shows one `doctor` line ("not supported on macOS yet"), with a `PENDINGS.md`
  entry. It is never half-broken on one OS. No hard-coded `%APPDATA%` or `~/Library` paths
  outside `config::data_dir()` and the per-OS path table.
- **Parsers need fixtures.** A change to anything that reads another tool's files or JSON
  (transcripts, Desktop cache, rollouts, `state.vscdb`, codeburn, PokéAPI) adds or updates a
  scrubbed fixture under `notchmon/tests/fixtures/<reader>/`. Readers fail small: one cell shows a
  dash and the log says why. The app keeps running.
- **Secrets never leave Rust.** Credentials, tokens, `env` values and OAuth blocks are never sent
  to a page, a log, `doctor` output or the GitHub backup. Config views use an allowlist, not a
  denylist.
- **Nothing leaves the machine silently.** Every outbound host is listed in the README privacy
  table. There is no telemetry.
- **Every Tauri window is in `notchmon/capabilities/default.json`**, or `event.listen` fails
  silently in that page.
- **Zero-dependency core.** The pill, the rings and the pet work with no Node and no extra CLI.
  codeburn, `gh` and `agy` are optional power-ups, detected and explained when missing.
- **Credit what we borrow.** Code or assets from upstream go into `NOTICE.md`. A ported fix names
  its source commit (`Ported from vinzdg/codenotch@<sha>`) and updates the sync table in
  `docs/UPSTREAM.md`.
- **Pokémon IP.** No Pokémon assets in the repository or the binaries: sprites and data come from
  PokéAPI at runtime. "Pokémon" never appears in the repo name, description, topics or app name.
  Keep the "unofficial fan project" disclaimer.
- **UI strings** come in English, Portuguese (pt-BR) and Russian, wherever the page already has
  that language. English is the fallback.
- **Design tokens** live in `notchmon/ui/theme.css` (slice 01). Do not add page-local colors. The
  reference page linked in the roadmap is the source of truth over the design canvas.

## Helping with growth

When asked about popularity or promotion, start from `docs/GROWTH.md`: respect its launch gate and
its "what not to do" list. Check current star counts with `gh api repos/<owner>/<repo>`. Add a
dated line to its log for anything tried.
