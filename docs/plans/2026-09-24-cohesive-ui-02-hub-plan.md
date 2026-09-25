# Cohesive UI 02: the hub window

Slice 2 of [the roadmap](2026-09-24-cohesive-ui-00-roadmap.md). Needs slice 1 (`theme.css`).
Canvas artboards: Hoje (`Main`), Uso, Companheiro, Loja, Mochila, Pokedex, Especie, Ajustes,
AjustesParceiro, AjustesSistema.

## Decision: a real window, not a docked panel

Today `consumo` is a 400 × 700 borderless panel docked beside the pill (`dock_consumo`,
`main.rs:1144`), and `settings` is a separate framed window. The hub replaces both with **one
framed window**: native title bar (Windows caption, macOS traffic lights), resizable, in the
   taskbar or the Dock only while open, 1040 × 700 by default, 880 × 600
minimum. It remembers its size and position in `config.json` (`hub_rect`).

Why not keep docking? The hub now also holds Contas, GitHub and Ajustes, which do not fit 400 px.
The quick look moves to the merged hover card (slice 3), so the path stays
pill → card → hub.

## What ships

1. **Window.** `tauri.conf.json` gets one `hub` window (`url: hub.html`, `decorations: true`,
   `visible: false`, `skipTaskbar: false`). The `consumo` and `settings` windows go away.
   `capabilities/default.json` lists `hub`: without it, `event.listen` fails silently in the page
   (it happened before, see the layout memory).
2. **Commands.**
   - `open_hub(section: String)` shows, unminimizes and focuses the window, then emits
     `hub_open` with the section.
   - Sections: `hoje`, `uso`, `parceiro`, `loja`, `mochila`, `pokedex`, `contas`, `github`,
     `ajustes`, `ajustes/parceiro`, `ajustes/sistema`.
   - `open_consumo(provider)` becomes a thin alias for one release, so an old `notch.html` never
     breaks: `all` → `uso`, `companion` → `parceiro`, `bag` → `mochila`, `dex` → `pokedex`,
     `shop` → `loja`, a provider id → `uso` filtered to it.
   - `pet_open_panel(tab)` calls `open_hub` with the same mapping.
   - `open_settings` → `open_hub("ajustes")`.
   - Removed: `close_consumo`, `consumo_height`, `dock_consumo`, `CONSUMO_H`, and the
     `consumo_visible` emits. The notch no longer holds its card for a panel that sits over it.
3. **`ui/hub.html`**: rail, header, one `<section>` per screen, hash routing (`#uso`,
   `#ajustes/sistema`), so `hub_open` just sets `location.hash`. The screens are ported from the
   current pages, not rewritten. Their data calls stay the same.

   | Screen | Data (existing commands and events) |
   |---|---|
   | Hoje | `get_companion` / `companion`; `get_tokens` / `tokens` (today, week, month); `get_codeburn` / `codeburn` (`last24h`, today's `days`); `get_usage` + `get_claude_accounts` + `get_codex` + `get_cursor` + `get_antigravity` (limits); `get_state` / `state` (`sessions`: `state`, `attn`) + `get_activity` / `activity` for the "Agora" card |
   | Uso | `get_codeburn` (periods, days), `refresh_usage`; filter **by provider** (all, claude, codex, cursor, gemini) + period (today, 7 days, 30 days, month). There is no per-account filter (roadmap). |
   | Parceiro / Loja / Mochila | `get_companion`, `companion_action`, `get_sprite`, `companion_event`; the balance pill = spendable tokens from the view |
   | Pokédex / Espécie | `get_companion` (dex, catches), `get_pokemon_details`, `get_sprite`; "Fixar na pílula" → `set_companion_prefs` |
   | Ajustes · Superfícies | `get_notch_slots`/`set_notch_slots`, `get_ring_reads`, `get_tray_config`/`set_tray_config`/`get_tray_preview`/`get_tray_options`, `get_ui_flags`/`set_ui_flags`, `get_scale`/`set_scale`, pet prefs in `get_companion_prefs` |
   | Ajustes · Parceiro | `get_companion_prefs`/`set_companion_prefs` (animation, limit display, bubbles, tray sprite, difficulty) |
   | Ajustes · Sistema | `get_autostart`/`set_autostart`, `get_lang`/`set_lang`, `get_hooks_installed`/`set_hooks_installed`, `open_data_dir`, doctor output (new `run_doctor` command wrapping `doctor.rs`), `get_antigravity_prefs`/`set_antigravity_prefs` |

4. **Hoje** is the only new screen. Its contents:
   - the companion card, which links to Parceiro;
   - four tiles: tokens today, cost over 24 h, calls, cache;
   - "Agora": live sessions, showing the provider glyph and the state `trabalhando` or
     `esperando você` (`attn` is set), with a click that calls `focus_session`;
   - every account's limits (tag, name, plan, 5 h bar, week bar);
   - the Rare Candy line.
   Nothing on Hoje is computed in the page that Rust does not already send.
5. **Parceiro groups three screens.** Loja and Mochila become sub-tabs (chips in the header),
   and the balance pill stays in the header on all three. Pokédex gets its own rail item. The
   Pokédex and Capturas segments stay as they are today.
6. **Ajustes** takes over every settings pane:
   - Superfícies = today's `tray` and `notch` panes plus the pet;
   - Parceiro = today's `companion` pane minus the pet;
   - Sistema = today's `behaviour`, `hooks` and `about` panes.
   The "Contas na pílula" row only links to Contas (slice 4).
7. **i18n.** The hub carries en, pt and ru, like `settings.html` does today (`RU_STATIC`,
   `PT_STATIC`, `STATUS_TEXT`). All three dictionaries move into `ui/i18n.js`, which
   `notch.html` and `pet.html` also load, so a string is translated once. Every new string gets
   all three languages. English is the fallback for a missing key. Today `consumo.html` has only
   en and pt (`lang_resolved === 'pt' ? 'pt' : 'en'`), so the ported Uso and Parceiro strings need
   Russian added. Alternatively, drop ru from the hub on purpose and record that in `PENDINGS.md`.
   Decide before phase 2.
8. **Platform.** Nothing in `hub.html` may assume Windows. Paths come from Rust (`data_dir()`,
   never `%APPDATA%` in the page), keyboard hints show `Ctrl` or `⌘` by `navigator.platform`,
   and "Abrir pasta" uses the opener that works on both OSes. On macOS, closing the last
   window never quits (the app lives in the menu bar).
9. **The tray menu's "Settings…" item** opens the hub on Ajustes. The rest of the menu is slice 3.

## Build order

One commit per phase.

1. Window, capabilities, `open_hub` with the aliases, and an empty `hub.html` with the rail and
   routing. Old pages still work.
2. Port Uso (from `consumo.html`), Parceiro, Loja, Mochila and Pokédex. Retire `consumo`.
3. Port Ajustes (from `settings.html`) and `i18n.js`. Retire `settings`.
4. Hoje, `run_doctor`, and saving `hub_rect`.

## Acceptance

- Every entry point lands on the right screen: pill click, card link, pet click, pet menu, tray
  item, the gear in the pill.
- `get_webview_window("consumo")` and `"settings"` no longer appear anywhere in `src/`.
- Closing the hub hides it (the app keeps running); reopening it restores its size and position.
- The UI in pt, en and ru shows no raw keys.

## Left for later

- Keyboard shortcuts per section (`Ctrl+1…7`).
- The `open_consumo` alias: remove one release after slice 3 ships.
