// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Emoji picker grid — the custom UI the host mounts when the
 * user triggers the `:` prefix.
 *
 * Renders results as a 10-column grid instead of the standard
 * vertical result list. Each cell shows the emoji glyph; the
 * shortcode and label are displayed in the footer for the
 * selected cell.
 */

import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type RefObject,
} from "react";
import type { GadgetViewProps, SourcedEntry } from "@torchsnap/gadget-sdk";
import { useLauncher } from "@torchsnap/gadget-sdk/hooks";
import {
  LAYER,
  useKeyBindings,
  type KeyBindingDefinition,
} from "@torchsnap/gadget-sdk/keybindings";
import { highlightText } from "@torchsnap/gadget-sdk/utils";
import { GRID_COLUMNS, GRID_VISIBLE_ROWS } from "./constants";
import { useWindowedGrid } from "./useWindowedGrid";

/**
 * Truthy-class joiner. The grid has no conflicting-class
 * patterns that would need `tailwind-merge` resolution — the
 * only `border` toggle is disambiguated by the ternary
 * branches in `GridCell`.
 */
function cn(...classes: (string | false | null | undefined)[]): string {
  return classes.filter(Boolean).join(" ");
}

// Picker bindings sit above the host's launcher layer so
// Enter / arrows / Escape route to the grid while it is
// mounted, not to the default result list.
const PLUGIN_LAYER = LAYER.COMPONENT + 2;

const FOOTER_HIGHLIGHT = "underline font-bold";

// =========================================================
// Grid Cell
// =========================================================

interface GridCellProps {
  entry: SourcedEntry;
  selected: boolean;
  onSelect: () => void;
  onExecute: () => void;
  mouseActiveRef: RefObject<boolean>;
}

function GridCell({ entry, selected, onSelect, onExecute, mouseActiveRef }: GridCellProps) {
  return (
    <div
      className={cn(
        "flex aspect-square items-center justify-center rounded-lg",
        "cursor-default select-none",
        selected ? "bg-selection border border-accent/30" : "border border-transparent",
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

export function EmojiGrid({ results }: GadgetViewProps) {
  const { goBack, mouseActiveRef, onExecute, onFooterChange } = useLauncher();
  const [selectedIndex, setSelectedIndex] = useState(0);

  // Reset selection when a new query produces a new result
  // array so the first entry is highlighted by default.
  const [prevResults, setPrevResults] = useState(results);
  if (prevResults !== results) {
    setPrevResults(results);
    setSelectedIndex(0);
  }

  const { windowStartRow, wheelRef: gridRef } = useWindowedGrid({
    selectedIndex,
    setSelectedIndex,
    resultCount: results.length,
    columns: GRID_COLUMNS,
    visibleRows: GRID_VISIBLE_ROWS,
  });

  // Footer shows the selected emoji's shortcode and label
  // alongside the Enter key hint. Highlight positions come
  // from the gadget's Rust side as UTF-16 offsets.
  useEffect(() => {
    const entry = results[selectedIndex];
    onFooterChange({
      primary: {
        combo: { modifiers: [], key: "Enter" },
        label: "Copy to Clipboard",
      },
      hints: entry
        ? [
            {
              label: (
                <>
                  {highlightText(entry.title, entry.titlePositions, FOOTER_HIGHLIGHT)}
                  {" · "}
                  {highlightText(entry.subtitle ?? "", entry.subtitlePositions, FOOTER_HIGHLIGHT)}
                </>
              ),
            },
          ]
        : [],
    });
  }, [selectedIndex, results, onFooterChange]);

  const moveSelection = useCallback(
    (delta: number) => {
      mouseActiveRef.current = false;
      setSelectedIndex((prev) => {
        const target = prev + delta;

        // Vertical jumps (delta is a full row or more) must
        // not clamp to the last item when the destination
        // column doesn't exist in the target row — clamping
        // would make ArrowDown from the last full row land
        // on a different column. Staying put preserves the
        // column invariant.
        if (Math.abs(delta) >= GRID_COLUMNS && (target < 0 || target >= results.length)) {
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
      onExecute(entry.id, "copy");
    }
  }, [results, selectedIndex, onExecute]);

  const gridBindings: KeyBindingDefinition[] = useMemo(
    () => [
      {
        id: "emoji-grid-right",
        layer: PLUGIN_LAYER,
        order: 0,
        handler: () => moveSelection(1),
        keybindings: [{ combo: { modifiers: [], key: "ArrowRight" }, allowInInput: true }],
      },
      {
        id: "emoji-grid-left",
        layer: PLUGIN_LAYER,
        order: 1,
        handler: () => moveSelection(-1),
        keybindings: [{ combo: { modifiers: [], key: "ArrowLeft" }, allowInInput: true }],
      },
      {
        id: "emoji-grid-down",
        layer: PLUGIN_LAYER,
        order: 2,
        handler: () => moveSelection(GRID_COLUMNS),
        keybindings: [{ combo: { modifiers: [], key: "ArrowDown" }, allowInInput: true }],
      },
      {
        id: "emoji-grid-up",
        layer: PLUGIN_LAYER,
        order: 3,
        handler: () => moveSelection(-GRID_COLUMNS),
        keybindings: [{ combo: { modifiers: [], key: "ArrowUp" }, allowInInput: true }],
      },
      {
        id: "emoji-grid-page-down",
        layer: PLUGIN_LAYER,
        order: 4,
        handler: () => moveSelection(GRID_COLUMNS * GRID_VISIBLE_ROWS),
        keybindings: [{ combo: { modifiers: [], key: "PageDown" }, allowInInput: true }],
      },
      {
        id: "emoji-grid-page-up",
        layer: PLUGIN_LAYER,
        order: 5,
        handler: () => moveSelection(-GRID_COLUMNS * GRID_VISIBLE_ROWS),
        keybindings: [{ combo: { modifiers: [], key: "PageUp" }, allowInInput: true }],
      },
      {
        id: "emoji-grid-enter",
        layer: PLUGIN_LAYER,
        order: 6,
        handler: handleEnter,
        keybindings: [{ combo: { modifiers: [], key: "Enter" }, allowInInput: true }],
      },
      {
        id: "emoji-grid-escape",
        layer: PLUGIN_LAYER,
        order: 10,
        handler: () => goBack(),
        keybindings: [{ combo: { modifiers: [], key: "Escape" }, allowInInput: true }],
      },
    ],
    [moveSelection, handleEnter, goBack],
  );

  useKeyBindings(gridBindings);

  const startIdx = windowStartRow * GRID_COLUMNS;
  const endIdx = Math.min(startIdx + GRID_COLUMNS * GRID_VISIBLE_ROWS, results.length);
  const visible = results.slice(startIdx, endIdx);

  return (
    <div
      ref={gridRef}
      className="grid grid-cols-10 overflow-hidden p-1.5"
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
            onExecute={() => onExecute(entry.id, "copy")}
            mouseActiveRef={mouseActiveRef}
          />
        );
      })}
    </div>
  );
}
