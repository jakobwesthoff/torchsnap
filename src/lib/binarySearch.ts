// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Find the index of `target` in a sorted array using binary search.
 *
 * `compare(element, target)` must return:
 *   - negative if element comes before target
 *   - 0 if element matches target
 *   - positive if element comes after target
 *
 * Returns the index of the matching element, or -1 if not found.
 * If multiple elements compare equal, the returned index is
 * unspecified among them.
 */
export function binarySearch<T, U>(
  array: readonly T[],
  target: U,
  compare: (element: T, target: U) => number,
): number {
  let lo = 0;
  let hi = array.length - 1;

  while (lo <= hi) {
    const mid = (lo + hi) >>> 1;
    const cmp = compare(array[mid], target);

    if (cmp < 0) {
      lo = mid + 1;
    } else if (cmp > 0) {
      hi = mid - 1;
    } else {
      return mid;
    }
  }

  return -1;
}
