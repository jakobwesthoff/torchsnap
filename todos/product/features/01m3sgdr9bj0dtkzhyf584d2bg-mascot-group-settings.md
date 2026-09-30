---
kind: feature
status: open
area: [src/settings/sections/AppearanceSection.tsx, src/mascot/variants.ts, src/mascot/selection.ts, src-tauri/src/lib.rs]
tags: [ux]
---

# Let users switch mascot groups on and off

Split out of the per-character selection (ADR 59) on 2026-09-30,
because the user left the settings page out of that work. The pools are
built by `buildMascotPools` in `src/mascot/variants.ts`; leaving a
switched-off group's mascots out of its `data` argument is enough for
the draw, and it adds `original` to the all-year pool whatever the data
holds.

## What is wanted

A switch per mascot group in the settings. The 23 groups and their
`label`, `description` and `seasonal` flag arrive in
`src/derived/mascot-groups.json` from torchsnap-mascot (ADR 58); the
labels and descriptions were written for this page.

## Decided

- A switched-off seasonal group never shows, even while its occasion's
  condition holds (user, 2026-09-30). Its occasion takes no part in the
  draw, so its share passes down to the next occasion or the all-year
  pool.
- A mascot joins an occasion only while its own group is switched on
  (ADR 58). Example: with Horror off, Halloween's pool holds only the
  Halloween group's own characters.
- "Show Horror all year" needs no setting of its own; it is the Horror
  group's switch.

## Open

- Page layout: a section of its own ("Snappy", with the four existing
  mascot switches moved out of Appearance) or a list inside Appearance.
- Storage: suggested is one list setting of the switched-off groups
  (for example `disabledMascotGroups`, default `[]` ensured in
  `src-tauri/src/lib.rs`), so a group added by a later export is on by
  default and a removed group's id is ignored. Settings values are JSON
  values (`SettingsInit::ensure`), so a list works.
- The last switch needs no special handling. Decided for the selection
  (user, 2026-09-30): `original`, the plain Snappy, is never hidden and
  always in the all-year pool, whatever the switches say. With every
  group off, the launcher shows `original` outside the occasions. It
  keeps `group: OffDuty` in the data, so the page should say that the
  OffDuty switch does not hide it (or leave it out of that group's
  description).
- Thin pools: switching groups off can shrink an occasion's pool
  (Superheroes off leaves FullMoon with 2 characters). The recent list
  excludes fewer than half of a pool's characters, so a pool of 2 stays
  plain chance: each full-moon launch repeats the previous character
  half the time.
- Maybe later: a per-group factor (less, normal, more) that multiplies
  the group's character count in the all-year pool. Not percentages per
  group, since those would need rebalancing on every switch. The
  selection does not prepare for it; the factor would be added with that
  setting.
