// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Nighttime and twilight detection for mascot condition callbacks.
 *
 * Computes civil twilight and sunset/sunrise times using the user's
 * approximate latitude, derived from their IANA system timezone via
 * the coordinate table in `src/derived/timezone-coordinates.json`.
 *
 * Three states of sky darkness:
 *
 * - **Daylight**: sun above the horizon.
 * - **Twilight**: sun between the horizon and −6° (civil twilight).
 *   Sky is visibly dimming, street lights come on.
 * - **Nighttime**: sun below −6°. Properly dark.
 *
 * Falls back to fixed estimates when the timezone cannot be mapped
 * to coordinates (e.g. "UTC" or an unrecognized zone).
 */

import {
  CIVIL_TWILIGHT_ALTITUDE,
  SUNSET_ALTITUDE,
  isSunAlwaysAbove,
  sunAltitudeHourAngle,
} from "../lib/astronomy";
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
// Fallback: Fixed Time Windows
// =========================================================

const FALLBACK_NIGHT_START = 19;
const FALLBACK_NIGHT_END = 6;
const FALLBACK_TWILIGHT_MINUTES = 35;

function isNighttimeFallback(now: Date): boolean {
  const hour = now.getHours();
  return hour >= FALLBACK_NIGHT_START || hour < FALLBACK_NIGHT_END;
}

function isTwilightFallback(now: Date): boolean {
  const minutes = now.getHours() * 60 + now.getMinutes();
  const eveningStart = FALLBACK_NIGHT_START * 60 - FALLBACK_TWILIGHT_MINUTES;
  const morningEnd = FALLBACK_NIGHT_END * 60 + FALLBACK_TWILIGHT_MINUTES;
  return (
    (minutes >= eveningStart && minutes < FALLBACK_NIGHT_START * 60) ||
    (minutes >= FALLBACK_NIGHT_END * 60 && minutes < morningEnd)
  );
}

// =========================================================
// Solar Time Computation
// =========================================================

/**
 * Approximate solar noon in local clock hours.
 *
 * The longitude correction accounts for the observer's position
 * within their timezone — a city at the western edge sees solar noon
 * ~30 min later than one at the eastern edge. The equation of time
 * is omitted (~±15 min error), which is acceptable for our purposes.
 */
function solarNoonLocal(now: Date, lon: number): number {
  const tzOffsetHours = -now.getTimezoneOffset() / 60;
  return 12 - lon / 15 + tzOffsetHours;
}

/** Current local time as a fractional hour (e.g. 19.5 = 19:30). */
function currentFractionalHour(now: Date): number {
  return now.getHours() + now.getMinutes() / 60;
}

// =========================================================
// Public API
// =========================================================

/**
 * Returns `true` when it is past civil twilight (properly dark) at
 * the user's approximate location — sun is below −6°.
 *
 * Uses the system timezone to look up coordinates and compute
 * twilight times astronomically. Falls back to a fixed 19:00–06:00
 * window for unrecognized timezones.
 */
export function isNighttime(now: Date = new Date()): boolean {
  const location = getSystemCoordinates();
  if (!location) return isNighttimeFallback(now);

  const [lat, lon] = location;
  const ha = sunAltitudeHourAngle(now, lat, CIVIL_TWILIGHT_ALTITUDE);

  if (ha === null) {
    // Polar case: either always above or always below the threshold.
    return !isSunAlwaysAbove(now, lat, CIVIL_TWILIGHT_ALTITUDE);
  }

  const noon = solarNoonLocal(now, lon);
  const hour = currentFractionalHour(now);
  return hour >= noon + ha || hour < noon - ha;
}

/**
 * Returns `true` during civil twilight — the transition period
 * between daylight and darkness when the sun is between the horizon
 * (−0.833°) and −6° below it. This is when the sky is visibly
 * dimming but not yet fully dark.
 *
 * Falls back to a fixed ~35 minute window around the nighttime
 * boundary for unrecognized timezones.
 */
export function isTwilight(now: Date = new Date()): boolean {
  const location = getSystemCoordinates();
  if (!location) return isTwilightFallback(now);

  const [lat, lon] = location;
  const haSunset = sunAltitudeHourAngle(now, lat, SUNSET_ALTITUDE);
  const haTwilight = sunAltitudeHourAngle(now, lat, CIVIL_TWILIGHT_ALTITUDE);

  // If either threshold is never crossed there is no twilight period.
  if (haSunset === null || haTwilight === null) return false;

  const noon = solarNoonLocal(now, lon);
  const hour = currentFractionalHour(now);

  // Evening twilight: between sunset and civil twilight end.
  const eveningSunset = noon + haSunset;
  const eveningTwilight = noon + haTwilight;
  if (hour >= eveningSunset && hour < eveningTwilight) return true;

  // Morning twilight: between civil twilight start and sunrise.
  const morningSunrise = noon - haSunset;
  const morningTwilight = noon - haTwilight;
  if (hour >= morningTwilight && hour < morningSunrise) return true;

  return false;
}

/**
 * Hook wrapper around {@link isNighttime}.
 */
export function useNighttime(): boolean {
  return isNighttime();
}

/**
 * Hook wrapper around {@link isTwilight}.
 */
export function useTwilight(): boolean {
  return isTwilight();
}
