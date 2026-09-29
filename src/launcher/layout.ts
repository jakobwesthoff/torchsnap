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

/** Width of the launcher card (px); `w-[680px]` in `Launcher.tsx`. */
export const CARD_WIDTH = 680;

// =========================================================
// Mascot Placement Constants
//
// Every mascot is placed by its ground anchor (see placement.ts):
// the anchor's y goes on the feet line below the card's top edge,
// its x on the card centre or, as sidekick, at a fixed inset from
// the card's right edge. The values were approved on renders of
// the whole mascot set (2026-09).
// =========================================================

/** Rendered mascot size per mode (px). */
export const MASCOT_SIZE = { center: 192, sidekick: 96 } as const;

/** How far the center-mode mascot's feet extend below the card top edge (px). */
export const CENTER_FEET_Y = 10.1;

/** How far the sidekick-mode mascot's feet extend below the card top edge (px). */
export const SIDEKICK_FEET_Y = 11.0;

/**
 * Distance of the sidekick's anchor from the card's right edge (px).
 * The 90th percentile of the set's anchor-to-visible-edge distance at
 * 96 px, so only the widest tenth of the mascots needs the edge clamp
 * in `placeMascot`.
 */
export const SIDEKICK_ANCHOR_INSET = 48;

/**
 * Clearance above the card for the decorative mascot image (px).
 *
 * The center-mode mascot is 192px tall and its anchor sits
 * {@link CENTER_FEET_Y} pixels below card top. The worst case is an
 * anchor on the image's bottom edge: `192 - CENTER_FEET_Y`.
 */
export const MASCOT_HEADROOM = MASCOT_SIZE.center - CENTER_FEET_Y;

/** Offset from window top edge to card top edge. */
export const CARD_TOP_OFFSET = SHADOW_PADDING + MASCOT_HEADROOM;
