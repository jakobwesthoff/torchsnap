// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Merge two pre-sorted arrays into a single sorted array.
 *
 * Both `a` and `b` must already be sorted according to `compare`.
 * The result preserves the relative order of equal elements from
 * `a` before those from `b` (stable merge).
 *
 * Runs in O(n + m) time where n = a.length and m = b.length.
 */
export function sortedMerge<T>(
  a: readonly T[],
  b: readonly T[],
  compare: (x: T, y: T) => number,
): T[] {
  const result: T[] = new Array(a.length + b.length);
  let i = 0;
  let j = 0;
  let k = 0;

  while (i < a.length && j < b.length) {
    // <= 0 keeps `a` elements first when equal (stable).
    if (compare(a[i], b[j]) <= 0) {
      result[k++] = a[i++];
    } else {
      result[k++] = b[j++];
    }
  }

  while (i < a.length) {
    result[k++] = a[i++];
  }

  while (j < b.length) {
    result[k++] = b[j++];
  }

  return result;
}
