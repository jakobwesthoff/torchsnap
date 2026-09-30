// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Snappy mascot data and the pools the draw picks from.
 *
 * The mascots come from `src/derived/mascots.json` and their groups from
 * `src/derived/mascot-groups.json`, which the torchsnap-mascot repository
 * writes with its export (`just export <torchsnap checkout>`) together
 * with the images in `public/images/mascot/`. That repository is the
 * place to add or change a mascot or a group; this file only reads what
 * it delivers (ADR 57, 58).
 *
 * The pools are built from each mascot's `group` and `occasions` and the
 * occasions of `occasions.json` (ADR 59): every group that is not
 * seasonal joins the all-year pool, and each occasion's pool holds its
 * seasonal group plus every mascot that names the occasion. NSFW mascots
 * sit in the pools like the others; the draw leaves them out while the
 * user has NSFW mascots turned off.
 *
 * See `selection.ts` for the draw.
 */

import type { MascotFacts, MascotPools } from "./selection";
import type { MascotAnchor } from "../launcher/placement";
import mascotData from "../derived/mascots.json";
import groupData from "../derived/mascot-groups.json";
import occasionData from "./occasions.json";

// =========================================================
// Mascot Data
// =========================================================

export interface MascotInfo {
  /** The line the mascot info overlay shows and screen readers read. */
  alt: string;
  /** When true, the mascot depicts content some users may find inappropriate
   *  (e.g. a visible weapon). Controlled by the `showNsfwMascots` setting. */
  nsfw: boolean;
  /** The character the mascot belongs to: the key of one of its variants.
   *  The draw picks a character first, then one of its variants. */
  character: string;
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

// Only `seasonal` is read here.
// TODO: show the labels and descriptions in the group settings
// (todos/product/features/01m3sgdr9bj0dtkzhyf584d2bg-mascot-group-settings.md).
const groups = groupData as Record<string, MascotGroupInfo>;

// =========================================================
// Pools
// =========================================================

/** The plain Snappy without a costume. It is never hidden: the launcher
 *  shows it while random mascots are off, and it is always in the
 *  all-year pool, so that pool is never empty. */
export const ORIGINAL_MASCOT = "original";

/** An entry of `occasions.json`; the file's order is the priority. */
export interface Occasion {
  /** The seasonal group whose mascots show on the occasion. */
  group: string;
  /** A name from the registry in `conditions.ts`. */
  condition: string;
  /** Whole percent, 1 to 100, of the launches still left. */
  share: number;
}

export const OCCASIONS: Occasion[] = occasionData.occasions;

/**
 * Builds the pools the draw picks from. A variant whose group
 * `groupInfo` does not know joins the all-year pool. `original` is added
 * to the all-year pool even when `data` lacks it, so leaving a group out
 * of `data` can never hide it.
 */
export function buildMascotPools(
  data: Record<string, MascotInfo>,
  groupInfo: Record<string, MascotGroupInfo>,
  occasions: Occasion[],
): MascotPools {
  const keys = Object.keys(data).sort();
  const isSeasonal = (group: string) => groupInfo[group]?.seasonal === true;
  const allYear = keys.filter((key) => !isSeasonal(data[key].group) && key !== ORIGINAL_MASCOT);
  return {
    allYear: [...allYear, ORIGINAL_MASCOT].sort(),
    occasions: occasions.map((occasion) => ({
      ...occasion,
      variants: keys.filter(
        (key) => data[key].group === occasion.group || data[key].occasions.includes(occasion.group),
      ),
    })),
  };
}

/** The pools of the shipped mascots. */
export const mascotPools: MascotPools = buildMascotPools(mascots, groups, OCCASIONS);

/** The draw's facts: every entry of the data carries `character` and
 *  `nsfw`. */
export const mascotFacts: Record<string, MascotFacts> = mascots;
