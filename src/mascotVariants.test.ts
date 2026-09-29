// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { afterEach, describe, expect, it, vi } from "vitest";
import mascotData from "./derived/mascots.json";
import {
  SEASONAL_GROUPS,
  SnappyHeroSets,
  buildHeroSets,
  getMascotAlt,
  getMascotAnchor,
  isNsfwVariant,
  type MascotInfo,
} from "./mascotVariants";
import type { MascotEntry } from "./hooks/useRandomMascot";

const data = mascotData as Record<string, MascotInfo>;

function entry(overrides: Partial<MascotInfo>): MascotInfo {
  return {
    alt: "Snappy -- test.",
    nsfw: false,
    group: "SciFi",
    groundAnchor: { x: 0.5, y: 0.9 },
    boxLeft: 0.2,
    ...overrides,
  };
}

function variantsOf(set: MascotEntry): string[] {
  return "variants" in set ? set.variants : [];
}

// =========================================================
// Data lookups
// =========================================================

describe("getMascotAlt", () => {
  it("returns the alt text of a known mascot", () => {
    expect(getMascotAlt("original")).toBe(data.original.alt);
  });

  it("falls back to the key with spaces for an unknown mascot", () => {
    expect(getMascotAlt("no-such-mascot")).toBe("no such mascot");
  });
});

describe("isNsfwVariant", () => {
  it("follows the nsfw flag of the data", () => {
    const nsfw = Object.keys(data).find((key) => data[key].nsfw)!;
    expect(isNsfwVariant(nsfw)).toBe(true);
    expect(isNsfwVariant("original")).toBe(false);
  });

  it("treats an unknown mascot as safe", () => {
    expect(isNsfwVariant("no-such-mascot")).toBe(false);
  });
});

describe("getMascotAnchor", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("returns the ground anchor and visible left edge of a known mascot", () => {
    expect(getMascotAnchor("original")).toEqual({
      groundAnchor: data.original.groundAnchor,
      boxLeft: data.original.boxLeft,
    });
  });

  it("falls back to the image's bottom centre for an unknown mascot and warns once", () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    expect(getMascotAnchor("unknown-for-anchor")).toEqual({
      groundAnchor: { x: 0.5, y: 1 },
      boxLeft: 0,
    });
    getMascotAnchor("unknown-for-anchor");
    expect(warn).toHaveBeenCalledTimes(1);
  });
});

// =========================================================
// Selection sets
// =========================================================

describe("buildHeroSets", () => {
  const fixture: Record<string, MascotInfo> = {
    original: entry({ group: "Original" }),
    robot: entry({ group: "SciFi" }),
    "brand-new-group": entry({ group: "SomethingNew" }),
    ghost: entry({ group: "Halloween" }),
    slasher: entry({ group: "Horror" }),
    santa: entry({ group: "Christmas" }),
    bunny: entry({ group: "Easter" }),
    party: entry({ group: "NewYear" }),
    werewolf: entry({ group: "FullMoon" }),
  };
  const [regular, halloween, christmas, easter, newYear, fullMoon] = buildHeroSets(fixture);

  it("puts every non-seasonal group into the regular pool, new groups included", () => {
    expect(variantsOf(regular)).toEqual(["brand-new-group", "original", "robot", "slasher"]);
    expect(regular.weight).toBe(100);
    expect("condition" in regular).toBe(false);
  });

  it("keeps the seasonal groups out of the regular pool", () => {
    for (const key of ["ghost", "santa", "bunny", "party", "werewolf"]) {
      expect(variantsOf(regular)).not.toContain(key);
    }
  });

  it("boosts Horror together with Halloween", () => {
    expect(variantsOf(halloween)).toEqual(["ghost", "slasher"]);
  });

  it("gives each season its own dormant entry with its boost", () => {
    const seasons = [halloween, christmas, easter, newYear, fullMoon];
    expect(seasons.map(variantsOf)).toEqual([
      ["ghost", "slasher"],
      ["santa"],
      ["bunny"],
      ["party"],
      ["werewolf"],
    ]);
    for (const season of seasons) {
      expect(season.weight).toBe(0);
      expect("condition" in season).toBe(true);
    }
    expect(seasons.map((s) => ("conditionalWeight" in s ? s.conditionalWeight : null))).toEqual([
      35, 70, 35, 1000, 1000,
    ]);
  });
});

describe("SnappyHeroSets", () => {
  it("offers every mascot of the data", () => {
    const offered = new Set(SnappyHeroSets.flatMap(variantsOf));
    expect([...offered].sort()).toEqual(Object.keys(data).sort());
  });

  it("only offers mascots the data knows", () => {
    for (const key of SnappyHeroSets.flatMap(variantsOf)) {
      expect(data[key]).toBeDefined();
    }
  });

  it("has a seasonal entry for every seasonal group in the data", () => {
    const groups = new Set(Object.values(data).map((m) => m.group));
    for (const group of SEASONAL_GROUPS) {
      expect(groups.has(group)).toBe(true);
    }
  });
});
