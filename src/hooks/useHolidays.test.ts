// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { describe, expect, it } from "vitest";
import {
  getActiveHolidays,
  getEasterDate,
  isChristmas,
  isEaster,
  isHalloween,
  isNewYear,
} from "./useHolidays";

// Every date here is built from local fields, as the checks read local
// fields, so the tests hold in any time zone.

describe("getEasterDate", () => {
  it.each([
    [2008, 2, 23],
    [2019, 3, 21],
    [2024, 2, 31],
    [2026, 3, 5],
    [2038, 3, 25],
    [2285, 2, 22],
  ])("puts Easter Sunday %i on the known date", (year, month, day) => {
    expect(getEasterDate(year)).toEqual(new Date(year, month, day));
  });
});

describe("isHalloween", () => {
  it.each([
    [new Date(2026, 9, 23, 23, 59), false],
    [new Date(2026, 9, 24, 0, 0), true],
    [new Date(2026, 9, 31, 23, 59), true],
    [new Date(2026, 10, 1, 0, 0), false],
  ])("%s -> %s", (date, expected) => {
    expect(isHalloween(date)).toBe(expected);
  });
});

describe("isChristmas", () => {
  it.each([
    [new Date(2026, 11, 19, 23, 59), false],
    [new Date(2026, 11, 20, 0, 0), true],
    [new Date(2026, 11, 26, 23, 59), true],
    [new Date(2026, 11, 27, 0, 0), false],
  ])("%s -> %s", (date, expected) => {
    expect(isChristmas(date)).toBe(expected);
  });
});

describe("isNewYear", () => {
  it.each([
    [new Date(2026, 11, 31, 19, 59), false],
    [new Date(2026, 11, 31, 20, 0), true],
    [new Date(2027, 0, 1, 11, 59), true],
    [new Date(2027, 0, 1, 12, 0), false],
  ])("%s -> %s", (date, expected) => {
    expect(isNewYear(date)).toBe(expected);
  });
});

describe("isEaster", () => {
  // Easter Sunday 2026 is April 5.
  it.each([
    [new Date(2026, 3, 2, 23, 59), false],
    [new Date(2026, 3, 3, 0, 0), true],
    [new Date(2026, 3, 6, 23, 59), true],
    [new Date(2026, 3, 7, 0, 0), false],
  ])("%s -> %s", (date, expected) => {
    expect(isEaster(date)).toBe(expected);
  });
});

describe("getActiveHolidays", () => {
  it("lists each active holiday", () => {
    expect(getActiveHolidays(new Date(2026, 9, 30))).toEqual(["halloween"]);
    expect(getActiveHolidays(new Date(2026, 11, 24))).toEqual(["christmas"]);
    expect(getActiveHolidays(new Date(2026, 11, 31, 22))).toEqual(["newYear"]);
    expect(getActiveHolidays(new Date(2026, 3, 5))).toEqual(["easter"]);
  });

  it("is empty outside every holiday", () => {
    expect(getActiveHolidays(new Date(2026, 6, 15))).toEqual([]);
  });
});
