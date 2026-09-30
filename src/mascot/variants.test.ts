// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { afterEach, describe, expect, it, vi } from "vitest";
import mascotData from "../derived/mascots.json";
import groupData from "../derived/mascot-groups.json";
import {
  OCCASION_GROUPS,
  buildMascotSelection,
  mascotSelection,
  getMascotAlt,
  getMascotAnchor,
  isNsfwVariant,
  type MascotGroupInfo,
  type MascotInfo,
} from "./variants";
import type { MascotEntry } from "./selection";

const data = mascotData as Record<string, MascotInfo>;
const groups = groupData as Record<string, MascotGroupInfo>;

function entry(overrides: Partial<MascotInfo>): MascotInfo {
  return {
    alt: "Snappy -- test.",
    nsfw: false,
    group: "SciFi",
    occasions: [],
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
// Selection
// =========================================================

describe("buildMascotSelection", () => {
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
    NewYear: group(true),
    FullMoon: group(true),
  };
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
    party: entry({ group: "NewYear" }),
    werewolf: entry({ group: "FullMoon" }),
  };
  const [regular, halloween, christmas, easter, newYear, fullMoon] = buildMascotSelection(
    fixture,
    fixtureGroups,
  );

  it("puts every group that shows all year into the regular pool, unknown groups included", () => {
    expect(variantsOf(regular)).toEqual([
      "brand-new-group",
      "original",
      "rabbit",
      "robot",
      "slasher",
    ]);
    expect(regular.weight).toBe(100);
    expect("condition" in regular).toBe(false);
  });

  it("keeps the seasonal groups out of the regular pool", () => {
    for (const key of ["pumpkin", "skeleton", "santa", "bunny", "party", "werewolf"]) {
      expect(variantsOf(regular)).not.toContain(key);
    }
  });

  it("adds a mascot to every occasion it names and keeps it in its own group", () => {
    expect(variantsOf(halloween)).toContain("slasher");
    expect(variantsOf(easter)).toContain("rabbit");
    expect(variantsOf(regular)).toEqual(expect.arrayContaining(["slasher", "rabbit"]));
  });

  it("lets a seasonal mascot join another occasion", () => {
    expect(variantsOf(christmas)).toEqual(["santa", "skeleton"]);
    expect(variantsOf(halloween)).toContain("skeleton");
  });

  it("gives each occasion a dormant entry with its condition and boost", () => {
    const occasions = [halloween, christmas, easter, newYear, fullMoon];
    expect(occasions.map(variantsOf)).toEqual([
      ["pumpkin", "skeleton", "slasher"],
      ["santa", "skeleton"],
      ["bunny", "rabbit"],
      ["party"],
      ["werewolf"],
    ]);
    for (const occasion of occasions) {
      expect(occasion.weight).toBe(0);
      expect("condition" in occasion).toBe(true);
    }
    expect(occasions.map((o) => ("conditionalWeight" in o ? o.conditionalWeight : null))).toEqual([
      35, 70, 35, 1000, 1000,
    ]);
  });

  it("leaves out a seasonal group without a condition and warns", () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    const sets = buildMascotSelection(
      { comet: entry({ group: "Comet" }), robot: entry({ group: "SciFi" }) },
      { SciFi: group(false), Comet: group(true) },
    );
    expect(sets.flatMap(variantsOf)).toEqual(["robot"]);
    expect(warn).toHaveBeenCalledOnce();
  });
});

describe("mascotSelection", () => {
  it("offers every mascot of the data", () => {
    const offered = new Set(mascotSelection.flatMap(variantsOf));
    expect([...offered].sort()).toEqual(Object.keys(data).sort());
  });

  it("only offers mascots the data knows", () => {
    for (const key of mascotSelection.flatMap(variantsOf)) {
      expect(data[key]).toBeDefined();
    }
  });

  // `useMascotVariant` draws from the SFW pool on every roll, and the
  // selection throws when nothing is left.
  it("has a safe mascot in an entry that always has weight", () => {
    const alwaysWeighted = mascotSelection.filter((set) => !("condition" in set) && set.weight > 0);
    const safe = alwaysWeighted.flatMap(variantsOf).filter((key) => !data[key].nsfw);
    expect(safe.length).toBeGreaterThan(0);
  });

  it("has a condition for every seasonal group of the data, and a seasonal group for every condition", () => {
    const seasonal = Object.keys(groups).filter((key) => groups[key].seasonal);
    expect([...OCCASION_GROUPS].sort()).toEqual(seasonal.sort());
  });

  it("names only groups the group data knows", () => {
    for (const mascot of Object.values(data)) {
      expect(groups[mascot.group]).toBeDefined();
      for (const occasion of mascot.occasions) {
        expect(groups[occasion]?.seasonal).toBe(true);
      }
    }
  });
});
