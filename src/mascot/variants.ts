// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Snappy mascot data and weighted selection configuration.
 *
 * The mascots come from `src/derived/mascots.json` and their groups from
 * `src/derived/mascot-groups.json`, which the torchsnap-mascot repository
 * writes with its export (`just export <torchsnap checkout>`) together
 * with the images in `public/images/mascot/`. That repository is the
 * place to add or change a mascot or a group; this file only reads what
 * it delivers (ADR 57, 58).
 *
 * The selection is built from each mascot's `group` and `occasions`: the
 * seasonal groups stay dormant until their occasion, every other group
 * joins one flat pool, and a mascot that names an occasion joins that
 * occasion as well. NSFW mascots sit in their groups like the others; the
 * NSFW filter in `useMascotVariant` excludes them at selection time when
 * the user has disabled non-family-friendly mascots.
 *
 * See `selection.ts` for the selection algorithm.
 */

import type { MascotEntry } from "./selection";
import type { MascotAnchor } from "../launcher/placement";
import { getFullMoonDistance } from "./fullMoon";
import { isHalloween, isChristmas, isEaster, isNewYear } from "./holidays";
import { isNighttime, isTwilight } from "./nighttime";
import mascotData from "../derived/mascots.json";
import groupData from "../derived/mascot-groups.json";

// =========================================================
// Mascot Data
// =========================================================

export interface MascotInfo {
  /** The line the mascot info overlay shows and screen readers read. */
  alt: string;
  /** When true, the mascot depicts content some users may find inappropriate
   *  (e.g. a visible weapon). Controlled by the `showNsfwMascots` setting. */
  nsfw: boolean;
  /** Theme group, e.g. `SciFi` or `Halloween`; drives the selection. */
  group: string;
  /** Seasonal groups whose occasion the mascot joins as well, besides
   *  its own group; empty for most mascots. */
  occasions: string[];
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
// Group Data
// =========================================================

export interface MascotGroupInfo {
  /** The group's display name. */
  label: string;
  /** Where Snappy got the ideas for the group's costumes. */
  description: string;
  /** When true, the group's mascots show only on its occasion. */
  seasonal: boolean;
}

// The labels and descriptions are delivered for the group settings of
// the weighting todo; until those exist, only `seasonal` is read here.
const groups = groupData as Record<string, MascotGroupInfo>;

// =========================================================
// Selection
// =========================================================

/**
 * When each seasonal group's occasion is on, and how strongly its entry
 * is boosted then. The data says which groups are seasonal; the timing
 * lives here because it is logic, not data. A test checks this table
 * against the seasonal groups of the data in both directions.
 */
const OCCASIONS: Record<string, { condition: () => boolean; conditionalWeight: number }> = {
  Halloween: { condition: isHalloween, conditionalWeight: 35 },
  Christmas: { condition: isChristmas, conditionalWeight: 70 },
  Easter: { condition: isEaster, conditionalWeight: 35 },
  NewYear: { condition: isNewYear, conditionalWeight: 1000 },
  // Within one day of a full moon, but only after dark so the werewolf
  // doesn't show up at noon.
  FullMoon: {
    condition: () => getFullMoonDistance() <= 1 && (isNighttime() || isTwilight()),
    conditionalWeight: 1000,
  },
};

export const OCCASION_GROUPS = Object.keys(OCCASIONS);

/**
 * Builds the weighted selection entries for the launcher mascot: one flat
 * pool of every group that shows all year, with equal probability per
 * mascot (a group the group data does not know joins it too), then one
 * dormant entry per occasion holding the occasion's own group and every
 * mascot that names the occasion.
 *
 * A seasonal group without a condition in `OCCASIONS` gets no entry, so
 * its own mascots never show; the dev build warns about it.
 */
export function buildMascotSelection(
  data: Record<string, MascotInfo>,
  groupInfo: Record<string, MascotGroupInfo>,
): MascotEntry[] {
  const keys = Object.keys(data).sort();
  const isSeasonal = (group: string) => groupInfo[group]?.seasonal === true;

  for (const group of Object.keys(groupInfo)) {
    if (isSeasonal(group) && !(group in OCCASIONS) && import.meta.env.DEV) {
      console.warn(
        `Mascot group "${group}" is seasonal but has no occasion condition; its mascots never show.`,
      );
    }
  }

  // Occasions in the table's order, so the entries keep a stable order.
  const occasions = OCCASION_GROUPS.filter(isSeasonal);
  return [
    { variants: keys.filter((key) => !isSeasonal(data[key].group)), weight: 100 },
    ...occasions.map((occasion) => ({
      variants: keys.filter(
        (key) => data[key].group === occasion || data[key].occasions.includes(occasion),
      ),
      weight: 0,
      ...OCCASIONS[occasion],
    })),
  ];
}

export const mascotSelection: MascotEntry[] = buildMascotSelection(mascots, groups);
