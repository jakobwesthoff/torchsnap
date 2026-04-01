// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Nighttime detection for mascot condition callbacks.
 *
 * Computes civil twilight times (sun 6° below the horizon) using the
 * user's approximate latitude, derived from their IANA system timezone
 * via the coordinate table in `src/derived/timezone-coordinates.json`.
 *
 * Civil twilight is the threshold at which the sky is noticeably dark
 * and street lights come on — the right moment for a werewolf to
 * appear.
 *
 * Falls back to a fixed 19:00–06:00 window when the timezone cannot
 * be mapped to coordinates (e.g. "UTC" or an unrecognized zone).
 */

import { civilTwilightHourAngle, isMidnightSun } from "../lib/astronomy";
import timezoneCoords from "../derived/timezone-coordinates.json";

// =========================================================
// Timezone → Coordinates Lookup
// =========================================================

const coords = timezoneCoords as unknown as Record<string, [lat: number, lon: number]>;

/**
 * Look up the approximate coordinates for the user's system timezone.
 * Returns `null` for timezones not in the table (e.g. "UTC", "GMT").
 */
function getSystemCoordinates(): [lat: number, lon: number] | null {
  try {
    const tz = Intl.DateTimeFormat().resolvedOptions().timeZone;
    return coords[tz] ?? null;
  } catch {
    return null;
  }
}

// =========================================================
// Fallback: Fixed Time Window
// =========================================================

const FALLBACK_NIGHT_START = 19;
const FALLBACK_NIGHT_END = 6;

function isNighttimeFallback(now: Date): boolean {
  const hour = now.getHours();
  return hour >= FALLBACK_NIGHT_START || hour < FALLBACK_NIGHT_END;
}

// =========================================================
// Solar Nighttime Detection
// =========================================================

/**
 * Compute the local hour (fractional, 0–24) of civil twilight start
 * (morning) and end (evening) for the given date and coordinates.
 *
 * Solar noon in local time is approximated as:
 *
 *     solarNoon = 12 − (longitude / 15) + timezoneOffsetHours
 *
 * This is accurate to within ~15 minutes (the equation of time is
 * omitted), which is more than adequate for "is it dark?" purposes.
 */
function getTwilightTimes(
  now: Date,
  lat: number,
  lon: number,
): { morning: number; evening: number } | "always-light" | "always-dark" {
  const hourAngle = civilTwilightHourAngle(now, lat);

  if (hourAngle === null) {
    return isMidnightSun(now, lat) ? "always-light" : "always-dark";
  }

  // Timezone offset in hours (positive = east of UTC).
  const tzOffsetHours = -now.getTimezoneOffset() / 60;

  // Approximate solar noon in local clock hours. The longitude
  // correction accounts for the observer's position within their
  // timezone — a city at the western edge sees solar noon ~30 min
  // later than one at the eastern edge.
  const solarNoon = 12 - lon / 15 + tzOffsetHours;

  return {
    morning: solarNoon - hourAngle,
    evening: solarNoon + hourAngle,
  };
}

// =========================================================
// Public API
// =========================================================

/**
 * Returns `true` when it is past civil twilight (dark outside) at
 * the user's approximate location.
 *
 * Uses the system timezone to look up coordinates and compute
 * sunset/sunrise astronomically. Falls back to a fixed 19:00–06:00
 * window for unrecognized timezones.
 */
export function isNighttime(now: Date = new Date()): boolean {
  const location = getSystemCoordinates();
  if (!location) return isNighttimeFallback(now);

  const [lat, lon] = location;
  const twilight = getTwilightTimes(now, lat, lon);

  if (twilight === "always-light") return false;
  if (twilight === "always-dark") return true;

  // Current time as a fractional hour (e.g. 19.5 = 19:30).
  const currentHour = now.getHours() + now.getMinutes() / 60;

  return currentHour >= twilight.evening || currentHour < twilight.morning;
}

/**
 * Hook wrapper around {@link isNighttime}.
 *
 * Pure arithmetic — trivially cheap to recompute. The returned boolean
 * changes at most twice per day (sunrise and sunset), so downstream
 * comparisons remain stable within a single render cycle.
 */
export function useNighttime(): boolean {
  return isNighttime();
}
