# Windows: CodeBurn inside the notch (Consumo)

Approved visually on 2026-09-15 through the brainstorm previews (`.superpowers/brainstorm/`,
local only). This is the design and the build order in one page.

## What ships

1. **Pill 84 px** (was 70). Percentages spread with equal space to the edges and between them,
   14 px / 600.
2. **NOVARC cell** at the top of the pill: the official negative symbol (design system
   `logo-symbol-negative.svg`, inline, never redrawn) in a 56 px box like a ring, and below it
   the API-equivalent dollars of the **last 24 rolling hours** in the percentage font. Hover
   shows the split per tool; click opens the Consumo panel on "Todos".
3. **Fold tab** on the pill's left edge, vertically centred, one SVG outline whose concave
   fillets flow into the pill edge; the chevron is drawn, not a glyph. Folded, the tab alone
   stays on the screen edge. Hidden while the hover card is open.
4. **Consumo in the hover cards** of Claude (every account: codeburn cannot tell the accounts
   apart, so the section says so) and Codex: period tabs Hoje / 7 dias / 30 dias / Mês, cost,
   calls, sessions, 14-day bars, top models, top activities, and "Ver tudo".
5. **Consumo panel**: a separate focusable window docked beside the pill, sized to its content
   so it never scrolls. Provider Todos / Claude / Codex, the same periods, six figures, a
   GitHub-style 26-week activity grid (Sessões / Tokens / Custo / Chamadas) with a per-day
   popup, then collapsible sections, one open at a time: cost per day, projects, top sessions,
   models (cost per edit, first-try rate), activities, tools and MCPs, skills and subagents,
   waste, reworked files. Closes on ×, Esc, or a second click on the NOVARC cell.

## Deviation from the preview

The per-day popup opens over the panel (above the grid), not outside it: outside would need an
invisible strip of window that swallows clicks meant for the app behind.

## Data (all local, `codeburn` 0.9.x through npm)

| Need | Command | Cadence |
|---|---|---|
| Last 24 h | `export --format json --from <yesterday> --to <today> -o <tmp>`, sum `records[].cost` with `timestamp` in the last 24 h, per `provider` | 10 min |
| Periods | `status --format menubar-json --period <today\|week\|30days\|month> [--provider claude\|codex] --no-optimize` | today 10 min, others 60 min |
| Activity grid, day popup | `sessions --period lifetime --format json`, aggregated per day and tool | 60 min |

One worker thread runs them in sequence, hidden (`CREATE_NO_WINDOW`), 120 s timeout each,
killed on timeout. Raw outputs persist in `%APPDATA%\codenotch\codeburn\` so a restart shows
the last reading, dated. No codeburn installed: the NOVARC cell shows a dash and the Consumo
sections are absent. Rust passes codeburn's own JSON through, trimmed to `current` and the last
14 days, and aggregates only the sessions list.

## Build order

1. Pill 84 px, even percentages, fold tab (notch.html only).
2. `codeburn.rs`: locate, run, persist, trim, aggregate, emit `codeburn`; `get_codeburn`.
3. NOVARC cell + 24 h hover card.
4. Consumo section in the Claude and Codex cards; `NOTCH_H` grows to fit.
5. `consumo.html` window: docking, content-sized height, Esc; then the grid and sections.
6. Tests: the 24 h sum, the per-day aggregation, the trim; doctor line; README.
