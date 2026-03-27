// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Windowed result list for the launcher.
 *
 * Only `PAGE_SIZE` rows are rendered at a time. The visible window
 * shifts via keyboard navigation (selection drives window position)
 * or mouse wheel (window shifts independently, hover updates
 * selection). There is no native scroll container or scrollbar.
 */

import { useRef, type RefObject } from "react";
import type { ScoredEntry } from "./types";
import { ResultRow } from "./ResultRow";
import { PAGE_SIZE } from "./constants";
import { useWindowedList } from "./hooks/useWindowedList";

interface ResultListProps {
  results: ScoredEntry[];
  selectedIndex: number;
  onSelectIndex: (index: number) => void;
  onExecute: (entry: ScoredEntry) => void;
  mouseActiveRef: RefObject<boolean>;
}

export function ResultList({
  results,
  selectedIndex,
  onSelectIndex,
  onExecute,
  mouseActiveRef,
}: ResultListProps) {
  const wheelRef = useRef<HTMLDivElement>(null);

  const { windowStart } = useWindowedList({
    selectedIndex,
    setSelectedIndex: onSelectIndex,
    resultCount: results.length,
    pageSize: PAGE_SIZE,
    wheelRef,
  });

  if (results.length === 0) {
    return (
      <div className="px-5 py-4 text-sm text-text-muted text-center">
        No results
      </div>
    );
  }

  const windowEnd = Math.min(windowStart + PAGE_SIZE, results.length);
  const visibleResults = results.slice(windowStart, windowEnd);

  return (
    <div
      ref={wheelRef}
      className="overflow-hidden"
      onMouseMove={() => {
        mouseActiveRef.current = true;
      }}
    >
      {visibleResults.map((entry, i) => {
        const absoluteIndex = windowStart + i;
        return (
          <ResultRow
            key={`${entry.source}:${entry.id}`}
            entry={entry}
            selected={absoluteIndex === selectedIndex}
            onSelect={() => onSelectIndex(absoluteIndex)}
            onExecute={() => onExecute(entry)}
            mouseActiveRef={mouseActiveRef}
          />
        );
      })}
    </div>
  );
}
