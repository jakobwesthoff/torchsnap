// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Low-precision solar and coordinate primitives based on Jean Meeus,
 * *Astronomical Algorithms*. Accurate to within a few hours for the
 * current century — more than enough for mascot selection and
 * nighttime detection.
 *
 * This module is intentionally limited to the building blocks that
 * both the lunar phase and sunset/sunrise calculations need. Higher-
 * level functions (moon coordinates, full moon distance, twilight
 * times) live in their respective hook modules.
 */

const { sin, cos, atan2, PI } = Math;

// =========================================================
// Constants
// =========================================================

/** Degrees-to-radians multiplier. */
export const RAD = PI / 180;

/** Milliseconds in one day. */
export const MS_PER_DAY = 86_400_000;

/** Julian date of the Unix epoch (1970-01-01T12:00:00 UTC). */
const J1970 = 2_440_588;

/** Julian date of the J2000.0 epoch (2000-01-01T12:00:00 UTC). */
const J2000 = 2_451_545;

/** Obliquity of the ecliptic (radians). */
export const OBLIQUITY = RAD * 23.4397;

// =========================================================
// Julian Date Helpers
// =========================================================

/** Convert a JS Date to days since J2000.0. */
export function toDays(date: Date): number {
  return date.valueOf() / MS_PER_DAY - 0.5 + J1970 - J2000;
}

// =========================================================
// Coordinate Helpers
// =========================================================

/** Declination from ecliptic longitude and latitude (radians). */
export function declination(l: number, b: number): number {
  return Math.asin(sin(b) * cos(OBLIQUITY) + cos(b) * sin(OBLIQUITY) * sin(l));
}

/** Right ascension from ecliptic longitude and latitude (radians). */
export function rightAscension(l: number, b: number): number {
  return atan2(sin(l) * cos(OBLIQUITY) - Math.tan(b) * sin(OBLIQUITY), cos(l));
}

// =========================================================
// Sun Position (low precision)
// =========================================================

/** Mean anomaly of the sun at a given Julian day offset from J2000.0. */
export function solarMeanAnomaly(d: number): number {
  return RAD * (357.5291 + 0.98560028 * d);
}

/** Ecliptic longitude of the sun from its mean anomaly. */
export function eclipticLongitude(M: number): number {
  const C = RAD * (1.9148 * sin(M) + 0.02 * sin(2 * M) + 0.0003 * sin(3 * M));
  const P = RAD * 102.9372;
  return M + C + P + PI;
}

/** Equatorial coordinates (declination, right ascension) of the sun. */
export function sunCoords(d: number): { dec: number; ra: number } {
  const M = solarMeanAnomaly(d);
  const L = eclipticLongitude(M);
  return { dec: declination(L, 0), ra: rightAscension(L, 0) };
}

// =========================================================
// Twilight / Sunrise / Sunset
// =========================================================

/** Apparent sun altitude at the horizon, accounting for atmospheric refraction. */
export const SUNSET_ALTITUDE = RAD * -0.833;

/** Sun altitude at civil twilight (6° below horizon). */
export const CIVIL_TWILIGHT_ALTITUDE = RAD * -6;

/**
 * Hour angle (in **hours**) at which the sun crosses a given altitude
 * threshold for a given date and observer latitude.
 *
 * The hour angle is the time offset from solar noon. For example,
 * sunset = solar noon + H, sunrise = solar noon − H.
 *
 * Returns `null` when the sun never crosses the threshold:
 * - `cosH > 1`: sun is always below (polar night at this altitude).
 * - `cosH < -1`: sun is always above (midnight sun at this altitude).
 *
 * Common altitudes:
 * - {@link SUNSET_ALTITUDE} (−0.833°) — sunrise / sunset
 * - {@link CIVIL_TWILIGHT_ALTITUDE} (−6°) — civil twilight boundary
 */
export function sunAltitudeHourAngle(date: Date, latDeg: number, altitude: number): number | null {
  const d = toDays(date);
  const { dec } = sunCoords(d);
  const lat = latDeg * RAD;

  const cosH = (sin(altitude) - sin(lat) * sin(dec)) / (cos(lat) * cos(dec));

  // Sun never crosses this altitude at this latitude/date.
  if (cosH < -1 || cosH > 1) return null;

  // Convert hour angle from radians to hours (π rad = 12 h).
  return Math.acos(cosH) * (12 / PI);
}

/**
 * Returns `true` when the sun is always above the given altitude on
 * the given date at the given latitude. Useful to distinguish the
 * two `null` cases from {@link sunAltitudeHourAngle}.
 */
export function isSunAlwaysAbove(date: Date, latDeg: number, altitude: number): boolean {
  const d = toDays(date);
  const { dec } = sunCoords(d);
  const lat = latDeg * RAD;

  const cosH = (sin(altitude) - sin(lat) * sin(dec)) / (cos(lat) * cos(dec));

  // cosH < -1 means the sun is always above this altitude.
  return cosH < -1;
}
