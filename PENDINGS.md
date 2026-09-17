# PENDINGS.md

Open items left behind by executed plans. **Every plan writes here.** When a plan
finishes and something is knowingly not done — deferred, blocked, gated on a decision,
or accepted as-is — it lands in this file instead of only in a chat message that
nobody will find again.

## How to use it

- **One section per plan/slice**, newest first. Keep the plan file path so the context
  is recoverable.
- Every entry states **what is pending, why it was not done, and what unblocks it**.
  "Why" is the part that saves the next person from re-deriving the decision.
- Mark each entry with a status:
  - `OPEN` — needs doing.
  - `GATED` — blocked on a decision or an external condition; name the gate.
  - `ACCEPTED` — knowingly left as-is; not a bug, do not re-flag it.
- Delete an entry when it is genuinely resolved. Do not leave a graveyard of DONEs.
- A pending that touches another slice's domain is **recorded here, not fixed
  unilaterally** — flag it and let the owner decide.

### Write them short

This file is read when someone is about to work, not for pleasure. Density beats prose.

- **Target: 3–6 lines per entry.** If it needs more, the entry is really two entries, or it
  belongs in the plan/spec — link there instead of re-explaining.
- **Drop the connective tissue.** No "it is worth noting that", no restating the title in the
  first sentence, no paragraph of background before the point.
- **Three facts, that is the whole shape**: what is pending · why it was not done · what
  unblocks it. Anything that is not one of the three is cut.
- **Keep every identifier, number, path and error string exact.** Terse means fewer words, never
  vaguer facts. `403 on getMarketplaceParticipations` survives; "an auth problem" does not.
- **No credential-rotation chores.** Secret hygiene is handled outside this file — do not add
  "rotate X" items here.

---

## NotchMon companion — `docs/plans/2026-09-16-notchmon-companion-plan.md` (2026-09-16)

- `OPEN` **Pet size change verified by reasoning, not by a run.** Root cause found: the `pet`
  window was missing from `notchmon/capabilities/default.json`, so `event.listen` was refused
  and `pet.html` never received `companion_prefs`; the window resized, the sprite did not.
  Fixed in this branch. Unblock: with the pet visible, Settings → Companion → pet size 48 → 384
  and confirm the sprite grows; also confirm a bubble shows on the next Rare Candy.

- `OPEN` **OS toast notifications.** The in-app bubbles (pill, pet) stand in for hatch,
  evolution, graduation, shiny and candy. Not ported because it needs `tauri-plugin-notification`
  and a permission entry. Unblock: add the plugin, gate it behind `companion.notifications`.

- `OPEN` **Cursor and Antigravity tokens for the companion.** Only Claude Code transcripts and
  Codex rollouts feed `tokens.rs`; the other two rings still show usage but grant no XP.
  Not done: no on-disk per-request token log is known for either. Unblock: locate one.

- `OPEN` **Codex fork/subagent replay dedup.** `parse_codex_line` skips a repeated identical
  `last_token_usage` state, but a forked rollout replays the parent's `token_count` events and
  counts them twice. Unblock: a sample rollout with a fork, then key on the first event's
  turn id.

- `OPEN` **Companion save export/import.** `companion.json` under `%APPDATA%\notchmon` is the
  whole save; there is no UI to back it up or move it. Cut for scope. Unblock: a Settings
  button pair that copies the file out and in (validate with `serde` before replacing).

- `ACCEPTED` **Old "Codenotch" autostart value is not migrated.** `autostart.rs` now writes
  `NotchMon`; an upgrader with start-at-sign-in on keeps a dead `Codenotch` Run value pointing
  at the removed exe and sees the switch off. Re-enable in Settings. The dev machine had none.

- `ACCEPTED` **`tray_sprite_file` builds a `pokeapi::Client` every 2 s** while the sprite tray
  icon is on (two `create_dir_all`). Marked `ponytail:` in `main.rs`; cache the path if it
  ever shows in a profile.

- `ACCEPTED` **`docs/img/pill-states.png` predates the gear button.** It is the states board
  from the design canvas, so the pill in it has no gear at its foot. Regenerate when the canvas
  is re-seeded; every other image under `docs/img` is a fresh render of the shipped pages.
