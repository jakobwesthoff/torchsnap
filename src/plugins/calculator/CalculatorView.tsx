// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Prefix mode custom UI for the calculator plugin.
 *
 * Registered as `views["history"]` in the plugin registry. Layout:
 * - Top: CalculatorResult (or CalculatorHelp if no result)
 * - Divider
 * - History list (windowed, most recent first)
 *
 * Selection model:
 * - Index 0: inline result area (no visual highlight)
 * - Index 1+: history entries (with standard selection highlight)
 *
 * Enter always copies the current result to clipboard, saves to
 * history (if valid), and dismisses.
 */

import { memo, useEffect, useState } from "react";
import type { PluginViewProps } from "../types";
import type { FooterState } from "../../launcher/types";
import { CalculatorResult } from "./CalculatorResult";
import { CalculatorHelp, CalculatorError } from "./CalculatorHelp";
import { useKeyBindings } from "../../keybindings/useKeyBindings";
import { LAYER } from "../../keybindings/matching";
import { useWindowedList } from "../../launcher/hooks/useWindowedList";

// Layout constants. The inline area (result/help/error) has a fixed
// height so the history list below it doesn't shift as the content
// changes. The page size is derived from the remaining space.
const INLINE_AREA_HEIGHT = 76; // px — fits CalculatorResult comfortably
const HISTORY_ROW_HEIGHT = 52; // px — py-2 (16) + two text lines (36)
const DIVIDER_HEIGHT = 1; // px — border-t
const MAX_CONTENT_HEIGHT = 360; // px — max-h on the outer container
const HISTORY_PAGE_SIZE = Math.floor(
  (MAX_CONTENT_HEIGHT - INLINE_AREA_HEIGHT - DIVIDER_HEIGHT) / HISTORY_ROW_HEIGHT,
);

interface CalcData {
  expression: string;
  result?: string;
  resultType?: string;
  error?: string;
}

/** Static footer — always the same in prefix mode. */
const CALCULATOR_FOOTER: FooterState = {
  primary: { combo: { modifiers: [], key: "Enter" }, label: "Copy to Clipboard" },
  hints: [{ combo: { modifiers: [], key: "Escape" }, label: "Back" }],
};

export default memo(function CalculatorView({
  results,
  data,
  query,
  matchedPrefix,
  goBack,
  mouseActiveRef,
  onExecute,
  onFooterChange,
  setDisplayQuery,
  sendMessage,
}: PluginViewProps) {
  // The backend's search() returns the eval result in the `data`
  // field of the CustomUI response, threaded through PluginViewRef.
  const rawData = data as CalcData | null | undefined;
  // Separate success (has result) from error (has error message).
  const evalResult =
    rawData?.result != null
      ? {
          expression: rawData.expression,
          result: rawData.result,
          resultType: rawData.resultType ?? "number",
        }
      : null;
  const evalError = rawData?.error ?? null;

  const [selectedIndex, setSelectedIndex] = useState(0);
  const [originalQuery, setOriginalQuery] = useState(query);

  // Track when query changes from user typing (not from history selection).
  const [prevQuery, setPrevQuery] = useState(query);
  if (prevQuery !== query) {
    setPrevQuery(query);
    setSelectedIndex(0);
    setOriginalQuery(query);
  }

  // Set footer on mount.
  useEffect(() => {
    onFooterChange(CALCULATOR_FOOTER);
  }, [onFooterChange]);

  // History entries from the search results.
  const historyEntries = results;

  // Total navigable items: result area (index 0) + history entries.
  const totalCount = 1 + historyEntries.length;

  // Windowed list for history (starts at index 1 in selection).
  const historySelectedIndex = selectedIndex > 0 ? selectedIndex - 1 : -1;
  const { windowStart, wheelRef } = useWindowedList({
    selectedIndex: historySelectedIndex >= 0 ? historySelectedIndex : 0,
    setSelectedIndex: (idx) => setSelectedIndex(idx + 1),
    resultCount: historyEntries.length,
    pageSize: HISTORY_PAGE_SIZE,
  });

  // When a history entry is selected, populate the display query.
  useEffect(() => {
    if (selectedIndex > 0 && selectedIndex - 1 < historyEntries.length) {
      const entry = historyEntries[selectedIndex - 1];
      setDisplayQuery(`${matchedPrefix}${entry.title}`);
    } else if (selectedIndex === 0) {
      // Revert to the original user-typed query.
      setDisplayQuery(`${matchedPrefix}${originalQuery}`);
    }
  }, [selectedIndex, historyEntries, matchedPrefix, setDisplayQuery, originalQuery]);

  // Keyboard navigation.
  useKeyBindings([
    {
      id: "calculator-view-up",
      layer: LAYER.COMPONENT + 2,
      keybindings: [{ combo: { modifiers: [], key: "ArrowUp" }, allowInInput: true }],
      handler: () => {
        mouseActiveRef.current = false;
        setSelectedIndex((prev) => Math.max(0, prev - 1));
      },
    },
    {
      id: "calculator-view-down",
      layer: LAYER.COMPONENT + 2,
      keybindings: [{ combo: { modifiers: [], key: "ArrowDown" }, allowInInput: true }],
      handler: () => {
        mouseActiveRef.current = false;
        setSelectedIndex((prev) => Math.min(totalCount - 1, prev + 1));
      },
    },
    {
      id: "calculator-view-enter",
      layer: LAYER.COMPONENT + 2,
      keybindings: [{ combo: { modifiers: [], key: "Enter" }, allowInInput: true }],
      handler: () => {
        // Determine which result to copy.
        let resultToCopy: string | null = null;
        let expressionToSave: string | null = null;
        let resultType: string | null = null;

        if (selectedIndex === 0 && evalResult) {
          resultToCopy = evalResult.result;
          expressionToSave = evalResult.expression;
          resultType = evalResult.resultType;
        } else if (selectedIndex > 0 && selectedIndex - 1 < historyEntries.length) {
          const entry = historyEntries[selectedIndex - 1];
          resultToCopy = entry.subtitle ?? entry.title;
          expressionToSave = entry.title;
          resultType = "number";
        }

        if (resultToCopy) {
          // Save to history before executing (which dismisses).
          if (expressionToSave) {
            sendMessage("save_history", {
              expression: expressionToSave,
              result: resultToCopy,
              resultType: resultType ?? "number",
            }).catch(() => {});
          }

          onExecute(resultToCopy, { type: "copy" });
        }
      },
    },
    {
      id: "calculator-view-escape",
      layer: LAYER.COMPONENT + 2,
      keybindings: [{ combo: { modifiers: [], key: "Escape" }, allowInInput: true }],
      handler: () => {
        goBack();
      },
    },
  ]);

  // =========================================================
  // Inline result area
  // =========================================================

  const isExpressionEmpty = !query.trim();

  // When a history entry is selected, show its expression and result
  // in the inline area instead of the typed query's evaluation.
  const selectedHistoryEntry =
    selectedIndex > 0 && selectedIndex - 1 < historyEntries.length
      ? historyEntries[selectedIndex - 1]
      : null;

  let inlineArea: React.ReactNode;
  if (selectedHistoryEntry && selectedHistoryEntry.subtitle) {
    // History entry selected — display its stored expression and result.
    inlineArea = (
      <CalculatorResult
        expression={selectedHistoryEntry.title}
        result={selectedHistoryEntry.subtitle}
        resultType="number"
      />
    );
  } else if (evalResult) {
    inlineArea = (
      <CalculatorResult
        expression={evalResult.expression}
        result={evalResult.result}
        resultType={evalResult.resultType}
      />
    );
  } else if (isExpressionEmpty) {
    // Just `=` typed — show centered usage examples.
    inlineArea = <CalculatorHelp />;
  } else if (evalError) {
    // Parser/evaluator error — show just the error, no help.
    inlineArea = <CalculatorError message={evalError} />;
  } else {
    inlineArea = <CalculatorHelp />;
  }

  // =========================================================
  // Render
  // =========================================================

  const visibleHistory = historyEntries.slice(windowStart, windowStart + HISTORY_PAGE_SIZE);

  return (
    <div className="flex flex-col" style={{ maxHeight: MAX_CONTENT_HEIGHT }}>
      {/* Inline result / help area — fixed height so history doesn't shift */}
      <div
        className="flex items-center overflow-hidden shrink-0"
        style={{ height: INLINE_AREA_HEIGHT }}
      >
        <div className="w-full">{inlineArea}</div>
      </div>

      {/* Divider */}
      {historyEntries.length > 0 && <div className="border-t border-border" />}

      {/* History list */}
      {historyEntries.length > 0 && (
        <div ref={wheelRef} className="flex-1 overflow-hidden">
          {visibleHistory.map((entry, i) => {
            const globalIndex = windowStart + i;
            const isSelected = historySelectedIndex === globalIndex;

            return (
              <div
                key={entry.id}
                className={`flex items-center gap-3 px-5 py-2 cursor-default ${
                  isSelected ? "bg-selection" : ""
                }`}
                onMouseMove={() => {
                  if (!mouseActiveRef.current) return;
                  setSelectedIndex(globalIndex + 1);
                }}
                onMouseEnter={() => {
                  mouseActiveRef.current = true;
                }}
                onClick={() => {
                  setSelectedIndex(globalIndex + 1);
                  if (entry.subtitle) {
                    sendMessage("save_history", {
                      expression: entry.title,
                      result: entry.subtitle,
                      resultType: "number",
                    }).catch(() => {});
                    onExecute(entry.subtitle, { type: "copy" });
                  }
                }}
              >
                {/* Clock icon */}
                <svg
                  className="h-4 w-4 shrink-0 text-text-muted"
                  fill="none"
                  viewBox="0 0 24 24"
                  strokeWidth={1.5}
                  stroke="currentColor"
                >
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    d="M12 6v6h4.5m4.5 0a9 9 0 1 1-18 0 9 9 0 0 1 18 0Z"
                  />
                </svg>
                <div className="flex flex-col min-w-0 flex-1">
                  <span className="text-sm text-text-primary truncate">{entry.title}</span>
                  {entry.subtitle && (
                    <span className="text-xs text-text-muted truncate">{entry.subtitle}</span>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
});
