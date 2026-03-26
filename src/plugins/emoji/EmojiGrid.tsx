// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Emoji picker grid — first plugin custom UI (ADR 0013).
 *
 * Renders emoji results as a 10-column grid instead of the standard
 * vertical result list. Each cell shows the emoji glyph; the
 * shortcode and label are displayed in the footer for the selected
 * cell.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useKeyBindings, LAYER, type KeyBindingDefinition } from "@torchsnap/keybindings";
import { cn } from "../../lib/cn";
import type { PluginViewProps } from "../types";
import type { ScoredEntry } from "@torchsnap/types";
import { GRID_COLUMNS, GRID_VISIBLE_ROWS } from "./constants";
import { useWindowedGrid } from "./useWindowedGrid";
import type { RefObject } from "react";

// Plugin keybindings sit above the host's launcher layer so they
// take priority when the plugin is mounted. The host deregisters
// its bindings via enabled=false, but using a higher layer adds
// defense-in-depth.
const PLUGIN_LAYER = LAYER.COMPONENT + 2;

// =========================================================
// Grid Cell
// =========================================================

interface GridCellProps {
  entry: ScoredEntry;
  selected: boolean;
  onSelect: () => void;
  onExecute: () => void;
  mouseActiveRef: RefObject<boolean>;
}

function GridCell({ entry, selected, onSelect, onExecute, mouseActiveRef }: GridCellProps) {
  return (
    <div
      className={cn(
        "flex h-[68px] w-[68px] items-center justify-center rounded-lg",
        "cursor-default select-none",
        selected
          ? "bg-accent/10 border border-accent/30"
          : "border border-transparent",
      )}
      onMouseEnter={() => {
        if (mouseActiveRef.current) onSelect();
      }}
      onClick={onExecute}
      title={entry.title}
    >
      <span className="text-3xl leading-none">{entry.icon?.value}</span>
    </div>
  );
}

// =========================================================
// Emoji Grid
// =========================================================

export default function EmojiGrid({
  results,
  goBack,
  mouseActiveRef,
  onExecute,
  onFooterChange,
}: PluginViewProps) {
  const [selectedIndex, setSelectedIndex] = useState(0);
  const gridRef = useRef<HTMLDivElement>(null);

  // Reset selection when results change (new query).
  useEffect(() => {
    setSelectedIndex(0);
  }, [results]);

  // -------------------------------------------------------
  // Windowing
  // -------------------------------------------------------

  const { windowStartRow } = useWindowedGrid({
    selectedIndex,
    setSelectedIndex,
    resultCount: results.length,
    columns: GRID_COLUMNS,
    visibleRows: GRID_VISIBLE_ROWS,
    gridRef,
  });

  // -------------------------------------------------------
  // Footer sync
  //
  // Show the selected emoji's shortcode and label alongside
  // the Enter key hint.
  // -------------------------------------------------------

  useEffect(() => {
    const entry = results[selectedIndex];
    onFooterChange({
      primary: {
        combo: { modifiers: [], key: "Enter" },
        label: "Copy to Clipboard",
      },
      hints: entry
        ? [{ label: `${entry.title} · ${entry.subtitle ?? ""}` }]
        : [],
    });
  }, [selectedIndex, results, onFooterChange]);

  // -------------------------------------------------------
  // Keyboard navigation
  // -------------------------------------------------------

  const moveSelection = useCallback(
    (delta: number) => {
      mouseActiveRef.current = false;
      setSelectedIndex((prev) => {
        const target = prev + delta;

        // For vertical movement (delta is a multiple of GRID_COLUMNS),
        // don't jump to the last item if the target column doesn't
        // exist in the target row. Instead, stay put.
        if (
          Math.abs(delta) >= GRID_COLUMNS &&
          (target < 0 || target >= results.length)
        ) {
          return prev;
        }

        return Math.max(0, Math.min(target, results.length - 1));
      });
    },
    [results.length, mouseActiveRef],
  );

  const handleEnter = useCallback(() => {
    const entry = results[selectedIndex];
    if (entry) {
      onExecute(entry.id, { type: "copy" });
    }
  }, [results, selectedIndex, onExecute]);

  const gridBindings: KeyBindingDefinition[] = useMemo(
    () => [
      {
        id: "emoji-grid-right",
        layer: PLUGIN_LAYER,
        order: 0,
        handler: () => moveSelection(1),
        keybindings: [
          { combo: { modifiers: [], key: "ArrowRight" }, allowInInput: true },
        ],
      },
      {
        id: "emoji-grid-left",
        layer: PLUGIN_LAYER,
        order: 1,
        handler: () => moveSelection(-1),
        keybindings: [
          { combo: { modifiers: [], key: "ArrowLeft" }, allowInInput: true },
        ],
      },
      {
        id: "emoji-grid-down",
        layer: PLUGIN_LAYER,
        order: 2,
        handler: () => moveSelection(GRID_COLUMNS),
        keybindings: [
          { combo: { modifiers: [], key: "ArrowDown" }, allowInInput: true },
        ],
      },
      {
        id: "emoji-grid-up",
        layer: PLUGIN_LAYER,
        order: 3,
        handler: () => moveSelection(-GRID_COLUMNS),
        keybindings: [
          { combo: { modifiers: [], key: "ArrowUp" }, allowInInput: true },
        ],
      },
      {
        id: "emoji-grid-page-down",
        layer: PLUGIN_LAYER,
        order: 4,
        handler: () => moveSelection(GRID_COLUMNS * GRID_VISIBLE_ROWS),
        keybindings: [
          { combo: { modifiers: [], key: "PageDown" }, allowInInput: true },
        ],
      },
      {
        id: "emoji-grid-page-up",
        layer: PLUGIN_LAYER,
        order: 5,
        handler: () => moveSelection(-GRID_COLUMNS * GRID_VISIBLE_ROWS),
        keybindings: [
          { combo: { modifiers: [], key: "PageUp" }, allowInInput: true },
        ],
      },
      {
        id: "emoji-grid-enter",
        layer: PLUGIN_LAYER,
        order: 6,
        handler: handleEnter,
        keybindings: [
          { combo: { modifiers: [], key: "Enter" }, allowInInput: true },
        ],
      },
      {
        id: "emoji-grid-escape",
        layer: PLUGIN_LAYER,
        order: 10,
        handler: () => goBack(),
        keybindings: [
          { combo: { modifiers: [], key: "Escape" }, allowInInput: true },
        ],
      },
    ],
    [moveSelection, handleEnter, goBack],
  );

  useKeyBindings(gridBindings);

  // -------------------------------------------------------
  // Render
  // -------------------------------------------------------

  const startIdx = windowStartRow * GRID_COLUMNS;
  const endIdx = Math.min(
    startIdx + GRID_COLUMNS * GRID_VISIBLE_ROWS,
    results.length,
  );
  const visible = results.slice(startIdx, endIdx);

  return (
    <div
      ref={gridRef}
      className="grid grid-cols-10 overflow-hidden"
      onMouseMove={() => {
        mouseActiveRef.current = true;
      }}
    >
      {visible.map((entry, i) => {
        const absIdx = startIdx + i;
        return (
          <GridCell
            key={entry.id}
            entry={entry}
            selected={absIdx === selectedIndex}
            onSelect={() => setSelectedIndex(absIdx)}
            onExecute={() => onExecute(entry.id, { type: "copy" })}
            mouseActiveRef={mouseActiveRef}
          />
        );
      })}
    </div>
  );
}
