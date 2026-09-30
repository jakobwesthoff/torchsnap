// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Holiday detection for mascot condition callbacks.
 *
 * Provides pure functions to check whether a given date falls within
 * a holiday period. Each holiday check returns `true` when active.
 * The `getActiveHolidays` function returns all currently active holidays.
 */

export type Holiday = "halloween" | "christmas" | "newYear" | "easter";

// =========================================================
// Easter Calculation (Anonymous Gregorian Algorithm)
// =========================================================

/**
 * Computes the date of Easter Sunday for a given year using the
 * Anonymous Gregorian algorithm (Meeus/Jones/Butcher variant).
 */
export function getEasterDate(year: number): Date {
  const a = year % 19;
  const b = Math.floor(year / 100);
  const c = year % 100;
  const d = Math.floor(b / 4);
  const e = b % 4;
  const f = Math.floor((b + 8) / 25);
  const g = Math.floor((b - f + 1) / 3);
  const h = (19 * a + b - d - g + 15) % 30;
  const i = Math.floor(c / 4);
  const k = c % 4;
  const l = (32 + 2 * e + 2 * i - h - k) % 7;
  const m = Math.floor((a + 11 * h + 22 * l) / 451);
  const month = Math.floor((h + l - 7 * m + 114) / 31);
  const day = ((h + l - 7 * m + 114) % 31) + 1;

  return new Date(year, month - 1, day);
}

// =========================================================
// Holiday Checks
// =========================================================

function daysBetween(a: Date, b: Date): number {
  const msPerDay = 86_400_000;
  // Normalize to midnight to avoid partial-day offsets.
  const aDay = new Date(a.getFullYear(), a.getMonth(), a.getDate());
  const bDay = new Date(b.getFullYear(), b.getMonth(), b.getDate());
  return Math.round((bDay.getTime() - aDay.getTime()) / msPerDay);
}

/** October 24–31. */
export function isHalloween(now: Date = new Date()): boolean {
  return now.getMonth() === 9 && now.getDate() >= 24;
}

/** December 20–26. */
export function isChristmas(now: Date = new Date()): boolean {
  const month = now.getMonth();
  const day = now.getDate();
  return month === 11 && day >= 20 && day <= 26;
}

/** December 31 20:00 – January 1 12:00. */
export function isNewYear(now: Date = new Date()): boolean {
  const month = now.getMonth();
  const day = now.getDate();
  const hour = now.getHours();
  return (month === 11 && day === 31 && hour >= 20) || (month === 0 && day === 1 && hour < 12);
}

/** Good Friday (−2) through Easter Monday (+1). */
export function isEaster(now: Date = new Date()): boolean {
  const year = now.getFullYear();

  // Check against current year's Easter and neighbouring years' Easter
  // in case we're near a year boundary (early January / late December).
  for (const y of [year - 1, year, year + 1]) {
    const easter = getEasterDate(y);
    const diff = daysBetween(easter, now);
    if (diff >= -2 && diff <= 1) {
      return true;
    }
  }

  return false;
}

// =========================================================
// Aggregate
// =========================================================

/**
 * Returns the list of all holidays currently active.
 */
export function getActiveHolidays(now: Date = new Date()): Holiday[] {
  const active: Holiday[] = [];
  if (isHalloween(now)) {
    active.push("halloween");
  }
  if (isChristmas(now)) {
    active.push("christmas");
  }
  if (isNewYear(now)) {
    active.push("newYear");
  }
  if (isEaster(now)) {
    active.push("easter");
  }
  return active;
}

/**
 * Hook wrapper around {@link getActiveHolidays}.
 */
export function useHolidays(): Holiday[] {
  return getActiveHolidays();
}
