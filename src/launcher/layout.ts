// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Launcher window layout constants.
 *
 * Only values that cannot be derived from DOM measurement live here:
 * the card's box-shadow and the clearance it needs (a shadow is
 * invisible to `getBoundingClientRect`), and the headroom for the
 * absolutely-positioned mascot. The card's width, search bar, footer
 * and content height are measured from the real rendered DOM via the
 * `measureDummy` / `onMeasure` mechanism in `Launcher.tsx`, and
 * {@link launcherWindowLayout} turns that measurement into the window
 * size. See ADR 0020.
 */

import type { CommandMap } from "../lib/command";

/**
 * Clearance beside and below the card for CSS box-shadow bleed (px).
 *
 * The card's largest shadow is `0 16px 48px`. Its alpha drops below
 * one 8-bit step about two standard deviations (48px) past its edge,
 * which the y-offset (16px) moves down: about 47px beside the card
 * and 63px below it. The pad also holds the mirrored sidekick, which
 * overhangs the card's right edge by up to about 20px. Above the card
 * the mascot headroom holds the shadow (see {@link CARD_TOP_OFFSET}).
 * See ADR 0071.
 */
export const SHADOW_PADDING = 64;

/** The card's box-shadow layers, joined with ", " into its `boxShadow`. */
export const CARD_SHADOW: readonly string[] = [
  "0 0 0 1px var(--color-border)",
  "inset 0 1px 0 0 rgba(255,255,255,0.06)",
  "0 4px 16px rgba(0,0,0,0.12)",
  "0 16px 48px rgba(0,0,0,0.16)",
];

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

/**
 * Offset from window top edge to card top edge.
 *
 * The headroom alone: every shadow layer ends within 56px above the
 * card (three standard deviations of `0 16px 48px`, less its 16px
 * y-offset), inside the headroom, so no shadow pad goes on top.
 */
export const CARD_TOP_OFFSET = MASCOT_HEADROOM;

// =========================================================
// Window Size
// =========================================================

/**
 * The launcher window's size and the card's offset in it, for a card
 * measured at its largest: the mascot headroom above the card, the
 * shadow pad beside and below it. This is what `launcher_set_layout`
 * sends to the backend, which places and sizes the window from it on
 * every show.
 */
export function launcherWindowLayout(
  cardWidth: number,
  cardHeight: number,
): CommandMap["launcher_set_layout"]["params"] {
  return {
    windowWidth: cardWidth + 2 * SHADOW_PADDING,
    windowHeight: CARD_TOP_OFFSET + cardHeight + SHADOW_PADDING,
    cardTopOffset: CARD_TOP_OFFSET,
  };
}
