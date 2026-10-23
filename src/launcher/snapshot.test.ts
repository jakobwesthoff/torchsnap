// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { describe, expect, it } from "vitest";
import type { SourcedEntry } from "../types";
import { buildLauncherSnapshot, type SnapshotParts } from "./snapshot";

function entry(id: string, title: string): SourcedEntry {
  return {
    id,
    title,
    subtitle: id === "firefox" ? "/Applications/Firefox.app" : null,
    icon: null,
    score: 1,
    titlePositions: [0],
    subtitlePositions: [],
    source: "app-launcher",
    actions: [{ slot: "primary", label: "Open" }],
  };
}

const FIREFOX = entry("firefox", "Firefox");
const FINDER = entry("finder", "Finder");

function parts(overrides: Partial<SnapshotParts> = {}): SnapshotParts {
  return {
    displayQuery: "fi",
    searching: false,
    results: [FIREFOX, FINDER],
    listSelectedIndex: 1,
    inlineSelected: false,
    gadgetView: null,
    inlineView: null,
    contentVisible: true,
    inputFocused: true,
    ...overrides,
  };
}

describe("buildLauncherSnapshot", () => {
  it("lists the results by source, id, title and subtitle only", () => {
    expect(buildLauncherSnapshot(parts()).results).toEqual([
      {
        source: "app-launcher",
        id: "firefox",
        title: "Firefox",
        subtitle: "/Applications/Firefox.app",
      },
      { source: "app-launcher", id: "finder", title: "Finder", subtitle: null },
    ]);
  });

  it("reports the selected row of the list", () => {
    expect(buildLauncherSnapshot(parts()).selectedIndex).toBe(1);
  });

  it("selects nothing in an empty list", () => {
    expect(
      buildLauncherSnapshot(parts({ results: [], listSelectedIndex: 0 })).selectedIndex,
    ).toBeNull();
  });

  it("selects no row while the inline view holds the selection", () => {
    const snapshot = buildLauncherSnapshot(
      parts({
        inlineView: { gadgetId: "calculator", view: "Result" },
        inlineSelected: true,
        listSelectedIndex: -1,
      }),
    );
    expect(snapshot.selectedIndex).toBeNull();
    expect(snapshot.inlineViewSelected).toBe(true);
  });

  it("selects nothing when the index is past the end of the list", () => {
    expect(buildLauncherSnapshot(parts({ listSelectedIndex: 2 })).selectedIndex).toBeNull();
  });

  it("shows no list while a gadget view replaces it", () => {
    const snapshot = buildLauncherSnapshot(
      parts({ gadgetView: { gadgetId: "emoji-picker", view: "Grid", data: { columns: 8 } } }),
    );
    expect(snapshot.results).toEqual([]);
    expect(snapshot.selectedIndex).toBeNull();
    expect(snapshot.gadgetView).toEqual({ gadgetId: "emoji-picker", view: "Grid" });
  });

  it("names the inline view without its data", () => {
    const snapshot = buildLauncherSnapshot(
      parts({ inlineView: { gadgetId: "calculator", view: "Result", data: { value: 42 } } }),
    );
    expect(snapshot.inlineView).toEqual({ gadgetId: "calculator", view: "Result" });
    expect(snapshot.inlineViewSelected).toBe(false);
  });

  it("reports the input text, which a gadget can change without a search", () => {
    expect(buildLauncherSnapshot(parts({ displayQuery: "= 6 * 7" })).query).toBe("= 6 * 7");
  });

  it("passes searching, content visibility and input focus through", () => {
    const snapshot = buildLauncherSnapshot(
      parts({ searching: true, contentVisible: false, inputFocused: false }),
    );
    expect(snapshot.searching).toBe(true);
    expect(snapshot.contentVisible).toBe(false);
    expect(snapshot.inputFocused).toBe(false);
  });

  it("reports an input that has focus", () => {
    const snapshot = buildLauncherSnapshot(parts());
    expect(snapshot.inputFocused).toBe(true);
    expect(snapshot.contentVisible).toBe(true);
  });
});
