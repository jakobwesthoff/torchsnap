---
kind: feature
status: open
area: [src/mascot/variants.ts, src/mascot/selection.ts, src/mascot/useMascotVariant.ts, src/launcher/Launcher.tsx]
---

# Pick mascots per character, with occasion shares and a recent list

Decided with the user on 2026-09-30 and reviewed adversarially the same
day; not implemented. The full plan,
self-contained with every decision, its reason and the rejected
alternatives, is `docs/plan/mascot-selection-weighting.md`. This todo is
closed when that plan is implemented.

In short:

- A draw has two fixed layers instead of the recursive tree in
  `src/mascot/selection.ts`: occasions in priority order, each taking its
  percentage of the launches still left (FullMoon 90, NewYear 90,
  Christmas 40, Halloween 40, Easter 40), then the all-year pool.
- Inside a pool every character has the same chance, then one of its
  eligible variants. A character joins SFW/NSFW twins, numbered repeats
  and other forms of one figure: a new `character` field from
  torchsnap-mascot, filled from torchsnap-mascot-base's references with
  the corrections listed in the plan.
- The 20 most recently shown characters are remembered in memory
  (recorded at dismiss, only when shown); a pool excludes fewer than
  half its characters, the most recent ones.
- Occasions, their order and shares move to `src/mascot/occasions.json`;
  the file names a condition registered in `src/mascot/conditions.ts`
  (logic in code, a description for debugging, no parameters in the
  file).
- The draw is pure; the hook keeps draw and recent list in one state,
  corrects an NSFW pick during render, and re-draws while hidden when
  the set of active conditions changes.
- With an empty all-year pool the active occasions share the draw;
  nothing eligible at all falls back to `original`.
- A debug panel below the launcher card, under `tauri dev` only, shows
  the draw.
- ADR 59 amends ADRs 57 and 58.

The group settings that this todo used to cover are split out into
`todos/product/features/01m3sgdr9bj0dtkzhyf584d2bg-mascot-group-settings.md`.
