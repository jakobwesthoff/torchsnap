// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { afterEach, describe, expect, it, vi } from "vitest";
import { selectMascotVariant, type MascotEntry } from "./selection";

afterEach(() => {
  vi.restoreAllMocks();
});

/** Feeds `Math.random` the given values in order. */
function randomSequence(...values: number[]) {
  const spy = vi.spyOn(Math, "random");
  for (const value of values) spy.mockReturnValueOnce(value);
  return spy;
}

/** Share of `draws` selections that return `variant`. */
function shareOf(variant: string, entries: MascotEntry[], draws = 20_000): number {
  let hits = 0;
  for (let i = 0; i < draws; i++) {
    if (selectMascotVariant(entries) === variant) hits++;
  }
  return hits / draws;
}

// =========================================================
// selectMascotVariant
// =========================================================

describe("selectMascotVariant", () => {
  it("picks among the variants of a leaf uniformly", () => {
    const entries = [{ variants: ["a", "b", "c", "d"], weight: 1 }];
    // One draw orders the single leaf, the next picks the variant.
    randomSequence(0.5, 0);
    expect(selectMascotVariant(entries)).toBe("a");
    randomSequence(0.5, 0.99);
    expect(selectMascotVariant(entries)).toBe("d");
  });

  it("never picks from an entry with weight 0", () => {
    const entries = [
      { variants: ["dormant"], weight: 0 },
      { variants: ["active"], weight: 1 },
    ];
    for (let i = 0; i < 100; i++) expect(selectMascotVariant(entries)).toBe("active");
  });

  it("chooses entries in proportion to their weights", () => {
    const entries = [
      { variants: ["heavy"], weight: 3 },
      { variants: ["light"], weight: 1 },
    ];
    expect(shareOf("heavy", entries)).toBeCloseTo(0.75, 1);
  });

  it("uses the conditional weight while the condition holds", () => {
    let holds = false;
    const entries = [
      { variants: ["regular"], weight: 100 },
      { variants: ["seasonal"], weight: 0, condition: () => holds, conditionalWeight: 300 },
    ];
    expect(shareOf("seasonal", entries, 1000)).toBe(0);
    holds = true;
    expect(shareOf("seasonal", entries)).toBeCloseTo(0.75, 1);
  });

  it("falls through to the next entry when the filter empties a leaf", () => {
    const entries = [
      { variants: ["nsfw-a", "nsfw-b"], weight: 1_000_000 },
      { variants: ["safe"], weight: 1 },
    ];
    const safe = (variant: string) => !variant.startsWith("nsfw");
    for (let i = 0; i < 100; i++) expect(selectMascotVariant(entries, safe)).toBe("safe");
  });

  it("only returns variants the filter accepts from a mixed leaf", () => {
    const entries = [{ variants: ["nsfw-a", "safe-a", "safe-b"], weight: 1 }];
    const safe = (variant: string) => !variant.startsWith("nsfw");
    for (let i = 0; i < 100; i++) expect(selectMascotVariant(entries, safe)).not.toBe("nsfw-a");
  });

  it("recurses into branches", () => {
    const entries: MascotEntry[] = [
      {
        children: [
          { variants: ["inner"], weight: 1 },
          { variants: ["never"], weight: 0 },
        ],
        weight: 1,
      },
    ];
    expect(selectMascotVariant(entries)).toBe("inner");
  });

  it("falls through past a branch whose children all have weight 0", () => {
    const entries: MascotEntry[] = [
      { children: [{ variants: ["dormant"], weight: 0 }], weight: 1_000_000 },
      { variants: ["fallback"], weight: 1 },
    ];
    for (let i = 0; i < 100; i++) expect(selectMascotVariant(entries)).toBe("fallback");
  });

  it("throws when no entry yields a variant", () => {
    expect(() => selectMascotVariant([{ variants: ["a"], weight: 0 }])).toThrow();
    expect(() => selectMascotVariant([{ variants: ["nsfw"], weight: 1 }], () => false)).toThrow();
  });
});
