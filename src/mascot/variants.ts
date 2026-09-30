// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Snappy mascot data and weighted selection configuration.
 *
 * The mascots come from `src/derived/mascots.json`, which the
 * torchsnap-mascot repository writes with its export (`just export
 * <torchsnap checkout>`) together with the images in
 * `public/images/mascot/`. That repository is the place to add or change
 * a mascot; this file only reads what it delivers.
 *
 * The selection sets are built from each mascot's `group`: the seasonal
 * groups stay dormant until their occasion, every other group joins one
 * flat pool. NSFW mascots sit in their groups like the others; the NSFW
 * filter in `useMascotVariant` excludes them at selection time when the
 * user has disabled non-family-friendly mascots.
 *
 * See `selection.ts` for the selection algorithm.
 */

import type { MascotEntry } from "./selection";
import type { MascotAnchor } from "../launcher/placement";
import { getFullMoonDistance } from "./fullMoon";
import { isHalloween, isChristmas, isEaster, isNewYear } from "./holidays";
import { isNighttime, isTwilight } from "./nighttime";
import mascotData from "../derived/mascots.json";

// =========================================================
// Mascot Data
// =========================================================

export interface MascotInfo {
  /** The line the mascot info overlay shows and screen readers read. */
  alt: string;
  /** When true, the mascot depicts content some users may find inappropriate
   *  (e.g. a visible weapon). Controlled by the `showNsfwMascots` setting. */
  nsfw: boolean;
  /** Theme group, e.g. `SciFi` or `Halloween`; drives the selection sets. */
  group: string;
  /** The point placed on the card, as fractions of the image (see
   *  `launcher/placement.ts`). */
  groundAnchor: MascotAnchor;
  /** Left edge of the visible figure, as a fraction of the image; keeps
   *  the mirrored sidekick inside the card. */
  boxLeft: number;
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
 * Returns `true` when the variant is tagged as NSFW in the mascot data.
 * Unlisted variants are treated as safe.
 */
export function isNsfwVariant(variant: string): boolean {
  return mascots[variant]?.nsfw === true;
}

/** The image's bottom centre on the feet line: a variant without data
 *  still stands on the card, if not exactly where its feet are. */
const FALLBACK_ANCHOR = { groundAnchor: { x: 0.5, y: 1 }, boxLeft: 0 };

/** Variants already reported as missing, so the warning fires once per
 *  variant instead of on every render. */
const warnedMissingAnchor = new Set<string>();

/**
 * Returns what the launcher needs to place a variant: its ground anchor
 * and the left edge of its visible figure.
 *
 * Selection only offers variants from the data, so the fallback is
 * reached only by a variant name that did not come from it.
 */
export function getMascotAnchor(variant: string): { groundAnchor: MascotAnchor; boxLeft: number } {
  const mascot = mascots[variant];
  if (mascot) return { groundAnchor: mascot.groundAnchor, boxLeft: mascot.boxLeft };

  if (import.meta.env.DEV && !warnedMissingAnchor.has(variant)) {
    warnedMissingAnchor.add(variant);
    console.warn(
      `Mascot "${variant}" is not in src/derived/mascots.json; placing it by its image's bottom centre.`,
    );
  }
  return FALLBACK_ANCHOR;
}

// =========================================================
// Hero Selection Sets
// =========================================================

/**
 * The groups that only show on their occasion. Each gets a dormant entry
 * (weight 0) that injects itself with a high weight while its condition
 * holds, so the user consistently sees seasonal mascots when the occasion
 * calls for it.
 */
const SEASONS: { group: string; condition: () => boolean; conditionalWeight: number }[] = [
  { group: "Halloween", condition: isHalloween, conditionalWeight: 35 },
  { group: "Christmas", condition: isChristmas, conditionalWeight: 70 },
  { group: "Easter", condition: isEaster, conditionalWeight: 35 },
  { group: "NewYear", condition: isNewYear, conditionalWeight: 1000 },
  // Within one day of a full moon, but only after dark so the werewolf
  // doesn't show up at noon.
  {
    group: "FullMoon",
    condition: () => getFullMoonDistance() <= 1 && (isNighttime() || isTwilight()),
    conditionalWeight: 1000,
  },
];

export const SEASONAL_GROUPS = SEASONS.map((season) => season.group);

/**
 * Horror is a regular group that joins the Halloween boost as well, so
 * the season also brings the year-round horror mascots forward.
 */
const EXTRA_GROUPS_IN_SEASON: Record<string, string[]> = { Halloween: ["Horror"] };

/**
 * Builds the weighted selection entries for the launcher hero slot from
 * the mascot data: one flat pool of every non-seasonal group with equal
 * probability per mascot (a group the code does not know yet joins it
 * too), then one dormant entry per season.
 */
export function buildHeroSets(data: Record<string, MascotInfo>): MascotEntry[] {
  const keys = Object.keys(data).sort();
  const inGroups = (groups: string[]) => keys.filter((key) => groups.includes(data[key].group));

  return [
    { variants: keys.filter((key) => !SEASONAL_GROUPS.includes(data[key].group)), weight: 100 },
    ...SEASONS.map(({ group, condition, conditionalWeight }) => ({
      variants: inGroups([group, ...(EXTRA_GROUPS_IN_SEASON[group] ?? [])]),
      weight: 0,
      condition,
      conditionalWeight,
    })),
  ];
}

export const SnappyHeroSets: MascotEntry[] = buildHeroSets(mascots);
