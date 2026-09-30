// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * The mascot draw: which costume the launcher shows next (ADR 59).
 *
 * A draw is a pure function of its input. It runs in two layers:
 *
 * 1. The occasions whose condition holds, in the priority order of
 *    `occasions.json`. Each takes its share of the launches still left:
 *    40% for Christmas alone, and on a full-moon night at Christmas
 *    FullMoon 90%, Christmas 40% of the remaining 10%.
 * 2. When no occasion was picked, the all-year pool.
 *
 * Inside the chosen pool it picks a character uniformly, leaving out the
 * most recently shown ones, then one of that character's variants. A
 * character with nine variants therefore shows as often as one with a
 * single variant, and SFW/NSFW twins never double a character's chance.
 */

import type { Condition } from "./conditions";

// =========================================================
// Types
// =========================================================

/** What the draw needs to know about a variant. */
export interface MascotFacts {
  character: string;
  nsfw: boolean;
}

/** The variants of one occasion, with the entry of `occasions.json`. */
export interface OccasionPool {
  group: string;
  condition: string;
  /** Whole percent of the launches still left while the condition holds. */
  share: number;
  variants: string[];
}

export interface MascotPools {
  allYear: string[];
  /** In priority order. */
  occasions: OccasionPool[];
}

export interface DrawInput {
  pools: MascotPools;
  mascots: Record<string, MascotFacts>;
  conditions: Record<string, Condition>;
  now: Date;
  allowNsfw: boolean;
  /** Recently shown characters, most recent first. */
  recent: readonly string[];
  /** Returns a number in [0, 1), like `Math.random`. */
  random: () => number;
}

/** One pool's effective share of a draw, as a fraction. */
export interface PoolShare {
  pool: string;
  share: number;
}

/** The result of a draw and what led to it, for the debug panel. */
export interface MascotDraw {
  variant: string;
  character: string;
  /** An occasion's group, or `ALL_YEAR`. */
  pool: string;
  /** Names of the conditions that held, in the occasions' order; an
   *  occasion whose pool had no eligible character is listed too. */
  conditionsHeld: string[];
  /** Every pool that took part, in walking order, the all-year pool last. */
  shares: PoolShare[];
  /** Recently shown characters left out of the chosen pool. */
  excluded: number;
  /** Length of the recent list the draw saw. */
  remembered: number;
  now: Date;
}

/** The name the all-year pool goes by in a draw. */
export const ALL_YEAR = "all-year";

/** How many recently shown characters are remembered. */
export const RECENT_LIMIT = 20;

// =========================================================
// Pool contents
// =========================================================

/** A variant outside the data counts as its own SFW character, so a
 *  pool holding one (`original` added by the pool builder) still draws. */
function factsOf(mascots: Record<string, MascotFacts>, variant: string): MascotFacts {
  return mascots[variant] ?? { character: variant, nsfw: false };
}

/** The eligible variants of a pool grouped by character, sorted by
 *  character and variant so a random number maps to one result. */
function charactersIn(
  variants: readonly string[],
  mascots: Record<string, MascotFacts>,
  allowNsfw: boolean,
): Map<string, string[]> {
  const byCharacter = new Map<string, string[]>();
  for (const variant of [...variants].sort()) {
    const facts = factsOf(mascots, variant);
    if (facts.nsfw && !allowNsfw) continue;
    byCharacter.set(facts.character, [...(byCharacter.get(facts.character) ?? []), variant]);
  }
  return new Map(
    [...byCharacter.keys()].sort().map((character) => [character, byCharacter.get(character)!]),
  );
}

function pick<T>(items: readonly T[], random: () => number): T {
  return items[Math.floor(random() * items.length)];
}

// =========================================================
// The draw
// =========================================================

export function drawMascot(input: DrawInput): MascotDraw {
  const { pools, mascots, conditions, now, allowNsfw, recent, random } = input;

  // Layer 1: an occasion takes part when its condition holds and its pool
  // has an eligible character. A share it cannot use passes down.
  const conditionsHeld: string[] = [];
  const takingPart: { pool: OccasionPool; characters: Map<string, string[]> }[] = [];
  for (const pool of pools.occasions) {
    if (!conditions[pool.condition]?.isActive(now)) continue;
    conditionsHeld.push(pool.condition);
    const characters = charactersIn(pool.variants, mascots, allowNsfw);
    if (characters.size > 0) takingPart.push({ pool, characters });
  }

  const shares: PoolShare[] = [];
  let left = 1;
  for (const { pool } of takingPart) {
    shares.push({ pool: pool.group, share: (pool.share / 100) * left });
    left *= 1 - pool.share / 100;
  }
  shares.push({ pool: ALL_YEAR, share: left });

  let chosen: { name: string; characters: Map<string, string[]> } | undefined;
  for (const { pool, characters } of takingPart) {
    if (random() < pool.share / 100) {
      chosen = { name: pool.group, characters };
      break;
    }
  }
  // Layer 2. The pool builder always puts `original` into the all-year
  // pool, so it has an eligible character whatever the filters.
  chosen ??= { name: ALL_YEAR, characters: charactersIn(pools.allYear, mascots, allowNsfw) };

  // Leave out fewer than half of the pool's characters, the most recently
  // shown ones: a pool of 3 drops the last one shown, a pool of 1 or 2
  // drops none and stays random instead of alternating.
  const eligible = [...chosen.characters.keys()];
  const cap = Math.floor((eligible.length - 1) / 2);
  const recentInPool = recent.filter((character) => chosen.characters.has(character));
  const excluded = new Set(recentInPool.slice(0, Math.max(0, cap)));
  const character = pick(
    eligible.filter((candidate) => !excluded.has(candidate)),
    random,
  );
  const variant = pick(chosen.characters.get(character)!, random);

  return {
    variant,
    character,
    pool: chosen.name,
    conditionsHeld,
    shares,
    excluded: excluded.size,
    remembered: recent.length,
    now,
  };
}

// =========================================================
// Around the draw
// =========================================================

/** The recent list after `character` was on screen: moved to the front,
 *  without a second entry, cut to `RECENT_LIMIT`. */
export function rememberShown(recent: readonly string[], character: string): string[] {
  return [character, ...recent.filter((entry) => entry !== character)].slice(0, RECENT_LIMIT);
}
