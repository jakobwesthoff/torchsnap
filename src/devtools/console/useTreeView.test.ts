// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { LogItem, LogItemKind } from "../types";
import { useTreeView } from "./useTreeView";

function item(seq: number, kind: LogItemKind): LogItem {
  return { seq, timestamp: seq, source: { type: "host" }, kind };
}

function spanStart(seq: number, spanId: number, parentId: number | null = null): LogItem {
  return item(seq, { type: "spanStart", spanId, name: "search", parentId, depth: 0, metadata: [] });
}

function spanEnd(seq: number, spanId: number, parentId: number | null = null): LogItem {
  return item(seq, {
    type: "spanEnd",
    spanId,
    name: "search",
    parentId,
    depth: 0,
    durationUs: 1200,
    metadata: [],
  });
}

function message(seq: number, spanId: number | null): LogItem {
  return item(seq, { type: "message", level: "info", message: "hi", metadata: [], spanId });
}

describe("useTreeView", () => {
  it("closes a span when its end arrives", () => {
    const start = spanStart(1, 7);
    const end = spanEnd(2, 7);
    const { result } = renderHook(() => useTreeView([start, end]));

    expect(result.current.flatRows).toHaveLength(1);
    const [row] = result.current.flatRows;
    expect(row.kind).toBe("span-header");
    if (row.kind !== "span-header") return;
    expect(row.node.spanEnd).toBe(end);
    expect(row.node.inProgress).toBe(false);
  });

  it("nests messages and child spans under their open parent", () => {
    const items = [spanStart(1, 7), message(2, 7), spanStart(3, 8, 7), spanEnd(4, 8, 7)];
    const { result } = renderHook(() => useTreeView(items));

    expect(result.current.flatRows.map((r) => [r.kind, r.depth])).toEqual([
      ["span-header", 0],
      ["item", 1],
      ["span-header", 1],
    ]);
  });

  it("puts a message at the root when its span is not in the window", () => {
    const orphan = message(1, 99);
    const { result } = renderHook(() => useTreeView([orphan]));

    expect(result.current.flatRows).toEqual([{ kind: "item", item: orphan, depth: 0 }]);
  });

  // The ring buffer evicts a long span's start before its end, so the
  // end can arrive alone. It carries the span's duration and must stay
  // visible, as it does in the flat view.
  it("puts a span end at the root when its start is not in the window", () => {
    const orphan = spanEnd(5, 99);
    const { result } = renderHook(() => useTreeView([message(4, null), orphan]));

    expect(result.current.flatRows).toEqual([
      { kind: "item", item: message(4, null), depth: 0 },
      { kind: "item", item: orphan, depth: 0 },
    ]);
  });
});
