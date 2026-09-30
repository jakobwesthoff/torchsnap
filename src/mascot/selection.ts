// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Recursive weighted random mascot selection with condition-based boosting.
 *
 * Entries form a tree: a leaf holds `variants` (pick uniformly), a branch
 * holds `children` (recurse). The selection algorithm produces a weighted
 * shuffle (Efraimidis–Spirakis) at each level and walks the ordering
 * with fallthrough: if a branch's children all resolve to zero weight,
 * the next entry in the shuffle is tried instead.
 */

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

/** A branch entry: recurse into `children`. */
type UnconditionalBranch = { children: MascotEntry[]; weight: number };
type ConditionalBranch = {
  children: MascotEntry[];
  weight: number;
  condition: () => boolean;
  conditionalWeight: number;
};
type MascotBranch = UnconditionalBranch | ConditionalBranch;

export type MascotEntry = MascotLeaf | MascotBranch;

// =========================================================
// Selection logic
// =========================================================

function effectiveWeight(entry: MascotEntry): number {
  if ("condition" in entry) {
    return entry.condition() ? entry.conditionalWeight : entry.weight;
  }
  return entry.weight;
}

function isBranch(entry: MascotEntry): entry is MascotBranch {
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
 * random variant. For branches, recurses into children. If a branch yields
 * no result (all children had zero effective weight), the next entry in
 * the shuffle is tried — this is the "fallthrough" behaviour.
 *
 * Returns `null` when the entire level is exhausted without finding a
 * variant (propagates up to the parent branch so it can fall through too).
 */
function selectFromEntries(
  entries: MascotEntry[],
  filter?: (variant: string) => boolean,
): string | null {
  const shuffled = weightedShuffle(entries);

  for (const entry of shuffled) {
    if (isBranch(entry)) {
      const result = selectFromEntries(entry.children, filter);
      if (result !== null) {
        return result;
      }
      // The branch produced nothing, so fall through to the next entry.
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
// Entry point
// =========================================================

/**
 * Selects a random mascot variant from a recursive weighted entry tree.
 * `useMascotVariant` calls it on every roll, including from the window
 * blur handler that re-rolls the mascot after each launcher show.
 *
 * An optional `filter` predicate can exclude specific variants (e.g. NSFW
 * mascots). Filtered-out variants are skipped at the leaf level; if every
 * variant in a leaf is excluded the algorithm falls through to the next
 * entry, so the tree structure and weights are otherwise unchanged.
 *
 * @throws When no entries produce an eligible variant after filtering.
 */
export function selectMascotVariant(
  entries: MascotEntry[],
  filter?: (variant: string) => boolean,
): string {
  const variant = selectFromEntries(entries, filter);
  if (variant === null) {
    throw new Error("selectMascotVariant: no mascot entries have a positive effective weight");
  }
  return variant;
}
