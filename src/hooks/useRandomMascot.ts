// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Recursive weighted random mascot selection with condition-based boosting.
 *
 * Entries form a tree: a leaf holds `variants` (pick uniformly), a group
 * holds `children` (recurse). The selection algorithm produces a weighted
 * shuffle (Efraimidis–Spirakis) at each level and walks the ordering
 * with fallthrough — if a group's children all resolve to zero weight,
 * the next entry in the shuffle is tried instead.
 *
 * When a `key` is provided, selections are cached at module scope so they
 * remain stable across component remounts within the same SPA session.
 * Omit the key to re-roll on every mount instead.
 */

import { useState } from "react";
import { preloadMascotVariant } from "../lib/preloadMascot";

// =========================================================
// Types
// =========================================================

/** A leaf entry — pick uniformly among `variants`. */
type UnconditionalLeaf = { variants: string[]; weight: number };
type ConditionalLeaf = {
  variants: string[];
  weight: number;
  condition: () => boolean;
  conditionalWeight: number;
};
type MascotLeaf = UnconditionalLeaf | ConditionalLeaf;

/** A group entry — recurse into `children`. */
type UnconditionalGroup = { children: MascotEntry[]; weight: number };
type ConditionalGroup = {
  children: MascotEntry[];
  weight: number;
  condition: () => boolean;
  conditionalWeight: number;
};
type MascotGroup = UnconditionalGroup | ConditionalGroup;

export type MascotEntry = MascotLeaf | MascotGroup;

// =========================================================
// Module-level session cache
// =========================================================

// Keyed by the caller-supplied identifier so independent call sites
// each get their own stable selection.
const sessionCache = new Map<string, string>();

// =========================================================
// Selection logic (pure, no React dependency)
// =========================================================

function effectiveWeight(entry: MascotEntry): number {
  if ("condition" in entry) {
    return entry.condition() ? entry.conditionalWeight : entry.weight;
  }
  return entry.weight;
}

function isGroup(entry: MascotEntry): entry is MascotGroup {
  return "children" in entry;
}

/**
 * Produce a weighted shuffle using the Efraimidis–Spirakis algorithm:
 * for each entry with weight > 0, compute key = random() ^ (1/weight)
 * and sort descending. Higher-weight entries are more likely to appear
 * first, but randomness is preserved.
 */
function weightedShuffle(entries: MascotEntry[]): MascotEntry[] {
  const keyed: { entry: MascotEntry; key: number }[] = [];

  for (const entry of entries) {
    const w = effectiveWeight(entry);
    if (w > 0) {
      keyed.push({ entry, key: Math.random() ** (1 / w) });
    }
  }

  keyed.sort((a, b) => b.key - a.key);
  return keyed.map((k) => k.entry);
}

/**
 * Recursively select a variant from a list of entries.
 *
 * Walks a weighted shuffle of the entries. For leaves, picks a uniform
 * random variant. For groups, recurses into children. If a group yields
 * no result (all children had zero effective weight), the next entry in
 * the shuffle is tried — this is the "fallthrough" behaviour.
 *
 * Returns `null` when the entire level is exhausted without finding a
 * variant (propagates up to the parent group so it can fall through too).
 */
function selectFromEntries(
  entries: MascotEntry[],
  filter?: (variant: string) => boolean,
): string | null {
  const shuffled = weightedShuffle(entries);

  for (const entry of shuffled) {
    if (isGroup(entry)) {
      const result = selectFromEntries(entry.children, filter);
      if (result !== null) {
        return result;
      }
      // Group produced nothing — fall through to the next entry.
    } else {
      // Leaf — apply filter then pick uniformly among eligible variants.
      // If the filter removes all candidates, fall through to the next
      // entry rather than returning an ineligible variant.
      const eligible = filter ? entry.variants.filter(filter) : entry.variants;
      if (eligible.length > 0) {
        return eligible[Math.floor(Math.random() * eligible.length)];
      }
    }
  }

  return null;
}

// =========================================================
// Pure selection function (usable outside React)
// =========================================================

/**
 * Imperatively select a random mascot variant from a recursive weighted
 * entry tree. Equivalent to the hook's logic but usable outside of React
 * — for example, in event handlers that re-roll the mascot on each
 * launcher show.
 *
 * An optional `filter` predicate can exclude specific variants (e.g. NSFW
 * mascots). Filtered-out variants are skipped at the leaf level; if every
 * variant in a leaf is excluded the algorithm falls through to the next
 * entry, so the tree structure and weights are otherwise unchanged.
 *
 * @throws When no entries produce an eligible variant after filtering.
 */
export function selectMascotVariant(
  sets: MascotEntry[],
  filter?: (variant: string) => boolean,
): string {
  const variant = selectFromEntries(sets, filter);
  if (variant === null) {
    throw new Error("selectMascotVariant: no mascot entries have a positive effective weight");
  }
  return variant;
}

// =========================================================
// Hook
// =========================================================

/**
 * Selects a random mascot variant from a recursive weighted entry tree,
 * optionally influenced by runtime conditions.
 *
 * When `key` is provided the result is session-stable: every call site
 * sharing that key shows the same variant until a full page reload.
 * Omit `key` to re-roll on every component mount.
 *
 * When `preloadSizes` is provided alongside a `key`, the selected variant
 * is preloaded at those logical sizes (and their 2x retina counterparts)
 * the first time the key is resolved. This warms the browser cache before
 * the images appear across different views.
 *
 * @param sets         - Root-level mascot entries with weights and optional conditions.
 * @param key          - Optional session-cache key (e.g. `"hero"`).
 * @param preloadSizes - Logical pixel sizes to preload when the variant is first selected.
 * @returns The selected variant name.
 */
export function useRandomMascot(
  sets: MascotEntry[],
  key?: string,
  preloadSizes?: number[],
): string {
  // Always call useState to satisfy the rules of hooks (consistent call
  // order). The initializer only runs on mount, giving us a per-mount
  // roll for the keyless path.
  const [initialRoll] = useState(() => selectMascotVariant(sets));

  if (key !== undefined) {
    const cached = sessionCache.get(key);
    if (cached !== undefined) {
      return cached;
    }

    // No cache entry yet — reuse the per-mount roll instead of
    // computing a second random selection.
    sessionCache.set(key, initialRoll);

    // First resolution for this key — warm the image cache at all requested
    // sizes so the variant is ready before it appears in other views.
    if (preloadSizes !== undefined && preloadSizes.length > 0) {
      preloadMascotVariant(initialRoll, preloadSizes);
    }
  }

  return initialRoll;
}
