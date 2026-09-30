// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Snappy mascot image component with retina display support.
 *
 * Serves WebP images with automatic 2x srcSet for high-DPI displays.
 * All Tauri target platforms (WebKit on macOS, WebView2 on Windows)
 * support WebP natively, so no PNG fallback is needed.
 *
 * The component is purely decorative — it carries no semantic meaning
 * and uses `alt=""`.
 */

import { cn } from "../lib/cn";

export interface MascotProps {
  /** Variant of the mascot to display (e.g., `'original'`). Defaults to `'original'`. */
  variant?: string;
  /** Logical pixel size. The component automatically serves a 2x file on retina displays. */
  size?: number;
  /**
   * CSS classes applied to the `<img>` element.
   * Use this for positioning, transforms, and z-index — e.g.:
   * `"absolute -top-[156px] left-1/2 -translate-x-1/2 z-10 pointer-events-none"`
   */
  className?: string;
}

/**
 * Displays the Snappy mascot with proper retina support.
 *
 * The mascot is treated as decorative: drag interactions and long-press
 * context menus are suppressed, and `alt` is left empty.
 */
export function Mascot({ variant = "original", size = 192, className }: MascotProps) {
  const basePath = `${import.meta.env.BASE_URL}images/mascot/snappy-${variant}`;
  const srcSet = `${basePath}-${size}.webp 1x, ${basePath}-${size * 2}.webp 2x`;

  return (
    <img
      src={`${basePath}-${size}.webp`}
      srcSet={srcSet}
      width={size}
      height={size}
      alt=""
      loading="eager"
      decoding="async"
      draggable={false}
      className={cn(
        // Suppress native long-press "Save Image" sheet (iOS) and
        // image drag ghost. These interfere with the launcher's own
        // click-to-dismiss handler.
        "[-webkit-touch-callout:none] [-webkit-user-drag:none]",
        className,
      )}
    />
  );
}
