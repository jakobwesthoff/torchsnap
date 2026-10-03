// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import type { SourcedEntry } from "../types";

/**
 * Deterministic sort comparator: score DESC, source ASC, id ASC.
 * Strings compare in Unicode code point order.
 *
 * This is the single source of truth on the TypeScript side.
 * The Rust backend has an equivalent method `SourcedEntry::cmp_sort_key`
 * in `src-tauri/src/commands/types.rs` that MUST stay in sync with this
 * implementation. Any change here requires a matching change there
 * (and vice versa).
 */
export function compareEntries(a: SourcedEntry, b: SourcedEntry): number {
  if (b.score !== a.score) return b.score - a.score;
  return compareCodePoints(a.source, b.source) || compareCodePoints(a.id, b.id);
}

/**
 * Compare two strings in code point order, which is the order of Rust's
 * `str::cmp`. JavaScript's `<` compares UTF-16 code units instead, and
 * puts a supplementary character (first unit a surrogate, 0xD800-0xDFFF)
 * before a BMP character from U+E000 up.
 *
 * Only the first differing unit decides. When both are surrogates their
 * unit order already is code point order. Otherwise surrogates move above
 * 0xE000-0xFFFF, which shifts down to make room.
 */
function compareCodePoints(a: string, b: string): number {
  const length = Math.min(a.length, b.length);
  for (let i = 0; i < length; i++) {
    const x = a.charCodeAt(i);
    const y = b.charCodeAt(i);
    if (x !== y) return codePointOrderKey(x) - codePointOrderKey(y);
  }
  return a.length - b.length;
}

function codePointOrderKey(unit: number): number {
  if (unit < 0xd800) return unit;
  return unit >= 0xe000 ? unit - 0x800 : unit + 0x2000;
}
