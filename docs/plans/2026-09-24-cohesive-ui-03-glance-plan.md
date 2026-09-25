# Cohesive UI 03: glance surfaces (pill, card, pet, tray)

Slice 3 of [the roadmap](2026-09-24-cohesive-ui-00-roadmap.md). Needs slices 1 and 2. Canvas
artboards: Pilula, Pet, Bandeja.

## Problem

`renderCard()` (`notch.html:578`) draws a different card for each thing under the pointer: the
brand card, the companion card (`renderMonCard`), or one provider card. Moving down the pill
swaps the card three times. The two Claude rings both read "C". The pet tooltip and the tray menu
use neither card's style.

## What ships

1. **One card.** Hovering anywhere on the pill opens the same card, from top to bottom:
   - companion (sprite, name, rarity, form, nature, level, XP bar);
   - tokens today and API cost;
   - one limits block per ring in pill order: tag, name, plan, 5 h bar, week bar, percentages
     in severity color;
   - a footer ("outer ring 5 h, inner ring week" and **Abrir NotchMon ›**, which opens the hub on
     Hoje).

   The item under the pointer is **emphasized, not swapped**:
   - its block gets the `--s2` fill;
   - it expands in place to show what the provider card shows today: reset times, "Updated N
     ago" when stale, the `needsAuth` sign-in note, and the Antigravity `group` headings.

   Everything else in the card stays still, so the card never jumps.
2. **Tagged rings.** Each ring's center shows the account tag in its tag color (`Pe`, `Nv`, `Cx`),
   not the provider letter. Until slice 4 ships, the tag defaults to:
   - `C` / `Cx` / `Cu` / `AG` for the main account of each provider;
   - `C2`, `C3`, … for extra Claude accounts, in `get_claude_accounts` order.

   The ring stroke uses the tag color, and the percentages under it use `sevColor`.
3. **Pill modes A, B and C** keep their layouts (settled). Only tokens, classes and the ring
   helper change.
4. **The pet** callout uses `.card`: name, rarity, level, tokens today, one `tag + 5 h %` pair per
   ring. Click → `open_hub("parceiro")`. Right-click keeps its menu and adds **Abrir NotchMon**
   at the top.
5. **The tray menu** (the macOS menu bar item uses the same menu) stays short on purpose (see the comment in `tray.rs`). It gains two items:
   - **Abrir NotchMon** at the top, which opens the hub on Hoje;
   - **Fazer backup agora**, shown only when slice 5 is connected.

   "Settings…" becomes "Ajustes…" and opens the hub on Ajustes. Refresh and Quit stay. Strings go
   in `i18n.rs` for every language it already has.

6. **On macOS** the pill sits on the screen edge like on Windows (slice 07 decides whether it
   also offers the notch-side layout). The card and the rings are the same page, so this slice
   has no Mac-only work.

## Build order

One commit.

1. `renderCard` → `renderGlance`: one layout, with emphasis by `hoverId`. Delete `renderBrandCard`
   and `renderMonCard` once nothing calls them.
2. Ring center tags and tag colors, falling back to the defaults above.
3. Pet callout and pet menu.
4. Tray items and `i18n.rs`.

## Acceptance

- Moving the pointer from the brand mark down to the last ring never changes the card's height
  by more than the height of the one expanded block.
- Two Claude accounts show two different tags and two different colors.
- `needsAuth`, `stale` and `backoff` still show their notes, now inside the expanded block.

## Left for later

- Clicking a ring to open Contas on that account. Needs slice 4's routes (`#contas/<id>`).
