// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { afterEach, describe, expect, it, vi } from "vitest";
import mascotData from "../derived/mascots.json";
import groupData from "../derived/mascot-groups.json";
import {
  OCCASIONS,
  ORIGINAL_MASCOT,
  buildMascotPools,
  mascotFacts,
  mascotPools,
  getMascotAlt,
  getMascotAnchor,
  isNsfwVariant,
  type MascotGroupInfo,
  type MascotInfo,
  type Occasion,
} from "./variants";

const data = mascotData as Record<string, MascotInfo>;
const groups = groupData as Record<string, MascotGroupInfo>;

function entry(overrides: Partial<MascotInfo>): MascotInfo {
  return {
    alt: "Snappy -- test.",
    nsfw: false,
    character: "test",
    group: "SciFi",
    occasions: [],
    groundAnchor: { x: 0.5, y: 0.9 },
    boxLeft: 0.2,
    ...overrides,
  };
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
// Pools
// =========================================================

describe("buildMascotPools", () => {
  function group(seasonal: boolean): MascotGroupInfo {
    return { label: "Test", description: "Snappy tests.", seasonal };
  }
  const fixtureGroups: Record<string, MascotGroupInfo> = {
    OffDuty: group(false),
    SciFi: group(false),
    Horror: group(false),
    Cartoons: group(false),
    Halloween: group(true),
    Christmas: group(true),
    Easter: group(true),
  };
  const fixtureOccasions: Occasion[] = [
    { group: "Christmas", condition: "christmas", share: 40 },
    { group: "Halloween", condition: "halloween", share: 30 },
    { group: "Easter", condition: "easter", share: 20 },
  ];
  const fixture: Record<string, MascotInfo> = {
    original: entry({ group: "OffDuty" }),
    robot: entry({ group: "SciFi" }),
    "brand-new-group": entry({ group: "SomethingNew" }),
    pumpkin: entry({ group: "Halloween" }),
    slasher: entry({ group: "Horror", occasions: ["Halloween"] }),
    rabbit: entry({ group: "Cartoons", occasions: ["Easter"] }),
    skeleton: entry({ group: "Halloween", occasions: ["Christmas"] }),
    santa: entry({ group: "Christmas" }),
    bunny: entry({ group: "Easter" }),
  };
  const pools = buildMascotPools(fixture, fixtureGroups, fixtureOccasions);

  it("puts every group that shows all year into the all-year pool, unknown groups included", () => {
    expect(pools.allYear).toEqual(["brand-new-group", "original", "rabbit", "robot", "slasher"]);
  });

  it("keeps the seasonal groups out of the all-year pool", () => {
    for (const key of ["pumpkin", "skeleton", "santa", "bunny"]) {
      expect(pools.allYear).not.toContain(key);
    }
  });

  it("adds a mascot to every occasion it names and keeps it in the all-year pool", () => {
    const [, halloween, easter] = pools.occasions;
    expect(halloween.variants).toContain("slasher");
    expect(easter.variants).toContain("rabbit");
    expect(pools.allYear).toEqual(expect.arrayContaining(["slasher", "rabbit"]));
  });

  it("lets a seasonal mascot join another occasion", () => {
    const [christmas, halloween] = pools.occasions;
    expect(christmas.variants).toEqual(["santa", "skeleton"]);
    expect(halloween.variants).toContain("skeleton");
  });

  it("builds one pool per occasion in the file's order, with its condition and share", () => {
    expect(pools.occasions).toEqual([
      { group: "Christmas", condition: "christmas", share: 40, variants: ["santa", "skeleton"] },
      {
        group: "Halloween",
        condition: "halloween",
        share: 30,
        variants: ["pumpkin", "skeleton", "slasher"],
      },
      { group: "Easter", condition: "easter", share: 20, variants: ["bunny", "rabbit"] },
    ]);
  });

  it("always puts the plain Snappy into the all-year pool", () => {
    // As a group switch would: the data arrives without OffDuty.
    const withoutOffDuty = { robot: entry({ group: "SciFi" }) };
    expect(buildMascotPools(withoutOffDuty, fixtureGroups, []).allYear).toEqual([
      ORIGINAL_MASCOT,
      "robot",
    ]);
    // With nothing else left, it is the whole pool.
    expect(buildMascotPools({}, fixtureGroups, []).allYear).toEqual([ORIGINAL_MASCOT]);
  });

  it("keeps the plain Snappy in the all-year pool whatever its group", () => {
    const seasonalOriginal = { original: entry({ group: "Halloween" }) };
    const built = buildMascotPools(seasonalOriginal, fixtureGroups, fixtureOccasions);
    expect(built.allYear).toEqual([ORIGINAL_MASCOT]);
    expect(built.occasions[1].variants).toEqual([ORIGINAL_MASCOT]);
  });
});

describe("mascotPools", () => {
  const offered = new Set([
    ...mascotPools.allYear,
    ...mascotPools.occasions.flatMap((pool) => pool.variants),
  ]);

  it("offers every mascot of the data", () => {
    expect([...offered].sort()).toEqual(Object.keys(data).sort());
  });

  it("only offers mascots the data knows", () => {
    for (const key of offered) expect(data[key]).toBeDefined();
  });

  it("has one pool per entry of occasions.json, in its order", () => {
    expect(mascotPools.occasions.map((pool) => pool.group)).toEqual(OCCASIONS.map((o) => o.group));
  });

  it("has the plain Snappy in the data, SFW and in the all-year pool", () => {
    expect(data[ORIGINAL_MASCOT]).toBeDefined();
    expect(data[ORIGINAL_MASCOT].nsfw).toBe(false);
    expect(mascotPools.allYear).toContain(ORIGINAL_MASCOT);
  });

  it("names only groups the group data knows", () => {
    for (const mascot of Object.values(data)) {
      expect(groups[mascot.group]).toBeDefined();
      for (const occasion of mascot.occasions) {
        expect(groups[occasion]?.seasonal).toBe(true);
      }
    }
  });

  it("gives every mascot a character whose id is one of that character's variants", () => {
    for (const mascot of Object.values(mascotFacts)) {
      expect(mascotFacts[mascot.character]?.character).toBe(mascot.character);
    }
  });
});
