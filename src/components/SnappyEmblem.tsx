// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * The Snappy emblem in a choice of framing and tone.
 *
 * Framing: `rising` shows the owl large and cut at the bottom edge, as
 * in the app icon (`aurora-rise`) but without its tile and sky; `full`
 * shows the whole owl.
 *
 * Tone: `colour` is the emblem as drawn. `ink` is the owl in one navy
 * ink, and in dark mode its negative; `negative` is that pale negative
 * alone. These are the one-ink styles of `tools/build-app-icons`.
 *
 * The emblem file stays as drawn; an outer `<svg>` frames it and SVG
 * filters give the one-ink tones. The emblem is a vector, so it stays
 * sharp at any display density.
 */

import { useId } from "react";
import emblemUrl from "../../assets/snappy-emblem-feathered.svg?url";
import { cn } from "../lib/cn";

// =========================================================
// Framing
// =========================================================

// The `viewBox` of the outer `<svg>` is a window onto the emblem's own
// coordinates: the emblem is drawn at 0,0 in its 1024 x 1024 viewBox,
// and the window picks the part to show. Whatever lies outside the
// window is clipped, which makes the rising framing's cut at the bottom.
// The window's aspect ratio sets the component's height.

// The owl fills this box in the emblem's viewBox, from the ear tips at
// the top to the feet at the bottom.
const OWL = { x: 18, y: 55, width: 988, height: 892 };

// Two proportions taken from the app icon, so the rising owl looks the
// same there and here (`RISE_OWL_WIDTH` and `RISE_CENTRE_Y` in
// `tools/build-app-icons`): the owl is 780 px wide on the 824 px icon
// tile, and the tile's bottom edge cuts it at 74 % of its height, just
// below the beak.
const OWL_SHARE_OF_WIDTH = 780 / 824;
const CUT_AT = 0.74;

// The margin left and right of the owl that its share of the width
// leaves. The full framing keeps it on all four sides.
const MARGIN = (OWL.width / OWL_SHARE_OF_WIDTH - OWL.width) / 2;

// The icon leaves the upper third of its tile to the sky. The rising
// framing keeps only a little room above the ears, as a share of the
// owl's height.
const HEADROOM = 0.12;

export type SnappyEmblemFraming = "rising" | "full";

interface Frame {
  x: number;
  y: number;
  width: number;
  height: number;
}

const FRAMES: Record<SnappyEmblemFraming, Frame> = {
  // From `HEADROOM` above the ear tips down to the cut.
  rising: {
    x: OWL.x - MARGIN,
    y: OWL.y - HEADROOM * OWL.height,
    width: OWL.width + 2 * MARGIN,
    height: (HEADROOM + CUT_AT) * OWL.height,
  },
  full: {
    x: OWL.x - MARGIN,
    y: OWL.y - MARGIN,
    width: OWL.width + 2 * MARGIN,
    height: OWL.height + 2 * MARGIN,
  },
};

// =========================================================
// Tone
// =========================================================

// `tools/build-app-icons` makes the one-ink owls by giving every colour
// of the emblem a single ink at an opacity from the colour's luminance
// L, linear in it and clamped to 0..1. A `feColorMatrix` computes the
// same per pixel: its first three rows set the ink, the fourth the
// opacity. The filter works in sRGB as the tool does; the browser
// default, linear RGB, would shift every L.
const LUMINANCE = [0.2126, 0.7152, 0.0722];

interface Ink {
  rgb: [number, number, number];
  /** Opacity = `scale` * L + `offset`. */
  scale: number;
  offset: number;
}

// Navy: dark tones print solid, the cream face prints as paper,
// opacity = (1 - L - 0.06) / 0.7.
const NAVY_INK: Ink = { rgb: [0x17, 0x22, 0x3b], scale: -1 / 0.7, offset: 0.94 / 0.7 };
// Pale: the light tones in pale ink, the dark ones left out,
// opacity = (L - 0.2) / 0.65.
const PALE_INK: Ink = { rgb: [0xf3, 0xef, 0xe2], scale: 1 / 0.65, offset: -0.2 / 0.65 };

function inkMatrix({ rgb, scale, offset }: Ink) {
  const [r, g, b] = rgb.map((channel) => channel / 255);
  const [lr, lg, lb] = LUMINANCE.map((weight) => weight * scale);
  return [
    [0, 0, 0, 0, r],
    [0, 0, 0, 0, g],
    [0, 0, 0, 0, b],
    [lr, lg, lb, 0, offset],
  ]
    .map((row) => row.join(" "))
    .join("  ");
}

export type SnappyEmblemTone = "colour" | "ink" | "negative";

// =========================================================
// The component
// =========================================================

export interface SnappyEmblemProps {
  /** Width in CSS pixels; the height follows from the framing. */
  width: number;
  framing?: SnappyEmblemFraming;
  tone?: SnappyEmblemTone;
  /** Pass `""` where the emblem is decorative. */
  alt?: string;
  className?: string;
}

export function SnappyEmblem({
  width,
  framing = "rising",
  tone = "colour",
  alt = "Snappy",
  className,
}: SnappyEmblemProps) {
  // Filter ids are document-wide, so each instance gets its own. React's
  // ids carry characters that `url(#…)` references do not take.
  const id = `snappy-emblem-${useId().replace(/[^A-Za-z0-9_-]/g, "")}`;
  const frame = FRAMES[framing];
  const label = alt ? { role: "img", "aria-label": alt } : { "aria-hidden": true };

  const inkFilter = (filterId: string, ink: Ink) => (
    <filter id={filterId} colorInterpolationFilters="sRGB">
      <feColorMatrix type="matrix" values={inkMatrix(ink)} />
      {/* The matrix makes the transparent background opaque ink (L = 0
          there); taking the emblem's own alpha keeps it transparent. */}
      <feComposite in2="SourceGraphic" operator="in" />
    </filter>
  );
  const image = (props: { filter?: string; className?: string }) => (
    <image href={emblemUrl} width="1024" height="1024" {...props} />
  );

  return (
    <svg
      viewBox={`${frame.x} ${frame.y} ${frame.width} ${frame.height}`}
      width={width}
      height={(width * frame.height) / frame.width}
      {...label}
      className={cn(
        // The colour emblem's outline is near black and vanishes on the
        // dark surface; a faint light rim keeps the ears and head readable.
        tone === "colour" && "dark:drop-shadow-[0_0_3px_rgb(255_255_255/0.5)]",
        className,
      )}
    >
      {tone === "colour" && image({})}
      {tone === "ink" && (
        <>
          <defs>
            {inkFilter(`${id}-navy`, NAVY_INK)}
            {inkFilter(`${id}-pale`, PALE_INK)}
          </defs>
          {image({ filter: `url(#${id}-navy)`, className: "dark:hidden" })}
          {image({ filter: `url(#${id}-pale)`, className: "hidden dark:inline" })}
        </>
      )}
      {tone === "negative" && (
        <>
          <defs>{inkFilter(`${id}-pale`, PALE_INK)}</defs>
          {image({ filter: `url(#${id}-pale)` })}
        </>
      )}
    </svg>
  );
}
