// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Computes the CSS placement values for the launcher mascot from the
 * variant's ground anchor (see `placement.ts`), including the anchor's
 * distance from the card's right edge in sidekick mode, which the Escape
 * pill aligns with.
 */

import { useMemo } from "react";
import { getMascotAnchor } from "../../mascotVariants";
import { placeMascot, type MascotPlacement } from "../placement";

export function useLauncherMascotPlacement(
  variant: string,
  mode: "center" | "sidekick" | "off",
): MascotPlacement | null {
  return useMemo(() => {
    if (mode === "off") return null;
    const { groundAnchor, boxLeft } = getMascotAnchor(variant);
    return placeMascot(mode, groundAnchor, boxLeft);
  }, [variant, mode]);
}
