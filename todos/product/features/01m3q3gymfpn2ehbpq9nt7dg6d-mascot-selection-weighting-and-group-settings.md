---
kind: investigation
status: open
area: [src/mascot/variants.ts, src/mascot/selection.ts, src/mascot/useMascotVariant.ts, src/settings/sections/AppearanceSection.tsx]
---

# Revisit mascot selection weighting and group settings

The mascot set grew from 189 to 663 variants (now maintained in the
`torchsnap-mascot` repository, which exports the webp files, the mascot
data and the group data). The selection rules were written for the small
set. Check whether they still give a good mix, and decide on the ideas
below.

Order agreed on 2026-09-29: first the variant lists move out of
`src/mascotVariants.ts` (now `src/mascot/variants.ts`) into the data
(done: `buildMascotSelection` builds the sets from each mascot's
`group`), then the selection code gets a cleanup pass (done 2026-09-30),
then this. The regrouping of 2026-09-30 (ADR 58) came in between.

## Selection as of 2026-09-30

- One flat leaf with weight 100 holds every group that shows all year.
  Inside it every variant has the same chance.
- Seasonal leaves have weight 0 until their condition holds: Halloween
  35 (`isHalloween`), Christmas 70, Easter 35, NewYear 1000, FullMoon
  1000 (within one day of full moon, only at night or twilight). Each
  holds its seasonal group plus every mascot that names the occasion in
  `occasions` (all Horror mascots name Halloween).
- Settings: `mascotMode` (center, sidekick, off), `randomMascots`,
  `showNsfwMascots`. With NSFW off, an NSFW pick is replaced by a
  separate draw from the SFW pool.

## Group sizes in the set

Horror 57, SaturdayMorning 53, SciFi 52, Fighters 49, Superheroes 48,
Fantasy 42, TimeTravel 40, AnimatedComedy 35, Gaming 35, Detectives 33,
Movies 32, Cartoons 28, SpaceOpera 28, KidsTV 22, Adventure 21, Anime 19,
DevCulture 19, OffDuty 7; seasonal: Christmas 16, Halloween 14, Easter 5,
NewYear 5, FullMoon 3. Occasion tags: Halloween 71, Easter 6, Christmas
3, FullMoon 2.

## Biases to judge

- A group's share follows its size, since every variant weighs the
  same: some Horror mascot comes up 8 times as often as some OffDuty one.
- 91 characters have an SFW and an NSFW version. With NSFW on, each of
  them is twice as likely as a character with one version.
- 47 variants are numbered repeats of a character (`-2`, `-3`, ...);
  `proton-pack-buster` has 8 variants, `hellfire-specter` 6. Same effect.
- A mascot with `occasions` sits in the regular pool and in its
  occasions' leaves, so during an occasion it comes up more often than
  the rest of its group.
- `original` is 1 of 620 variants in the regular pool.

## Ideas, none decided

- Weight per group instead of per variant, or per character: pick an
  SFW/NSFW pair or a numbered series first, then one of its variants.
  `partner` in `torchsnap-mascot/mascots.json` links the pairs; the
  export would have to send it (it sends alt, nsfw, group, occasions,
  groundAnchor, boxLeft). Numbered series are only visible in the key.
- User settings for costumes: enable or disable groups; one on/off
  switch per seasonal group. `src/derived/mascot-groups.json` already
  carries each group's `label` and `description` for this (rules in
  torchsnap-mascot's `docs/alt-text-rules.md`). Decided with ADR 58: a
  mascot joins an occasion only while its own group is enabled.
  "Show Horror all year" is the same as enabling the Horror group, so it
  needs no setting of its own.
- Any group filter must leave at least one SFW variant, since
  `selectMascotVariant` throws on an empty pool. `original` is the
  natural last fallback.
