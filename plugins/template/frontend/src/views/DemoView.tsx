// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// DemoView — Template Plugin Launcher View
//
// Custom UI component rendered when the user types the "tpl:"
// prefix. Showcases the typical plugin authoring surface so
// new plugin authors can copy-and-adapt:
//
// - JSX with the automatic runtime (no `import React` needed)
// - React hooks via the SDK
// - Per-render data (`data`, `query`) from PluginViewProps
// - Launcher actions via the `useLauncher()` context hook
// - Component-lifecycle keybinding registration via
//   `useKeyBindings` and the `LAYER` constants
// - Windowed list rendering via `useWindowedList`
// - Tailwind utilities using the host's design tokens
//
// This file is meant to be **read** by new plugin authors and
// adapted into real plugin code. It is intentionally kept
// simple and pedagogical. End-to-end test coverage of the SDK
// shim machinery lives in the dedicated `plugins/test-fixture/`
// crate, not here.
// =========================================================

import { useState } from "react";
import type { PluginViewProps } from "@torchsnap/plugin-sdk";
import { useLauncher, useWindowedList } from "@torchsnap/plugin-sdk/hooks";
import {
  LAYER,
  useKeyBindings,
} from "@torchsnap/plugin-sdk/keybindings";
import "../../styles/launcher.css";

// Sample dataset for the windowed list. Real plugins source
// this from `data` (host-provided per-render payload) or from
// `sendMessage(...)` round-trips into their backend.
const DEMO_ITEMS = Array.from({ length: 32 }, (_, i) => `Item #${i + 1}`);
const PAGE_SIZE = 6;

export function DemoView({ data, query }: PluginViewProps) {
  const { dismiss } = useLauncher();
  const [count, setCount] = useState(0);
  const [selectedIndex, setSelectedIndex] = useState(0);

  const echoText = (data as { query?: string })?.query ?? query ?? "";

  // Windowed list — exposes a `wheelRef` to attach to the
  // scroll container plus a `windowStart` index to slice the
  // visible page from. The hook handles wheel events and
  // keeps the selection visible as the user navigates.
  const { windowStart, wheelRef } = useWindowedList({
    selectedIndex,
    setSelectedIndex,
    resultCount: DEMO_ITEMS.length,
    pageSize: PAGE_SIZE,
  });

  // Component-layer keybindings: ArrowUp / ArrowDown move the
  // selection through the windowed list. The COMPONENT layer
  // sits above the host's launcher layer so these take
  // precedence while the view is mounted.
  useKeyBindings([
    {
      id: "demo-view-up",
      layer: LAYER.COMPONENT,
      keybindings: [{ combo: { modifiers: [], key: "ArrowUp" }, allowInInput: true }],
      handler: () => setSelectedIndex((i) => Math.max(0, i - 1)),
    },
    {
      id: "demo-view-down",
      layer: LAYER.COMPONENT,
      keybindings: [{ combo: { modifiers: [], key: "ArrowDown" }, allowInInput: true }],
      handler: () =>
        setSelectedIndex((i) => Math.min(DEMO_ITEMS.length - 1, i + 1)),
    },
  ]);

  const visible = DEMO_ITEMS.slice(windowStart, windowStart + PAGE_SIZE);

  return (
    <div className="flex flex-col gap-4 p-6">
      <h2 className="text-lg font-semibold text-text-primary">Template Plugin</h2>

      <p className="text-sm text-text-secondary">
        This view was loaded dynamically from a WASM plugin's frontend bundle
        via the <code className="text-accent">torchsnap-plugin://</code> protocol.
      </p>

      {/* Echo the query to prove data flows from WASM → host → React */}
      <div className="rounded-lg border border-border bg-surface-inset p-4">
        <p className="text-xs font-medium text-text-muted uppercase tracking-wide mb-1">
          Query
        </p>
        <p className="text-sm text-text-primary font-mono">
          {echoText || "(empty)"}
        </p>
      </div>

      {/* Local component state, persisted across re-renders. */}
      <div className="flex items-center gap-3">
        <button
          onClick={() => setCount((c) => c + 1)}
          className="rounded-md border border-border bg-surface px-3 py-1.5 text-sm text-text-primary hover:bg-surface-hover transition-colors"
        >
          Count: {count}
        </button>
      </div>

      {/* Windowed list driven by useWindowedList + useKeyBindings. */}
      <div className="flex flex-col gap-1">
        <p className="text-xs font-medium text-text-muted uppercase tracking-wide">
          Items ({selectedIndex + 1}/{DEMO_ITEMS.length})
        </p>
        <div
          ref={wheelRef}
          className="rounded-lg border border-border bg-surface-inset overflow-hidden"
        >
          {visible.map((item, i) => {
            const globalIndex = windowStart + i;
            const isSelected = globalIndex === selectedIndex;
            return (
              <div
                key={item}
                className={
                  "px-3 py-1.5 text-sm " +
                  (isSelected
                    ? "bg-accent text-white"
                    : "text-text-primary hover:bg-surface-hover")
                }
              >
                {item}
              </div>
            );
          })}
        </div>
      </div>

      <button
        onClick={dismiss}
        className="self-start rounded-md bg-accent px-4 py-1.5 text-sm font-medium text-white hover:bg-accent-hover transition-colors"
      >
        Dismiss
      </button>
    </div>
  );
}
