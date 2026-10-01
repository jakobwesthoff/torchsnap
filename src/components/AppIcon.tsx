// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Torchsnap's app icon at any CSS size and display density.
 *
 * `just asset-app-icons` scales the shipped icon render into a ladder of
 * WebP widths in `src/derived/app-icon/` (ADR 60 for the render). The
 * component offers the whole ladder with width descriptors and tells the
 * webview the size it draws at, so the webview picks the file for the
 * display's pixel ratio: 1x, 2x on Retina, fractional scaling on Windows.
 */

import { cn } from "../lib/cn";
import { appIconImage, type AppIconSource } from "./appIconImage";

// Vite resolves the files at build time. The ladder is generated and not
// tracked, so before `just assets` has run it is empty and the icon shows
// its alt text.
const LADDER: AppIconSource[] = Object.entries(
  import.meta.glob<string>("../derived/app-icon/app-icon-*.webp", {
    eager: true,
    query: "?url",
    import: "default",
  }),
)
  .map(([path, url]) => ({ url, width: Number(/app-icon-(\d+)\.webp$/.exec(path)?.[1]) }))
  .sort((a, b) => a.width - b.width);

export interface AppIconProps {
  /** Width and height in CSS pixels. */
  size: number;
  /** Defaults to the app's name; pass `""` where the icon is decorative. */
  alt?: string;
  className?: string;
}

export function AppIcon({ size, alt = "Torchsnap", className }: AppIconProps) {
  return (
    <img
      {...appIconImage(size, LADDER)}
      width={size}
      height={size}
      alt={alt}
      decoding="async"
      draggable={false}
      className={cn("[-webkit-user-drag:none]", className)}
    />
  );
}
