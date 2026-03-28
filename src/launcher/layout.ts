// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Launcher window layout constants.
 *
 * Only values that cannot be derived from DOM measurement live here:
 * box-shadow bleed (invisible to `getBoundingClientRect`) and the
 * absolutely-positioned mascot headroom. Everything else — card
 * width, search bar height, footer height, content area max height —
 * is measured from the real rendered DOM via the `measureDummy` /
 * `onMeasure` mechanism in `Launcher.tsx`. See ADR 0020.
 */

/**
 * Clearance around the card for CSS box-shadow bleed (px).
 *
 * The card's largest shadow is `0 16px 48px`. The blur radius (48px)
 * sets how far the shadow extends; adding the y-offset (16px) gives
 * 64px on the bottom edge. We use 64px uniformly on all sides.
 */
export const SHADOW_PADDING = 64;

/**
 * Clearance above the card for the decorative mascot image (px).
 *
 * The center-mode mascot is 192px tall and absolutely positioned at
 * `top: -156px` relative to the card, needing ~160px of headroom.
 */
export const MASCOT_HEADROOM = 160;

/** Offset from window top edge to card top edge. */
export const CARD_TOP_OFFSET = SHADOW_PADDING + MASCOT_HEADROOM;
