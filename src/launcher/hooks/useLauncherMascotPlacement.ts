// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Computes the CSS placement values for the launcher mascot based on
 * the variant's trim data. Also provides the horizontal center point
 * of the mascot image (relative to the card's right edge) so that
 * other UI elements like the Escape hint can align with it.
 */

import { useMemo } from "react";
import { getMascotTrim } from "../../mascotVariants";
import { CENTER_FEET_Y, SIDEKICK_FEET_Y, SIDEKICK_EDGE_X } from "../layout";

interface MascotPlacement {
  /** CSS `top` value for the mascot element (px). Always negative. */
  top: number;
  /** CSS `right` value for the mascot element (px). Sidekick only. */
  right?: number;
  /** Distance from the card's right edge to the mascot's horizontal
   *  center (px). Sidekick only — used to align the Escape hint pill. */
  centerX?: number;
}

export function useLauncherMascotPlacement(
  variant: string,
  mode: "center" | "sidekick" | "off",
): MascotPlacement | null {
  return useMemo(() => {
    if (mode === "off") return null;

    const size = mode === "sidekick" ? 96 : 192;
    const trim = getMascotTrim(variant);

    // Position the image so the feet (bottom of visible content)
    // land at the target Y position regardless of bottom trim.
    const feetY = mode === "sidekick" ? SIDEKICK_FEET_Y : CENTER_FEET_Y;
    const top = feetY - (size * (100 - trim.bottom)) / 100;

    if (mode === "sidekick") {
      // The sidekick is flipped (-scale-x-100), so the visual right
      // edge corresponds to source trimLeft. Position the image so
      // the visual right edge lands at the target X position.
      const right = SIDEKICK_EDGE_X - (size * trim.left) / 100;

      // The image center is half the image width inward from the
      // right edge of the positioned element.
      const centerX = right + size / 2;

      return { top, right, centerX };
    }

    return { top };
  }, [variant, mode]);
}
