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

// =========================================================
// Mascot Overlap Constants
//
// Absolute pixel positions where the original mascot's visible
// content edges land in the launcher layout. Derived once from
// the original hardcoded positions and the original's trim data
// (bottom: 8.7%, left: 8.5%):
//
//   Center Y:   -156 + 192 × (1 - 0.087) = 19.3px below card top
//   Sidekick Y: -72  +  96 × (1 - 0.087) = 15.6px below card top
//   Sidekick X: -10  +  96 × 0.085        = -1.8px from card right
//
// Every variant uses these same targets with its own trim data
// (see LauncherMascot.tsx).
// =========================================================

/** How far the center-mode mascot's feet extend below the card top edge (px). */
export const CENTER_FEET_Y = 10.1;

/** How far the sidekick-mode mascot's feet extend below the card top edge (px). */
export const SIDEKICK_FEET_Y = 11.0;

/** Visual right edge position of the sidekick mascot relative to the card right edge (px). More positive = further left. */
export const SIDEKICK_EDGE_X = 8;

/**
 * Clearance above the card for the decorative mascot image (px).
 *
 * The center-mode mascot is 192px tall. Its `top` position is computed
 * dynamically from per-variant trim data so the feet always land at
 * {@link CENTER_FEET_Y} pixels below card top. The worst case is a
 * variant with zero bottom trim: `192 - CENTER_FEET_Y`.
 */
export const MASCOT_HEADROOM = 192 - CENTER_FEET_Y;

/** Offset from window top edge to card top edge. */
export const CARD_TOP_OFFSET = SHADOW_PADDING + MASCOT_HEADROOM;
