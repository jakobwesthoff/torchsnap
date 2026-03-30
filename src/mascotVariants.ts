// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Snappy mascot variant registry and weighted selection configuration.
 *
 * This file defines which mascot variants exist and how they are weighted
 * in the random selection system. It is the single place to add new Snappy
 * mascots — add the variant name to the appropriate group below, then wire
 * it into `SnappyHeroSets` with the desired weight and conditions.
 *
 * The selection system supports:
 *   - Flat weights (variant always participates at the given weight)
 *   - Conditional weights (variant gets a boosted weight when a condition
 *     is true, e.g. during Halloween or on a full moon)
 *   - Nested groups (recurse into children with their own weights)
 *
 * See `useRandomMascot.ts` for the full algorithm details.
 */

import type { MascotEntry } from "./hooks/useRandomMascot";
import mascotData from "./mascots.json";

// Imports needed once themed variants are wired in:
// import { getSeason } from "./hooks/useSeason";
// import { getFullMoonDistance } from "./hooks/useFullMoonDistance";
// import { isHalloween, isChristmas, isEaster, isNewYear } from "./hooks/useHolidays";

// =========================================================
// Mascot Metadata
// =========================================================

interface MascotInfo {
  alt: string;
  /** When true, the mascot depicts content some users may find inappropriate
   *  (e.g. a visible weapon). Controlled by the `showNsfwMascots` setting. */
  nsfw?: boolean;
}

// TypeScript infers a precise type from the JSON literal, so we widen it
// to a plain record for safe runtime lookups on unknown variant names.
const mascots = mascotData as Record<string, MascotInfo>;

/**
 * Returns the alt text for a given variant, falling back to the variant
 * name formatted as a human-readable string when no entry exists.
 */
export function getMascotAlt(variant: string): string {
  return mascots[variant]?.alt ?? variant.replace(/-/g, " ");
}

/**
 * Returns `true` when the variant is tagged as NSFW in `mascots.json`.
 * Unlisted variants are treated as safe.
 */
export function isNsfwVariant(variant: string): boolean {
  return mascots[variant]?.nsfw === true;
}

// =========================================================
// Variant Registry
// =========================================================

// All known Snappy mascot variant names, organized by theme.
// Each variant name corresponds to a set of image files in
// `public/images/mascot/snappy-{variant}-{size}.{png,webp}`.
export const Variants = {
  // The original Snappy. Always available, always charming.
  Original: ["original"],

  // --- Add more variants here as they are created ---
  //
  // General: ['snappy-detective', 'snappy-astronaut'],
  //
  // Seasonal:
  //   Spring:  ['snappy-spring'],
  //   Summer:  ['snappy-summer', 'snappy-beach'],
  //   Autumn:  ['snappy-autumn'],
  //   Winter:  ['snappy-winter', 'snappy-snowman'],
  //
  // Holidays:
  //   Halloween: ['snappy-halloween'],
  //   Christmas:  ['snappy-santa'],
  //   Easter:     ['snappy-easter'],
  //   NewYear:    ['snappy-fireworks'],
  //
  // FullMoon: ['snappy-werewolf'],
};

// =========================================================
// Hero Selection Sets
// =========================================================

/**
 * Weighted mascot selection configuration for the launcher hero slot.
 *
 * Currently only the original Snappy is available — it will always be
 * selected. The structure is already prepared for themed variants:
 * add entries below following the commented-out examples, then add the
 * corresponding image assets and register the variant names above.
 *
 * Weight guidelines (for when multiple entries are active):
 *   - Original:   35  (always a familiar face)
 *   - General:    65  (themed mascots compete for the rest)
 *   - Seasonal:    0 → 20 conditional (active only in their season)
 *   - Halloween:   0 → 35 conditional (October)
 *   - Christmas:   0 → 70 conditional (December, stronger presence)
 *   - Easter:      0 → 35 conditional (one week around Easter Sunday)
 *   - New Year:    0 → 1000 conditional (Dec 30 – Jan 2, dominates)
 *   - Full moon:   0 → 1000 conditional (day of / day before full moon)
 */
export const SnappyHeroSets: MascotEntry[] = [
  // Original Snappy — the only variant for now. When more mascots
  // are added, lower this weight (e.g. to 35) so alternatives get
  // their fair share of appearances.
  { variants: [...Variants.Original], weight: 1 },

  // --- Future entries go here ---
  //
  // General themed mascots (activate alongside Original once available):
  // { variants: [...Variants.General], weight: 65 },
  //
  // Seasonal variants — dormant (weight 0) until their season fires:
  // { variants: [...Variants.Seasonal.Spring], weight: 0,
  //   condition: () => getSeason() === 'spring', conditionalWeight: 20 },
  // { variants: [...Variants.Seasonal.Summer], weight: 0,
  //   condition: () => getSeason() === 'summer', conditionalWeight: 20 },
  // { variants: [...Variants.Seasonal.Autumn], weight: 0,
  //   condition: () => getSeason() === 'autumn', conditionalWeight: 20 },
  // { variants: [...Variants.Seasonal.Winter], weight: 0,
  //   condition: () => getSeason() === 'winter', conditionalWeight: 20 },
  //
  // Holiday mascots (very high weight when active to dominate the rotation):
  // { variants: [...Variants.Holidays.Halloween], weight: 0,
  //   condition: isHalloween, conditionalWeight: 35 },
  // { variants: [...Variants.Holidays.Christmas], weight: 0,
  //   condition: isChristmas, conditionalWeight: 70 },
  // { variants: [...Variants.Holidays.Easter], weight: 0,
  //   condition: isEaster, conditionalWeight: 35 },
  // { variants: [...Variants.Holidays.NewYear], weight: 0,
  //   condition: isNewYear, conditionalWeight: 1000 },
  //
  // Full moon — extreme boost when within one day of a full moon:
  // { weight: 0,
  //   condition: () => getFullMoonDistance() <= 1, conditionalWeight: 1000,
  //   children: [{ variants: [...Variants.FullMoon], weight: 1 }] },
];
