// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { describe, expect, it } from "vitest";
import type { Condition } from "./conditions";
import {
  ALL_YEAR,
  RECENT_LIMIT,
  drawMascot,
  rememberShown,
  safeVariantInPool,
  type DrawInput,
  type MascotDraw,
  type MascotFacts,
  type MascotPools,
} from "./selection";

// =========================================================
// Fixtures
// =========================================================

/** A seeded generator (mulberry32), so the share tests are repeatable. */
function seeded(seed: number): () => number {
  let state = seed >>> 0;
  return () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = state;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4_294_967_296;
  };
}

/** A random source that returns the given values in order. */
function sequence(...values: number[]): () => number {
  let index = 0;
  return () => {
    if (index >= values.length)
      throw new Error("the draw asked for more random numbers than given");
    return values[index++];
  };
}

/** Conditions with fixed answers, keyed like the registry. */
function conditions(held: string[]): Record<string, Condition> {
  const names = ["fullMoonNight", "newYear", "christmas", "halloween", "easter"];
  return Object.fromEntries(
    names.map((name) => [
      name,
      { description: `${name} window`, isActive: () => held.includes(name) },
    ]),
  );
}

/** Facts for variants named `<character>` or `<character>-<suffix>`;
 *  a key ending in `-nsfw` is NSFW. */
function facts(...keys: string[]): Record<string, MascotFacts> {
  return Object.fromEntries(
    keys.map((key) => [key, { character: key.split("-")[0], nsfw: key.endsWith("-nsfw") }]),
  );
}

const NOW = new Date(2026, 11, 24, 22, 14);

const mascots = facts(
  "original",
  "robot",
  "robot-nsfw",
  "ghost",
  "wolf",
  "wolf-nsfw",
  "santa",
  "party",
  "bunny",
  "pumpkin",
);

const pools: MascotPools = {
  allYear: ["ghost", "original", "robot", "robot-nsfw"],
  occasions: [
    { group: "FullMoon", condition: "fullMoonNight", share: 90, variants: ["wolf", "wolf-nsfw"] },
    { group: "NewYear", condition: "newYear", share: 90, variants: ["party"] },
    { group: "Christmas", condition: "christmas", share: 40, variants: ["santa"] },
    { group: "Halloween", condition: "halloween", share: 40, variants: ["pumpkin"] },
    { group: "Easter", condition: "easter", share: 40, variants: ["bunny"] },
  ],
};

function input(overrides: Partial<DrawInput> = {}): DrawInput {
  return {
    pools,
    mascots,
    conditions: conditions([]),
    now: NOW,
    allowNsfw: true,
    recent: [],
    random: seeded(1),
    ...overrides,
  };
}

/** How often each value of `pick` comes up in `draws` draws, as fractions. */
function frequencies(
  draws: number,
  base: DrawInput,
  pick: (draw: MascotDraw) => string,
): Record<string, number> {
  const counts: Record<string, number> = {};
  for (let i = 0; i < draws; i++) {
    const key = pick(drawMascot(base));
    counts[key] = (counts[key] ?? 0) + 1;
  }
  return Object.fromEntries(Object.entries(counts).map(([key, count]) => [key, count / draws]));
}

function sharesOf(draw: MascotDraw): Record<string, number> {
  return Object.fromEntries(draw.shares.map(({ pool, share }) => [pool, share]));
}

// =========================================================
// Layer 1 and 2: the effective shares
// =========================================================

describe("drawMascot: shares", () => {
  it("gives the all-year pool everything when no condition holds", () => {
    expect(drawMascot(input()).shares).toEqual([{ pool: ALL_YEAR, share: 1 }]);
  });

  it.each([
    ["fullMoonNight", "FullMoon", 0.9],
    ["newYear", "NewYear", 0.9],
    ["christmas", "Christmas", 0.4],
    ["halloween", "Halloween", 0.4],
    ["easter", "Easter", 0.4],
  ])("gives %s alone its share", (condition, pool, share) => {
    const draw = drawMascot(input({ conditions: conditions([condition]) }));
    expect(draw.shares.map((s) => s.pool)).toEqual([pool, ALL_YEAR]);
    expect(sharesOf(draw)[pool]).toBeCloseTo(share, 10);
    expect(sharesOf(draw)[ALL_YEAR]).toBeCloseTo(1 - share, 10);
  });

  it("splits a full-moon night at Christmas 90 / 4 / 6", () => {
    const draw = drawMascot(input({ conditions: conditions(["fullMoonNight", "christmas"]) }));
    expect(draw.shares.map((s) => s.pool)).toEqual(["FullMoon", "Christmas", ALL_YEAR]);
    expect(sharesOf(draw).FullMoon).toBeCloseTo(0.9, 10);
    expect(sharesOf(draw).Christmas).toBeCloseTo(0.04, 10);
    expect(sharesOf(draw)[ALL_YEAR]).toBeCloseTo(0.06, 10);
  });

  it("splits a full-moon night on New Year's Eve 90 / 9 / 1", () => {
    const draw = drawMascot(input({ conditions: conditions(["newYear", "fullMoonNight"]) }));
    expect(draw.shares.map((s) => s.pool)).toEqual(["FullMoon", "NewYear", ALL_YEAR]);
    expect(sharesOf(draw).NewYear).toBeCloseTo(0.09, 10);
    expect(sharesOf(draw)[ALL_YEAR]).toBeCloseTo(0.01, 10);
  });

  it("draws the pools in proportion to their shares", () => {
    const seen = frequencies(
      20_000,
      input({ conditions: conditions(["fullMoonNight", "christmas"]), random: seeded(7) }),
      (draw) => draw.pool,
    );
    expect(seen.FullMoon).toBeCloseTo(0.9, 1);
    expect(seen.Christmas).toBeCloseTo(0.04, 1);
    expect(seen[ALL_YEAR]).toBeCloseTo(0.06, 1);
  });

  it("walks the occasions in their order and takes one when its roll is below its share", () => {
    const held = conditions(["fullMoonNight", "christmas"]);
    // FullMoon misses (0.95 >= 0.9), Christmas hits (0.1 < 0.4), then the
    // character and the variant.
    const draw = drawMascot(input({ conditions: held, random: sequence(0.95, 0.1, 0, 0) }));
    expect(draw.pool).toBe("Christmas");
    expect(draw.variant).toBe("santa");
  });

  it("falls to the all-year pool when every occasion misses", () => {
    const draw = drawMascot(
      input({ conditions: conditions(["christmas"]), random: sequence(0.4, 0, 0) }),
    );
    expect(draw.pool).toBe(ALL_YEAR);
    expect(draw.variant).toBe("ghost");
  });

  it("passes the share of an occasion without an eligible character down", () => {
    const draw = drawMascot(
      input({
        conditions: conditions(["fullMoonNight", "christmas"]),
        pools: {
          ...pools,
          occasions: [{ ...pools.occasions[0], variants: ["wolf-nsfw"] }, pools.occasions[2]],
        },
        allowNsfw: false,
      }),
    );
    expect(draw.shares.map((s) => s.pool)).toEqual(["Christmas", ALL_YEAR]);
    expect(sharesOf(draw).Christmas).toBeCloseTo(0.4, 10);
    expect(draw.conditionsHeld).toEqual(["fullMoonNight", "christmas"]);
  });

  it("lets an occasion with an empty pool take no part", () => {
    const draw = drawMascot(
      input({
        conditions: conditions(["easter"]),
        pools: { ...pools, occasions: [{ ...pools.occasions[4], variants: [] }] },
      }),
    );
    expect(draw.shares).toEqual([{ pool: ALL_YEAR, share: 1 }]);
  });

  it("treats a condition the registry lacks as not holding", () => {
    const draw = drawMascot(input({ conditions: {} }));
    expect(draw.conditionsHeld).toEqual([]);
    expect(draw.pool).toBe(ALL_YEAR);
  });

  it("records the conditions that held, in the occasions' order", () => {
    const draw = drawMascot(input({ conditions: conditions(["easter", "fullMoonNight"]) }));
    expect(draw.conditionsHeld).toEqual(["fullMoonNight", "easter"]);
  });

  it("takes a 100% occasion every time", () => {
    const always = { ...pools, occasions: [{ ...pools.occasions[2], share: 100 }] };
    const seen = frequencies(
      500,
      input({ pools: always, conditions: conditions(["christmas"]) }),
      (draw) => draw.pool,
    );
    expect(seen).toEqual({ Christmas: 1 });
  });
});

// =========================================================
// Character, then variant
// =========================================================

describe("drawMascot: characters and variants", () => {
  const many = facts(
    "proton",
    "proton-2",
    "proton-3",
    "proton-4",
    "proton-5",
    "proton-6",
    "proton-7",
    "proton-8",
    "proton-9",
    "single",
  );
  const manyPools: MascotPools = { allYear: Object.keys(many), occasions: [] };

  it("draws a character with nine variants as often as one with a single variant", () => {
    const seen = frequencies(
      20_000,
      input({ pools: manyPools, mascots: many, random: seeded(3) }),
      (draw) => draw.character,
    );
    expect(seen.proton).toBeCloseTo(0.5, 1);
    expect(seen.single).toBeCloseTo(0.5, 1);
  });

  it("picks the variant uniformly among the character's variants in the pool", () => {
    const seen = frequencies(
      45_000,
      input({ pools: manyPools, mascots: many, random: seeded(4) }),
      (draw) => (draw.character === "proton" ? draw.variant : "other"),
    );
    for (const variant of Object.keys(many).filter((key) => key.startsWith("proton"))) {
      expect(seen[variant]).toBeCloseTo(0.5 / 9, 2);
    }
  });

  it("never doubles a character's chance through its twin", () => {
    const seen = frequencies(20_000, input({ random: seeded(5) }), (draw) => draw.character);
    for (const character of ["ghost", "original", "robot"]) {
      expect(seen[character]).toBeCloseTo(1 / 3, 1);
    }
  });

  it("only picks variants that are in the chosen pool", () => {
    const split: MascotPools = {
      allYear: ["ghost", "pumpkin"],
      occasions: [
        { group: "Halloween", condition: "halloween", share: 100, variants: ["pumpkin"] },
      ],
    };
    const sameCharacter = { ...mascots, ghost: { character: "pumpkin", nsfw: false } };
    const seen = frequencies(
      500,
      input({ pools: split, mascots: sameCharacter, conditions: conditions(["halloween"]) }),
      (draw) => draw.variant,
    );
    expect(seen).toEqual({ pumpkin: 1 });
  });

  it("records the draw", () => {
    // Characters ghost, original, robot: 0.9 picks robot; its variants
    // robot, robot-nsfw: 0.99 picks robot-nsfw.
    const draw = drawMascot(input({ random: sequence(0.9, 0.99) }));
    expect(draw).toEqual({
      variant: "robot-nsfw",
      character: "robot",
      pool: ALL_YEAR,
      conditionsHeld: [],
      shares: [{ pool: ALL_YEAR, share: 1 }],
      excluded: 0,
      remembered: 0,
      now: NOW,
    });
  });

  it("treats a variant without data as its own SFW character", () => {
    const draw = drawMascot(
      input({ pools: { allYear: ["stranger"], occasions: [] }, allowNsfw: false }),
    );
    expect(draw.variant).toBe("stranger");
    expect(draw.character).toBe("stranger");
  });
});

// =========================================================
// NSFW
// =========================================================

describe("drawMascot: NSFW", () => {
  it("never draws an NSFW variant while NSFW is off", () => {
    const base = input({
      allowNsfw: false,
      conditions: conditions(["fullMoonNight"]),
      random: seeded(9),
    });
    for (let i = 0; i < 2_000; i++) {
      expect(mascots[drawMascot(base).variant].nsfw).toBe(false);
    }
  });

  it("drops a character without an SFW variant and keeps the pool's share", () => {
    const nsfwOnly = { allYear: ["ghost", "robot-nsfw"], occasions: pools.occasions };
    const base = input({
      pools: nsfwOnly,
      allowNsfw: false,
      conditions: conditions(["christmas"]),
      random: seeded(11),
    });
    expect(sharesOf(drawMascot(base))[ALL_YEAR]).toBeCloseTo(0.6, 10);
    const seen = frequencies(2_000, base, (draw) => draw.character);
    expect(Object.keys(seen).sort()).toEqual(["ghost", "santa"]);
  });

  it("draws NSFW variants while NSFW is on", () => {
    const seen = frequencies(4_000, input({ random: seeded(12) }), (draw) => draw.variant);
    expect(seen["robot-nsfw"]).toBeGreaterThan(0);
  });
});

// =========================================================
// Recently shown characters
// =========================================================

describe("drawMascot: recently shown", () => {
  /** A pool of `k` characters named c0, c1, ... */
  function poolOf(k: number) {
    const keys = Array.from({ length: k }, (_, i) => `c${i}`);
    return { keys, pools: { allYear: keys, occasions: [] }, mascots: facts(...keys) };
  }

  it.each([
    [1, 0],
    [2, 0],
    [3, 1],
    [5, 2],
    [8, 3],
    [58, 20],
  ])("in a pool of %i characters with all of them recent, excludes %i", (k, expected) => {
    const { keys, pools: kPools, mascots: kMascots } = poolOf(k);
    const recent = keys.slice(0, RECENT_LIMIT);
    const draw = drawMascot(input({ pools: kPools, mascots: kMascots, recent }));
    expect(draw.excluded).toBe(expected);
  });

  it("excludes fewer than half even when the whole pool was shown recently", () => {
    const { keys, pools: kPools, mascots: kMascots } = poolOf(8);
    const seen = frequencies(
      8_000,
      input({
        pools: kPools,
        mascots: kMascots,
        recent: [...keys].reverse(),
        random: seeded(13),
      }),
      (draw) => draw.character,
    );
    // The three most recent (c7, c6, c5) are out, the other five share.
    expect(Object.keys(seen).sort()).toEqual(["c0", "c1", "c2", "c3", "c4"]);
  });

  it("excludes the most recent characters of the pool", () => {
    const { pools: kPools, mascots: kMascots } = poolOf(5);
    const seen = frequencies(
      5_000,
      input({ pools: kPools, mascots: kMascots, recent: ["c3", "c1"], random: seeded(14) }),
      (draw) => draw.character,
    );
    expect(Object.keys(seen).sort()).toEqual(["c0", "c2", "c4"]);
  });

  it("excludes only as many as it finds when few are recent", () => {
    const { pools: kPools, mascots: kMascots } = poolOf(58);
    const draw = drawMascot(input({ pools: kPools, mascots: kMascots, recent: ["c5", "c9"] }));
    expect(draw.excluded).toBe(2);
    expect(draw.remembered).toBe(2);
  });

  it("does not count recent characters of other pools against a thin pool", () => {
    const fullMoon = { ...pools, occasions: [{ ...pools.occasions[0], share: 100 }] };
    const draw = drawMascot(
      input({
        pools: fullMoon,
        conditions: conditions(["fullMoonNight"]),
        recent: ["ghost", "robot", "wolf"],
      }),
    );
    expect(draw.character).toBe("wolf");
    expect(draw.excluded).toBe(0);
  });

  it("skips the recent characters outside the pool when walking the list", () => {
    const { pools: kPools, mascots: kMascots } = poolOf(3);
    const draw = drawMascot(
      input({ pools: kPools, mascots: kMascots, recent: ["x", "y", "c2", "c1"] }),
    );
    expect(draw.excluded).toBe(1);
    expect(draw.character).not.toBe("c2");
  });

  it("counts only the pool's eligible characters", () => {
    // With NSFW off the robot character still has its SFW variant: k = 3.
    const draw = drawMascot(input({ allowNsfw: false, recent: ["robot", "ghost", "original"] }));
    expect(draw.excluded).toBe(1);
    expect(draw.character).not.toBe("robot");
  });
});

describe("rememberShown", () => {
  it("puts the shown character first", () => {
    expect(rememberShown(["b", "c"], "a")).toEqual(["a", "b", "c"]);
  });

  it("moves a character already in the list to the front", () => {
    expect(rememberShown(["b", "a", "c"], "a")).toEqual(["a", "b", "c"]);
    expect(rememberShown(["a", "b"], "a")).toEqual(["a", "b"]);
  });

  it("keeps the 20 most recent", () => {
    const full = Array.from({ length: RECENT_LIMIT }, (_, i) => `c${i}`);
    const next = rememberShown(full, "new");
    expect(RECENT_LIMIT).toBe(20);
    expect(next).toHaveLength(20);
    expect(next[0]).toBe("new");
    expect(next).not.toContain("c19");
  });

  it("leaves the given list unchanged", () => {
    const recent = Object.freeze(["b", "c"]);
    rememberShown(recent, "a");
    expect(recent).toEqual(["b", "c"]);
  });
});

// =========================================================
// The SFW variant of a drawn character
// =========================================================

describe("safeVariantInPool", () => {
  it("returns the first SFW variant of the character in the draw's pool", () => {
    const draw = drawMascot(input({ random: sequence(0.9, 0.99) }));
    expect(draw.variant).toBe("robot-nsfw");
    expect(safeVariantInPool(draw, pools, mascots)).toBe("robot");
  });

  it("looks in the draw's occasion pool", () => {
    const draw = drawMascot(
      input({ conditions: conditions(["fullMoonNight"]), random: sequence(0, 0, 0.99) }),
    );
    expect(draw).toMatchObject({ pool: "FullMoon", variant: "wolf-nsfw" });
    expect(safeVariantInPool(draw, pools, mascots)).toBe("wolf");
  });

  it("returns null for a character without an SFW variant in the pool", () => {
    const nsfwOnly = { allYear: ["robot-nsfw"], occasions: [] };
    const draw = drawMascot(input({ pools: nsfwOnly }));
    expect(safeVariantInPool(draw, nsfwOnly, mascots)).toBeNull();
  });

  it("returns null for a pool the pools do not have", () => {
    const draw = { ...drawMascot(input()), pool: "Gone" };
    expect(safeVariantInPool(draw, pools, mascots)).toBeNull();
  });
});
