// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Launcher mascot placement from a per-mascot anchor point.
 *
 * `groundAnchor` is the point of the mascot that lines up with the card,
 * as fractions of the square image with the origin top left (from
 * `src/derived/mascots.json`). Its `y` goes on the feet line just below
 * the card's top edge. Its `x` goes on the card centre in centre mode,
 * and a fixed inset from the card's right edge in sidekick mode, where
 * the Escape pill sits under it.
 *
 * The renders the anchors were approved on used this function, so what
 * the review approved is what the launcher draws.
 */

import {
  CARD_WIDTH,
  CENTER_FEET_Y,
  MASCOT_SIZE,
  SIDEKICK_ANCHOR_INSET,
  SIDEKICK_FEET_Y,
} from "./layout";

export interface MascotAnchor {
  x: number;
  y: number;
}

export interface MascotPlacement {
  /** CSS `top` of the image relative to the card's top edge (px). */
  top: number;
  /** Centre mode: CSS `left` of the image relative to the card (px). */
  left?: number;
  /** Sidekick mode: CSS `right` of the image relative to the card (px). */
  right?: number;
  /** Sidekick mode: distance from the card's right edge to the anchor
   *  (px), for centring the Escape pill under it. */
  anchorFromRight?: number;
}

/**
 * @param boxLeft - left edge of the visible figure, as a fraction of the
 *   image. The sidekick is mirrored, so this edge ends up on the right.
 */
export function placeMascot(
  mode: "center" | "sidekick",
  groundAnchor: MascotAnchor,
  boxLeft: number,
): MascotPlacement {
  const size = MASCOT_SIZE[mode];

  if (mode === "center") {
    return {
      top: CENTER_FEET_Y - size * groundAnchor.y,
      left: CARD_WIDTH / 2 - size * groundAnchor.x,
    };
  }

  // Mirrored with `-scale-x-100`, a source x lies size * x from the
  // image's right edge on screen, so `right = inset - size * x` puts the
  // anchor at the inset. The visible figure's mirrored edge must not pass
  // the card edge: for wide figures the mascot moves left until that
  // edge sits on the card edge.
  const right = Math.max(SIDEKICK_ANCHOR_INSET - size * groundAnchor.x, -size * boxLeft);
  return {
    top: SIDEKICK_FEET_Y - size * groundAnchor.y,
    right,
    anchorFromRight: right + size * groundAnchor.x,
  };
}
