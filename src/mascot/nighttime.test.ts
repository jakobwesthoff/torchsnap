// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { afterEach, describe, expect, it, vi } from "vitest";
import { isNighttime, isTwilight } from "./nighttime";

// The location comes from the system time zone, and the clock hours
// from the same zone, so each test runs in the zone it describes. Node
// applies a changed `TZ` to both `Date` and `Intl` at once.
function inTimeZone(timeZone: string) {
  vi.stubEnv("TZ", timeZone);
}

afterEach(() => {
  vi.unstubAllEnvs();
});

/** A local date in the current `TZ`, with minutes. */
function at(year: number, month: number, day: number, hour: number, minute = 0): Date {
  return new Date(year, month, day, hour, minute);
}

// =========================================================
// From the location
// =========================================================

// Berlin (52.5 N) on the June solstice: sunset about 21:31, civil dusk
// about 22:22, civil dawn about 03:52, sunrise about 04:42 (CEST). On
// the December solstice: sunset about 15:56, civil dusk about 16:38 (CET).
describe("in Berlin", () => {
  it("tells day, twilight and night apart on a summer evening", () => {
    inTimeZone("Europe/Berlin");
    const expected = [
      [at(2026, 5, 21, 12), false, false],
      [at(2026, 5, 21, 21, 0), false, false],
      [at(2026, 5, 21, 21, 55), false, true],
      [at(2026, 5, 21, 23, 0), true, false],
      [at(2026, 5, 21, 1, 0), true, false],
      [at(2026, 5, 21, 4, 15), false, true],
      [at(2026, 5, 21, 5, 30), false, false],
    ] as const;
    for (const [date, night, twilight] of expected) {
      expect([isNighttime(date), isTwilight(date)], date.toString()).toEqual([night, twilight]);
    }
  });

  it("is already night at half past five on a winter evening", () => {
    inTimeZone("Europe/Berlin");
    expect(isNighttime(at(2026, 11, 21, 17, 30))).toBe(true);
    expect(isNighttime(at(2026, 5, 21, 17, 30))).toBe(false);
  });

  it("is twilight shortly after a winter sunset", () => {
    inTimeZone("Europe/Berlin");
    expect(isTwilight(at(2026, 11, 21, 16, 15))).toBe(true);
    expect(isNighttime(at(2026, 11, 21, 16, 15))).toBe(false);
  });
});

describe("in the southern hemisphere", () => {
  // Sydney (33.9 S): June is winter, so 18:30 is dark there while the
  // December evening is still light.
  it("follows the local season", () => {
    inTimeZone("Australia/Sydney");
    expect(isNighttime(at(2026, 5, 21, 18, 30))).toBe(true);
    expect(isNighttime(at(2026, 11, 21, 18, 30))).toBe(false);
  });
});

describe("beyond the polar circle", () => {
  // Longyearbyen (78 N): the sun never sets in June and stays below -6°
  // all day in December.
  it("has neither night nor twilight under the midnight sun", () => {
    inTimeZone("Arctic/Longyearbyen");
    expect(isNighttime(at(2026, 5, 21, 0, 30))).toBe(false);
    expect(isTwilight(at(2026, 5, 21, 0, 30))).toBe(false);
  });

  it("is night at noon in the polar night", () => {
    inTimeZone("Arctic/Longyearbyen");
    expect(isNighttime(at(2026, 11, 21, 12))).toBe(true);
    expect(isTwilight(at(2026, 11, 21, 12))).toBe(false);
  });
});

// =========================================================
// Fallback for zones without coordinates
// =========================================================

describe("in a zone without coordinates", () => {
  it("uses night from 19:00 to 06:00", () => {
    inTimeZone("UTC");
    expect(isNighttime(at(2026, 5, 21, 18, 59))).toBe(false);
    expect(isNighttime(at(2026, 5, 21, 19, 0))).toBe(true);
    expect(isNighttime(at(2026, 5, 21, 5, 59))).toBe(true);
    expect(isNighttime(at(2026, 5, 21, 6, 0))).toBe(false);
  });

  it("uses twilight for 35 minutes on the day side of each boundary", () => {
    inTimeZone("UTC");
    expect(isTwilight(at(2026, 5, 21, 18, 24))).toBe(false);
    expect(isTwilight(at(2026, 5, 21, 18, 25))).toBe(true);
    expect(isTwilight(at(2026, 5, 21, 19, 0))).toBe(false);
    expect(isTwilight(at(2026, 5, 21, 6, 0))).toBe(true);
    expect(isTwilight(at(2026, 5, 21, 6, 34))).toBe(true);
    expect(isTwilight(at(2026, 5, 21, 6, 35))).toBe(false);
  });
});
