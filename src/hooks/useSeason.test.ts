// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { afterEach, describe, expect, it, vi } from "vitest";
import { getHemisphere } from "./useHemisphere";
import { getSeason } from "./useSeason";

// Both read the system time zone; Node applies a changed `TZ` at once.
afterEach(() => {
  vi.unstubAllEnvs();
});

describe("getHemisphere", () => {
  it.each([
    ["Europe/Berlin", "northern"],
    ["America/New_York", "northern"],
    ["UTC", "northern"],
    ["Australia/Sydney", "southern"],
    ["America/Argentina/Buenos_Aires", "southern"],
    ["Pacific/Auckland", "southern"],
    ["Africa/Johannesburg", "southern"],
  ])("%s is %s", (timeZone, hemisphere) => {
    vi.stubEnv("TZ", timeZone);
    expect(getHemisphere()).toBe(hemisphere);
  });
});

describe("getSeason", () => {
  it("follows the meteorological seasons in the north", () => {
    vi.stubEnv("TZ", "Europe/Berlin");
    expect(getSeason(new Date(2026, 1, 28))).toBe("winter");
    expect(getSeason(new Date(2026, 2, 1))).toBe("spring");
    expect(getSeason(new Date(2026, 5, 1))).toBe("summer");
    expect(getSeason(new Date(2026, 8, 1))).toBe("autumn");
    expect(getSeason(new Date(2026, 10, 30))).toBe("autumn");
    expect(getSeason(new Date(2026, 11, 1))).toBe("winter");
  });

  it("inverts the seasons in the south", () => {
    vi.stubEnv("TZ", "Australia/Sydney");
    expect(getSeason(new Date(2026, 0, 15))).toBe("summer");
    expect(getSeason(new Date(2026, 3, 15))).toBe("autumn");
    expect(getSeason(new Date(2026, 6, 15))).toBe("winter");
    expect(getSeason(new Date(2026, 9, 15))).toBe("spring");
  });
});
