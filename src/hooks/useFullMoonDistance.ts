// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Distance in whole days to the nearest full moon (past or future).
 *
 * Uses the low-precision Meeus algorithm (Jean Meeus, *Astronomical
 * Algorithms*) to compute the lunar phase from sun/moon ecliptic
 * coordinates. Accurate to within a few hours for the current century
 * — more than enough for mascot selection purposes.
 */

import { RAD, declination, rightAscension, sunCoords, toDays } from "../lib/astronomy";

const { sin, cos, acos, atan2, round, abs, PI } = Math;

// =========================================================
// Moon Position (low precision)
// =========================================================

function moonCoords(d: number) {
  const L = RAD * (218.316 + 13.176396 * d);
  const M = RAD * (134.963 + 13.064993 * d);
  const F = RAD * (93.272 + 13.22935 * d);

  const l = L + RAD * 6.289 * sin(M);
  const b = RAD * 5.128 * sin(F);
  const dist = 385_001 - 20_905 * cos(M);

  return { ra: rightAscension(l, b), dec: declination(l, b), dist };
}

// =========================================================
// Lunar Phase
// =========================================================

/**
 * Compute the lunar phase as a value in [0, 1).
 *
 * - 0.00 = new moon
 * - 0.25 = first quarter
 * - 0.50 = full moon
 * - 0.75 = last quarter
 */
export function getLunarPhase(date: Date): number {
  const d = toDays(date);
  const s = sunCoords(d);
  const m = moonCoords(d);

  const EARTH_SUN_DIST = 149_598_000; // km

  // Angular separation between sun and moon as seen from Earth.
  const phi = acos(sin(s.dec) * sin(m.dec) + cos(s.dec) * cos(m.dec) * cos(s.ra - m.ra));

  // Phase angle (elongation).
  const inc = atan2(EARTH_SUN_DIST * sin(phi), m.dist - EARTH_SUN_DIST * cos(phi));

  // Sign of the angle tells us waxing vs. waning.
  const angle = atan2(
    cos(s.dec) * sin(s.ra - m.ra),
    sin(s.dec) * cos(m.dec) - cos(s.dec) * sin(m.dec) * cos(s.ra - m.ra),
  );

  return 0.5 + (0.5 * inc * (angle < 0 ? -1 : 1)) / PI;
}

// =========================================================
// Full Moon Distance
// =========================================================

/** Mean synodic month length in days (new moon → new moon). */
const SYNODIC_MONTH = 29.53059;

/**
 * Returns the number of whole days to the nearest full moon.
 *
 * - `0` on the day of a full moon.
 * - Uses the phase value to estimate position within the current cycle,
 *   then converts the angular distance from full moon (phase 0.5) to days.
 */
export function getFullMoonDistance(now: Date = new Date()): number {
  const phase = getLunarPhase(now);

  // Distance from full moon in phase units (0–0.5 range).
  const phaseDistance = abs(phase - 0.5);

  // Convert phase distance to days. A full cycle (1.0) = SYNODIC_MONTH days.
  const dayDistance = phaseDistance * SYNODIC_MONTH;

  return round(dayDistance);
}

/**
 * Hook wrapper around {@link getFullMoonDistance}.
 *
 * Pure arithmetic — trivially cheap to recompute. The returned integer
 * only changes once per day, so downstream comparisons remain stable.
 */
export function useFullMoonDistance(): number {
  return getFullMoonDistance();
}
