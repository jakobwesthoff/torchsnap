# 59. Pick the mascot by character in two layers

Date: 2026-09-30

## Status

Accepted

Amends [57. Deliver the mascot set from the torchsnap-mascot repository](0057-deliver-the-mascot-set-from-the-torchsnap-mascot-repository.md)

Amends [58. Describe mascot groups and occasions in the mascot data](0058-describe-mascot-groups-and-occasions-in-the-mascot-data.md)

## Context

The launcher shows one of 663 mascot variants per launch. The selection
rules dated from a set of 189:

- Every variant weighed the same. A character with an SFW and an NSFW
  variant showed twice as often as one with a single variant (91 such
  pairs), and the proton-pack costume, 9 variants with the related one,
  nine times as often.
- The occasions boosted their pools with relative weights (Halloween
  35, Christmas 70, Easter 35, NewYear 1000, FullMoon 1000 against 100
  for the pool that shows all year). What such a weight meant depended
  on what else was active.
- `src/mascot/selection.ts` was a recursive tree with branches,
  fallthrough and a weighted shuffle, but the data only ever built one
  flat all-year leaf and one leaf per occasion.

## Decision

**Characters.** torchsnap-mascot's `mascots.json` gives every mascot a
required `character`: the costume a viewer recognises, with all its
variants. Its value is the key of one of the character's variants, and
the export sends it with the other per-mascot fields.

**Two layers.** A draw walks the occasions whose condition holds and
whose pool has an eligible character, in the order of
`src/mascot/occasions.json`. Each is picked with its share of the
launches still left: the effective share is the share times (1 - share)
of every occasion above it that takes part. When no occasion is picked,
the all-year pool is used. Inside the chosen pool a character is picked
uniformly, then one of its eligible variants in that pool. There are no
per-group weights. A variant is eligible unless it is NSFW while NSFW
mascots are off.

**Occasions file and conditions.** `src/mascot/occasions.json` lists
per occasion its seasonal group, a condition name and a whole-number
share from 1 to 100; the list's order is the priority. The file only
names conditions. `src/mascot/conditions.ts` is the registry: per name
a short description and an `isActive(now)` function. A new kind of
timing is new code. The order and shares are:

| Order | Occasion | Share |
|---|---|---|
| 1 | FullMoon | 90 |
| 2 | NewYear | 90 |
| 3 | Christmas | 40 |
| 4 | Halloween | 40 |
| 5 | Easter | 40 |

A full-moon night at Christmas gives FullMoon 90%, Christmas 4% and the
all-year pool 6%.

**Recently shown.** The launcher keeps the 20 most recently shown
characters in memory; a restart clears it. Only a character that was on
screen enters the list. In a pool with `k` eligible characters, the draw
leaves out the first `min(floor((k - 1) / 2), found)` of the pool's
characters in that list, most recent first.

**The plain Snappy.** `original` is never hidden: it is SFW, the pool
builder always puts it into the all-year pool whatever its group, and
the launcher shows it while random mascots are off. The all-year pool is
therefore never empty.

**When the draw happens.** The next mascot is drawn when the launcher is
dismissed, as before. If the launcher stays hidden across an occasion's
edge, the first launch after it shows a draw from the other side.
Re-checking while the launcher is hidden is left to the todo
`todos/product/features/01m3sj9erdbc3xqpmvpa946v7t-mascot-occasion-recheck-while-hidden.md`.

**NSFW turned off** while an NSFW variant is drawn: the launcher
switches to an SFW variant of the same character in the draw's pool, or
draws again when the character has none there. Turning NSFW back on does
not bring the NSFW variant back.

**Debug panel.** In `tauri dev` builds, the double-click on the mascot
also opens a panel below the card with the draw: pool, character and
variant, the conditions that held with their effective shares and
descriptions, and the recent list. Release builds leave it out.

Rejected:

- A share when alone, turned into a relative weight in code: 99%
  becomes a weight of 9900, 100% cannot be written, and overlaps are
  hard to read. This is what the old weights did.
- The all-year pool gets the rest, scaled down when the active shares
  exceed 100%: on an overlap the all-year pool drops to 0%, and a 99%
  FullMoon ends up at about 66%.
- For thin pools: treating a pool whose characters were all shown
  recently as empty (its share would pass down, so the 90% collapses);
  ignoring the recent list once a pool is exhausted (a 3-character pool
  repeats every third time); always taking the character shown longest
  ago (a fixed, predictable rotation).
- `floor(k / 2)` instead of `floor((k - 1) / 2)`: a 2-character pool
  would alternate strictly.
- A shuffle bag (every character once before a repeat): it needs state
  that survives restarts and changes with every setting and occasion,
  and it would drain the occasion pools.
- A parameterised occasions file with dates and expressions: a script
  language inside JSON.
- A re-check of the occasions when the launcher is shown: it would swap
  the mascot visibly, and on macOS `launcher-shown` is not emitted.

## Consequences

A character shows equally often whatever its number of variants, and
SFW/NSFW twins no longer double its chance. A group's share of the
all-year pool follows its number of characters.

On a full-moon night the launcher mostly shows the three FullMoon
characters.

Changing a share or the order of the occasions is an edit of
`occasions.json` and needs no export. A new seasonal group in
torchsnap-mascot needs an entry there, and a new kind of timing needs a
condition in `conditions.ts`. Tests fail when a seasonal group has no
occasion, an occasion names an unregistered condition or a group that is
not seasonal, or a registered condition is unused.

A new mascot needs its `character`: the id of the character it belongs
to, or its own key for a new character.
