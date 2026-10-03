// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { describe, expect, it } from "vitest";
import type { SourcedEntry } from "../types";
import { compareEntries } from "./compareEntries";

function entry(fields: Partial<SourcedEntry>): SourcedEntry {
  return {
    id: "id",
    title: "title",
    subtitle: null,
    icon: null,
    score: 0,
    titlePositions: [],
    subtitlePositions: [],
    source: "source",
    actions: [],
    ...fields,
  };
}

function sortedIds(ids: string[]): string[] {
  return ids
    .map((id) => entry({ id }))
    .sort(compareEntries)
    .map((e) => e.id);
}

describe("compareEntries", () => {
  it("sorts by score descending, then source and id ascending", () => {
    const entries = [
      entry({ score: 1, source: "b", id: "x" }),
      entry({ score: 2, source: "z", id: "z" }),
      entry({ score: 1, source: "a", id: "y" }),
      entry({ score: 1, source: "a", id: "x" }),
    ];
    expect(entries.sort(compareEntries).map((e) => [e.score, e.source, e.id])).toEqual([
      [2, "z", "z"],
      [1, "a", "x"],
      [1, "a", "y"],
      [1, "b", "x"],
    ]);
  });

  it("returns 0 for equal keys", () => {
    expect(compareEntries(entry({}), entry({}))).toBe(0);
  });

  it("sorts a prefix before the longer string", () => {
    expect(sortedIds(["abc", "ab"])).toEqual(["ab", "abc"]);
  });

  // Rust's `cmp_sort_key` compares in code point order. UTF-16 unit
  // order differs from it where a BMP character at U+E000 or above meets
  // a supplementary one, whose first unit is a surrogate (0xD800-0xDFFF).
  // The emoji-picker uses emoji as entry ids. The same fixtures are in
  // `cmp_sort_key_orders_ids_by_code_point` in
  // `src-tauri/src/commands/types.rs`.
  it("orders ids by code point, as the Rust comparator does", () => {
    expect(sortedIds(["\u{1F600}", "\u{FDFD}"])).toEqual(["\u{FDFD}", "\u{1F600}"]);
    expect(sortedIds(["a\u{1F600}", "a\u{E000}", "a\u{D7FF}"])).toEqual([
      "a\u{D7FF}",
      "a\u{E000}",
      "a\u{1F600}",
    ]);
    expect(sortedIds(["\u{1F601}", "\u{1F600}"])).toEqual(["\u{1F600}", "\u{1F601}"]);
  });

  it("orders sources by code point", () => {
    const entries = [entry({ source: "\u{1F600}" }), entry({ source: "\u{FDFD}" })];
    expect(entries.sort(compareEntries).map((e) => e.source)).toEqual(["\u{FDFD}", "\u{1F600}"]);
  });
});
