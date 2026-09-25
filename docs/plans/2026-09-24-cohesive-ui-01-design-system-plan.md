# Cohesive UI 01: the design system

Slice 1 of [the roadmap](2026-09-24-cohesive-ui-00-roadmap.md). Visual reference: https://claude.ai/artifact/T18BRUuUfXr4JdF1yyT62L.

## Problem

Each page carries its own inline `<style>`. Across `notch.html`, `consumo.html`, `settings.html` and
`pet.html` there are about 20 greys (`#808080`, `#8a8a8a`, `#9a9a9a`, `#b0b0b3`, `#c8c8c8`, …).
The brand blue `#0a84ff` and Codex `#6ab7ff` read as the same color. Cards, buttons and toggles are
redrawn slightly differently in every page.

## What ships

1. **`notchmon/ui/theme.css`**, linked first by every page (`<link rel="stylesheet" href="theme.css">`).
   `frontendDist` is the static `ui/` folder with no bundler, so a shared stylesheet is the whole
   mechanism. It holds tokens and component classes, nothing page-specific.
2. **Tokens** (CSS custom properties on `:root`). The app stays dark-only.

   | Token | Value | Role |
   |---|---|---|
   | `--bg` | `#0b0b0c` | window ground |
   | `--s1` | `#121214` | card |
   | `--s2` | `#18181b` | inset, segmented track, hover |
   | `--s3` | `#202024` | raised control, bar track, selected rail item |
   | `--line` | `#26262b` | card border, dividers |
   | `--line2` | `#34343a` | control border |
   | `--tx` | `#ededef` | text |
   | `--mut` | `#a3a3ab` | secondary text (6.9:1 on `--s1`) |
   | `--dim` | `#8a8a93` | labels, captions (5.3:1 on `--s1`) |
   | `--acc` | `#0a84ff` | NotchMon: XP, selection, primary button, links |
   | `--acc-ink` | `#08101f` | text on `--acc` (white on `#0a84ff` fails 4.5:1) |
   | `--acc-soft` | `#10233f` | tinted fill (balance pill, new-account banner) |
   | `--p-claude` | `#FF7A1A` | provider Claude |
   | `--p-codex` | `#d9dde5` | provider Codex (was `#6ab7ff`) |
   | `--p-cursor` | `#b28cff` | provider Cursor |
   | `--p-gemini` | `#5ad1a4` | provider Antigravity |
   | `--warn` | `#f0b43c` | limit 70 % or more |
   | `--hot` | `#f0605a` | limit 90 % or more, destructive |
   | `--ok` | `#6fd49a` | health checks, active item |
   | `--desk` | `#1b1d22` | canvas stand-in for the desktop (mockups only) |

   Radii: 14 card, 12 sprite frame, 9 control, 7 segment, 999 chip. Spacing uses 4 px steps;
   card padding 16–20 px, page padding 22 × 28 px. Type: the system face, 13 px body (`--font: "Segoe UI", -apple-system,
   BlinkMacSystemFont, system-ui, sans-serif`), 11 px uppercase label (`letter-spacing .06em`),
   19 px page title, numbers in `--mono: ui-monospace, "SF Mono", Consolas, monospace` with
   `tabular-nums`. No webfont is bundled, so both OSes look native and nothing loads from the
   network.
3. **The color rule**, written once in `theme.css` as a comment and followed everywhere:
   - `--acc` belongs to NotchMon: XP bars and rings, selection, primary actions. It is never a
     provider color.
   - A provider color appears only where the provider is the subject: the provider split bar, a
     legend, a provider glyph.
   - An account's **tag color** (slice 4) paints that account's ring and tag. It defaults to its
     provider color, and a second account of the same provider takes the next free color from
     `#ef7fae`, `#d6b98c`, `#8fd3e8`. Amber, red and green are never offered, because they mean
     severity.
   - Limit percentages are `--tx` below 70 %, `--warn` from 70 %, `--hot` from 90 %. One helper,
     `sevColor(pct)`, lives in `theme.js` (below).
   - Data charts (heatmap, daily bars) use one `--acc` ramp: `--s2`, `#12305e`, `#0f4ea6`,
     `#0a6be0`, `--acc`.
4. **Component classes**: `.card`, `.lbl`, `.btn` (`.pri`, `.ghost`, `.sm`), `.chip` (`.on`), `.seg`
   (buttons with `.on` + `aria-selected`), `.bar > i`, `.badge.r-raro|r-incomum|r-comum|r-lendario`,
   `.tag`, `.sw` (a real `<button role="switch" aria-checked>`), `.set` rows (title, description,
   control), `.stat` tiles, `.spr` sprite frame, `.mono`. The markup for each is in the reference page.
5. **`notchmon/ui/theme.js`**: the few helpers every page redefines today. `sevColor(pct)`,
   `fmtTokens(n)` (`202.6M`, `1.38B`), `fmtUsd(n)`, and `ring(el, outer, inner, color, label)`, which
   draws the dual ring as SVG (outer = 5 h window, inner = week at 55 % opacity).
6. **Restyle in place**: `notch.html`, `pet.html` and the two pages retired later still move onto
   `theme.css` now. This slice changes looks, not layout. `PCOL` in `consumo.html:170` and the
   hard-coded `#FF7A1A` in `notch.html:290` read the CSS variables instead.

## Build order

One commit for the whole slice, on its own branch.

1. `theme.css` + `theme.js`; link them from all four pages.
2. Swap page-local greys for tokens, one page at a time. Stop at any value with no token and
   decide: map it, or add a token. Never add a third grey for one spot.
3. Provider colors through variables; Codex to `--p-codex`.
4. Hand check: open every window at 100 % and 150 % DPI on Windows, and on a Retina Mac once
   slice 07 builds there. Compare with the reference page.

## Acceptance

- `rg "#[0-9a-fA-F]{6}" notchmon/ui/*.html` finds only intentional one-offs (sprite shadows,
  glyph SVGs), each with a comment.
- Nothing blue in the app means "Codex" any more.
- `cargo build -p notchmon` passes, and so does the `doctor` run. No Rust changes are expected.

## Left for later

- A light theme. The app is dark-only on purpose, and the tokens make a light theme possible later.
