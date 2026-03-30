// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Utilities for warming the browser's image cache with Snappy mascot images
 * before they are rendered. Uses `new Image()` which triggers a normal fetch
 * at default priority — enough to avoid a first-paint flicker without
 * interfering with higher-priority resources.
 *
 * The mascot URL scheme is:
 *   /images/mascot/snappy-{variant}-{size}.webp
 *
 * Currently used logical sizes: 96, 192, 384.
 */

const MASCOT_BASE = `${import.meta.env.BASE_URL}images/mascot/snappy`;

// Keep preloading images alive until they settle. Without a strong reference
// the browser may GC the Image object and abort the in-flight fetch
// (NS_BINDING_ABORTED).
const inflight = new Set<HTMLImageElement>();

/**
 * Warm the cache for a single mascot variant at the given logical size.
 * Only the image that the browser will actually use is fetched — the 2x file
 * on retina displays (devicePixelRatio ≥ 2), the 1x file everywhere else.
 * Preloads the WebP variant which the `<picture>` element will select.
 */
export function preloadMascotImage(variant: string, size: number): void {
  const physicalSize = window.devicePixelRatio >= 2 ? size * 2 : size;
  const img = new Image();
  inflight.add(img);
  img.onload = img.onerror = () => inflight.delete(img);
  img.src = `${MASCOT_BASE}-${variant}-${physicalSize}.webp`;
}

/**
 * Warm the cache for a single variant at several sizes.
 * Use this when the same variant appears at different sizes across views.
 */
export function preloadMascotVariant(variant: string, sizes: number[]): void {
  for (const size of sizes) {
    preloadMascotImage(variant, size);
  }
}

/**
 * Warm the cache for multiple variants at the same logical size.
 * Use this for sets of mascots that can appear without warning.
 */
export function preloadMascotVariants(variants: string[], size: number): void {
  for (const variant of variants) {
    preloadMascotImage(variant, size);
  }
}
