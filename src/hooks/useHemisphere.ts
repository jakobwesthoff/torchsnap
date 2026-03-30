// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Hemisphere detection based on the user's IANA timezone.
 *
 * Maps known southern hemisphere timezone prefixes to determine the
 * user's hemisphere. Falls back to northern hemisphere when the
 * timezone cannot be determined.
 */

export type Hemisphere = "northern" | "southern";

/**
 * Known IANA timezone prefixes for southern hemisphere regions.
 * Covers the major populated areas; edge cases (equatorial zones)
 * default to northern hemisphere, which is the larger user base.
 */
const SOUTHERN_PREFIXES = [
  "Australia/",
  "Antarctica/",
  "Pacific/Auckland",
  "Pacific/Fiji",
  "Pacific/Tongatapu",
  "America/Buenos_Aires",
  "America/Argentina/",
  "America/Sao_Paulo",
  "America/Santiago",
  "America/Montevideo",
  "America/Asuncion",
  "America/Lima",
  "Africa/Johannesburg",
  "Africa/Harare",
  "Africa/Maputo",
  "Africa/Windhoek",
  "Indian/Antananarivo",
];

export function getHemisphere(): Hemisphere {
  try {
    const tz = Intl.DateTimeFormat().resolvedOptions().timeZone;
    if (SOUTHERN_PREFIXES.some((prefix) => tz.startsWith(prefix))) {
      return "southern";
    }
  } catch {
    // Intl not available — fall back to northern.
  }

  return "northern";
}

/**
 * Hook wrapper around {@link getHemisphere}.
 */
export function useHemisphere(): Hemisphere {
  return getHemisphere();
}
