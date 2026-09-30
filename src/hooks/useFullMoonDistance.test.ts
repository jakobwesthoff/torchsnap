// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { describe, expect, it } from "vitest";
import { getFullMoonDistance, getLunarPhase } from "./useFullMoonDistance";

// Reference instants in UTC: the full moon of the total lunar eclipse
// on 2026-03-03 (11:33 UTC) and the new moon of the total solar eclipse
// on 2026-08-12 (17:37 UTC). The phase works on the instant alone, so
// the tests hold in any time zone.
const FULL_MOON = new Date(Date.UTC(2026, 2, 3, 11, 33));
const NEW_MOON = new Date(Date.UTC(2026, 7, 12, 17, 37));
const DAY = 86_400_000;

describe("getLunarPhase", () => {
  it("is 0.5 at a full moon", () => {
    expect(getLunarPhase(FULL_MOON)).toBeCloseTo(0.5, 1);
  });

  it("is next to 0 or 1 at a new moon", () => {
    const phase = getLunarPhase(NEW_MOON);
    expect(Math.min(phase, 1 - phase)).toBeLessThan(0.05);
  });

  it("stays in [0, 1)", () => {
    for (let day = 0; day < 60; day++) {
      const phase = getLunarPhase(new Date(FULL_MOON.getTime() + day * DAY));
      expect(phase).toBeGreaterThanOrEqual(0);
      expect(phase).toBeLessThan(1);
    }
  });
});

describe("getFullMoonDistance", () => {
  it("is 0 on the day of a full moon", () => {
    expect(getFullMoonDistance(FULL_MOON)).toBe(0);
  });

  it("counts whole days on both sides of a full moon", () => {
    expect(getFullMoonDistance(new Date(FULL_MOON.getTime() - DAY))).toBe(1);
    expect(getFullMoonDistance(new Date(FULL_MOON.getTime() + DAY))).toBe(1);
  });

  it("is out of the one-day window three days away", () => {
    expect(getFullMoonDistance(new Date(FULL_MOON.getTime() - 3 * DAY))).toBeGreaterThan(1);
    expect(getFullMoonDistance(new Date(FULL_MOON.getTime() + 3 * DAY))).toBeGreaterThan(1);
  });

  it("is about half a lunar month at a new moon", () => {
    expect(getFullMoonDistance(NEW_MOON)).toBeGreaterThanOrEqual(14);
    expect(getFullMoonDistance(NEW_MOON)).toBeLessThanOrEqual(15);
  });
});
