// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Nighttime detection for mascot condition callbacks.
 *
 * Currently uses a fixed local-time window (19:00–06:00). A future
 * iteration could replace this with a proper sunrise/sunset
 * calculation based on the user's approximate latitude (derived from
 * their IANA timezone), but the fixed window is good enough to keep
 * werewolves from appearing at lunchtime.
 */

// TODO: Replace fixed window with astronomical sunset/sunrise
// calculation using the user's IANA timezone as a latitude proxy.
// The solar position math is similar to the existing lunar phase
// code in `useFullMoonDistance.ts`.

const NIGHT_START_HOUR = 19;
const NIGHT_END_HOUR = 6;

/**
 * Returns `true` when the local time is between 19:00 and 06:00.
 */
export function isNighttime(now: Date = new Date()): boolean {
  const hour = now.getHours();
  return hour >= NIGHT_START_HOUR || hour < NIGHT_END_HOUR;
}

/**
 * Hook wrapper around {@link isNighttime}.
 *
 * Pure arithmetic — trivially cheap to recompute. The returned boolean
 * only changes once per hour boundary, so downstream comparisons
 * remain stable within a single render cycle.
 */
export function useNighttime(): boolean {
  return isNighttime();
}
