// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * The registry of occasion conditions.
 *
 * `occasions.json` names a condition per occasion; this file holds what
 * each name means. The file carries no dates or parameters, so a new kind
 * of timing is a new entry here (ADR 59). The descriptions ship in every
 * build: the draw records which conditions held, and the debug panel
 * shows their descriptions.
 */

import { getFullMoonDistance } from "./fullMoon";
import { isChristmas, isEaster, isHalloween, isNewYear } from "./holidays";
import { isNighttime, isTwilight } from "./nighttime";

export interface Condition {
  /** What the condition checks, in a few words. */
  description: string;
  isActive: (now: Date) => boolean;
}

export const CONDITIONS: Record<string, Condition> = {
  // `getFullMoonDistance` rounds to whole days, so a distance of 1 covers
  // up to 1.5 days on either side. Only after dark, so the werewolf does
  // not show up at noon.
  fullMoonNight: {
    description: "full moon within 1.5 days, dusk to dawn",
    isActive: (now) => getFullMoonDistance(now) <= 1 && (isNighttime(now) || isTwilight(now)),
  },
  newYear: {
    description: "December 31 20:00 to January 1 12:00",
    isActive: (now) => isNewYear(now),
  },
  christmas: { description: "December 20 to 26", isActive: (now) => isChristmas(now) },
  halloween: { description: "October 24 to 31", isActive: (now) => isHalloween(now) },
  easter: { description: "Good Friday to Easter Monday", isActive: (now) => isEaster(now) },
};
