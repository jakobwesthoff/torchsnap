// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import type { SourcedEntry } from "../types";

/**
 * Deterministic sort comparator: score DESC, source ASC, id ASC.
 *
 * This is the single source of truth on the TypeScript side.
 * The Rust backend has an equivalent method `SourcedEntry::cmp_sort_key`
 * in `src-tauri/src/search/types.rs` that MUST stay in sync with this
 * implementation. Any change here requires a matching change there
 * (and vice versa).
 */
export function compareEntries(a: SourcedEntry, b: SourcedEntry): number {
  if (b.score !== a.score) return b.score - a.score;
  if (a.source < b.source) return -1;
  if (a.source > b.source) return 1;
  if (a.id < b.id) return -1;
  if (a.id > b.id) return 1;
  return 0;
}
