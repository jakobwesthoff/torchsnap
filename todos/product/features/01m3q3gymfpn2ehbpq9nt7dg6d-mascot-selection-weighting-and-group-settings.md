---
kind: investigation
status: open
area: [src/mascot/variants.ts, src/mascot/selection.ts, src/mascot/useMascotVariant.ts, src/settings/sections/AppearanceSection.tsx]
---

# Revisit mascot selection weighting and group settings

The mascot set grew from 189 to 662 variants (now maintained in the
`torchsnap-mascot` repository, which exports the webp files and the
mascot data). The selection rules were written for the small set. Check
whether they still give a good mix, and decide on the ideas below.

Order agreed on 2026-09-29: first the variant lists move out of
`src/mascotVariants.ts` (now `src/mascot/variants.ts`) into the data (done: `buildHeroSets` builds the sets
from each mascot's `group`), then the selection code gets a cleanup
pass, then this.

## Selection as of 2026-09-29

- One flat leaf with weight 100 holds every non-seasonal group. Inside
  it every variant has the same chance.
- Seasonal leaves have weight 0 until their condition holds: Halloween
  plus Horror 35 (`isHalloween`), Christmas 70, Easter 35, NewYear 1000,
  FullMoon 1000 (within one day of full moon, only at night or
  twilight).
- Settings: `mascotMode` (center, sidekick, off), `randomMascots`,
  `showNsfwMascots`. With NSFW off, an NSFW pick is replaced by a
  separate draw from the SFW pool.

## Group sizes in the new set

SciFi 126, PopCulture 85, Superheroes 67, Fantasy 53, RealmDefenders 52,
Halloween 49, Detectives 39, Horror 37, DevCulture 31, TimeWanderers 28,
Explorers 20, Christmas 17, Easter 12, Anime 10, TorchBearers 7,
FullMoon 7, CenturyHoppers 5, NewYear 5, Gargoyles 5, Steampunk 4,
Kids 2, Original 1. Kids, Anime and DevCulture are new groups.

## Biases to judge

- A group's share follows its size, since every variant weighs the
  same: some SciFi mascot comes up 63 times as often as some Kids one.
- 91 characters have an SFW and an NSFW version. With NSFW on, each of
  them is twice as likely as a character with one version.
- 47 variants are numbered repeats of a character (`-2`, `-3`, ...);
  `proton-pack-buster` has 8 variants, `hellfire-specter` 6. Same effect.
- `original` is 1 of about 560 variants in the regular pool.

## Ideas, none decided

- Weight per group instead of per variant, or per character: pick an
  SFW/NSFW pair or a numbered series first, then one of its variants.
  `partner` in `torchsnap-mascot/mascots.json` links the pairs; the
  export would have to send it (it sends only alt, nsfw, group,
  groundAnchor, boxLeft). Numbered series are only visible in the key.
- Split the Halloween group: `Halloween` for mascots that are explicitly
  Halloween themed (pumpkins, candy, witches with lanterns), `Horror`
  for horror characters in general. Today the data's Halloween group
  (49) also holds plain horror characters, for example
  `shower-curtain-motel-scream`, `long-wet-hair-tv-well-girl` and
  `bolted-neck-monster`. Needs a pass over all 86 Halloween and Horror
  mascots; the group is set in `torchsnap-mascot/mascots.json`.
- User settings for costumes: enable or disable groups; one on/off
  switch per seasonal group. "Show Horror all year" is the same as
  enabling the Horror group outside the Halloween window, so it needs no
  setting of its own.
- Any group filter must leave at least one SFW variant, since
  `selectMascotVariant` throws on an empty pool. `original` is the
  natural last fallback.
