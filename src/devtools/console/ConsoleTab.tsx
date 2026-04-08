// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Console Tab
//
// Composes the log stream, filters, toolbar, and log list
// into the full console experience. Supports flat (chrono)
// and tree (grouped by span) view modes. Handles keyboard
// shortcuts for the console (Cmd+K clear, Cmd+F search).
// =========================================================

import { useCallback, useEffect, useRef, useState } from "react";
import { useLogStream } from "./useLogStream";
import { useLogFilters } from "./useLogFilters";
import { useTreeView } from "./useTreeView";
import { ConsoleToolbar, type ViewMode } from "./ConsoleToolbar";
import { LogList } from "./LogList";
import { TreeLogList } from "./TreeLogList";

export function ConsoleTab() {
  const { items, droppedCount, clear, spanDepthMap, completedSpanIds } = useLogStream();
  const {
    filters,
    filteredItems,
    toggleLevel,
    toggleSpans,
    toggleSource,
    setAllSources,
    setSearchText,
    levelCounts,
    spanCount,
    knownSources,
  } = useLogFilters(items);

  const [viewMode, setViewMode] = useState<ViewMode>("flat");
  const searchInputRef = useRef<HTMLInputElement>(null);

  // Tree view state — lifted here so the toolbar can show the
  // correct visible row count in tree mode.
  const { flatRows, toggleSpan, collapsedSpans, resetCollapse } = useTreeView(filteredItems);

  // The count shown in the toolbar should reflect what the user
  // actually sees: in flat mode that's filteredItems.length, in
  // tree mode it's the number of visible (flattened) rows.
  const visibleCount = viewMode === "flat" ? filteredItems.length : flatRows.length;

  // Window-local keyboard shortcuts.
  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if (e.metaKey && e.key === "k") {
        e.preventDefault();
        clear();
      } else if (e.metaKey && e.key === "f") {
        e.preventDefault();
        searchInputRef.current?.focus();
      }
    },
    [clear],
  );

  useEffect(() => {
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [handleKeyDown]);

  return (
    <div className="flex flex-col h-full">
      <ConsoleToolbar
        filters={filters}
        onToggleLevel={toggleLevel}
        onToggleSpans={toggleSpans}
        onToggleSource={toggleSource}
        onSetAllSources={setAllSources}
        onSetSearchText={setSearchText}
        onClear={clear}
        levelCounts={levelCounts}
        spanCount={spanCount}
        knownSources={knownSources}
        totalCount={items.length}
        filteredCount={visibleCount}
        searchInputRef={searchInputRef}
        viewMode={viewMode}
        onSetViewMode={setViewMode}
      />

      <div className="flex-1 min-h-0">
        {viewMode === "flat" ? (
          <LogList
            items={filteredItems}
            droppedCount={droppedCount}
            spanDepthMap={spanDepthMap}
            completedSpanIds={completedSpanIds}
          />
        ) : (
          <TreeLogList
            items={filteredItems}
            flatRows={flatRows}
            toggleSpan={toggleSpan}
            collapsedSpans={collapsedSpans}
            resetCollapse={resetCollapse}
            droppedCount={droppedCount}
          />
        )}
      </div>
    </div>
  );
}
