// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { afterEach, describe, expect, it, vi } from "vitest";
import groupData from "../derived/mascot-groups.json";
import occasionData from "./occasions.json";
import { CONDITIONS } from "./conditions";

const groups = groupData as Record<string, { seasonal: boolean }>;
const { occasions } = occasionData;

afterEach(() => {
  vi.unstubAllEnvs();
});

// =========================================================
// The occasions file against the registry and the groups
// =========================================================

describe("occasions.json", () => {
  it("names only registered conditions, and uses every registered one", () => {
    const named = occasions.map((occasion) => occasion.condition);
    for (const name of named) expect(CONDITIONS[name]).toBeDefined();
    expect([...named].sort()).toEqual(Object.keys(CONDITIONS).sort());
  });

  it("gives each seasonal group exactly one occasion and names no other group", () => {
    const seasonal = Object.keys(groups).filter((group) => groups[group].seasonal);
    expect(occasions.map((occasion) => occasion.group).sort()).toEqual(seasonal.sort());
  });

  it("names no group twice", () => {
    const named = occasions.map((occasion) => occasion.group);
    expect(new Set(named).size).toBe(named.length);
  });

  it("gives every occasion a whole-number share from 1 to 100", () => {
    for (const { share } of occasions) {
      expect(Number.isInteger(share)).toBe(true);
      expect(share).toBeGreaterThanOrEqual(1);
      expect(share).toBeLessThanOrEqual(100);
    }
  });
});

// =========================================================
// The conditions on fixed dates
// =========================================================

describe("CONDITIONS", () => {
  it("describes every condition", () => {
    for (const condition of Object.values(CONDITIONS)) {
      expect(condition.description.length).toBeGreaterThan(0);
    }
  });

  it.each([
    ["halloween", new Date(2026, 9, 24, 0, 0), true],
    ["halloween", new Date(2026, 9, 31, 23, 59), true],
    ["halloween", new Date(2026, 9, 23, 23, 59), false],
    ["halloween", new Date(2026, 10, 1, 0, 0), false],
    ["christmas", new Date(2026, 11, 20, 0, 0), true],
    ["christmas", new Date(2026, 11, 26, 23, 59), true],
    ["christmas", new Date(2026, 11, 19, 23, 59), false],
    ["christmas", new Date(2026, 11, 27, 0, 0), false],
    ["newYear", new Date(2026, 11, 31, 20, 0), true],
    ["newYear", new Date(2027, 0, 1, 11, 59), true],
    ["newYear", new Date(2026, 11, 31, 19, 59), false],
    ["newYear", new Date(2027, 0, 1, 12, 0), false],
    // Easter 2027 is on March 28: Good Friday March 26, Easter Monday March 29.
    ["easter", new Date(2027, 2, 26, 0, 0), true],
    ["easter", new Date(2027, 2, 29, 23, 59), true],
    ["easter", new Date(2027, 2, 25, 23, 59), false],
    ["easter", new Date(2027, 2, 30, 0, 0), false],
  ])("%s on %s is %s", (name, now, expected) => {
    vi.stubEnv("TZ", "Europe/Berlin");
    expect(CONDITIONS[name].isActive(now)).toBe(expected);
  });

  // The full moon of 2026-03-03 11:33 UTC; Berlin is at UTC+1 then, with
  // civil dusk around 18:40 and civil dawn around 06:25.
  describe("fullMoonNight", () => {
    const fullMoonNight = CONDITIONS.fullMoonNight;

    it.each([
      ["the night after the full moon", new Date(2026, 2, 3, 23, 0), true],
      ["the night before it", new Date(2026, 2, 2, 23, 0), true],
      ["dusk on the full-moon day", new Date(2026, 2, 3, 18, 30), true],
      ["noon on the full-moon day", new Date(2026, 2, 3, 12, 0), false],
      ["a night three days later", new Date(2026, 2, 6, 23, 0), false],
      ["a night three days before", new Date(2026, 1, 28, 23, 0), false],
    ])("holds on %s: %s", (_label, now, expected) => {
      vi.stubEnv("TZ", "Europe/Berlin");
      expect(fullMoonNight.isActive(now)).toBe(expected);
    });

    it("names both parts of its window", () => {
      expect(fullMoonNight.description).toBe("full moon within 1.5 days, dusk to dawn");
    });
  });
});
