// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

export interface AppIconSource {
  url: string;
  /** Pixel width of the file; the icon is square. */
  width: number;
}

/**
 * The `<img>` sources for an icon `size` CSS pixels wide, from `sources`
 * in ascending width. `srcSet` and `sizes` let the webview pick the file
 * for the display's pixel ratio. `src` only matters to a webview without
 * `srcset` support; it takes the smallest file that covers the size at 1x.
 */
export function appIconImage(size: number, sources: AppIconSource[]) {
  const fallback = sources.find((source) => source.width >= size) ?? sources[sources.length - 1];
  return {
    src: fallback?.url,
    srcSet: sources.map((source) => `${source.url} ${source.width}w`).join(", "),
    sizes: `${size}px`,
  };
}
