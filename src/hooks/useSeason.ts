// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Hemisphere-aware season detection.
 *
 * Determines the current meteorological season based on the user's
 * timezone. Southern hemisphere seasons are automatically inverted.
 */

import { getHemisphere } from "./useHemisphere";

export type Season = "spring" | "summer" | "autumn" | "winter";

// =========================================================
// Season Calculation
// =========================================================

/** Meteorological season boundaries by month (0-indexed). */
const NORTHERN_SEASONS: Record<number, Season> = {
  0: "winter", // January
  1: "winter", // February
  2: "spring", // March
  3: "spring", // April
  4: "spring", // May
  5: "summer", // June
  6: "summer", // July
  7: "summer", // August
  8: "autumn", // September
  9: "autumn", // October
  10: "autumn", // November
  11: "winter", // December
};

const SEASON_INVERSION: Record<Season, Season> = {
  spring: "autumn",
  summer: "winter",
  autumn: "spring",
  winter: "summer",
};

/**
 * Returns the current meteorological season, adjusted for hemisphere.
 */
export function getSeason(now: Date = new Date()): Season {
  const month = now.getMonth();
  const northern = NORTHERN_SEASONS[month];
  const hemisphere = getHemisphere();

  return hemisphere === "southern" ? SEASON_INVERSION[northern] : northern;
}

/**
 * Hook wrapper around {@link getSeason}.
 */
export function useSeason(): Season {
  return getSeason();
}
