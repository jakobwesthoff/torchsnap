/**
 * Scrollable result list for the launcher.
 *
 * Renders scored entries as `ResultRow` components with keyboard
 * selection, mouse hover gating, and automatic scroll-into-view.
 */

import { useEffect, useRef, type RefObject } from "react";
import type { ScoredEntry } from "./types";
import { ResultRow } from "./ResultRow";

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
  const listRef = useRef<HTMLDivElement>(null);

  // Scroll the selected row into view when selection changes
  // via keyboard navigation.
  useEffect(() => {
    const container = listRef.current;
    if (!container) return;

    const row = container.querySelector(`[data-index="${selectedIndex}"]`);
    if (row) {
      row.scrollIntoView({ block: "nearest" });
    }
  }, [selectedIndex]);

  if (results.length === 0) {
    return (
      <div className="px-5 py-4 text-sm text-text-muted text-center">
        No results
      </div>
    );
  }

  return (
    <div
      ref={listRef}
      className="max-h-[384px] overflow-y-auto"
      onMouseMove={() => {
        mouseActiveRef.current = true;
      }}
    >
      {results.map((entry, index) => (
        <div key={`${entry.source}:${entry.id}`} data-index={index}>
          <ResultRow
            entry={entry}
            selected={index === selectedIndex}
            onSelect={() => onSelectIndex(index)}
            onExecute={() => onExecute(entry)}
            mouseActiveRef={mouseActiveRef}
          />
        </div>
      ))}
    </div>
  );
}
