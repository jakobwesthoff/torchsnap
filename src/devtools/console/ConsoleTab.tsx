// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Console Tab
//
// Composes the log stream, filters, toolbar, and log list
// into the full console experience. Handles keyboard
// shortcuts for the console (Cmd+K clear, Cmd+F search).
// =========================================================

import { useCallback, useEffect, useRef } from "react";
import { useLogStream } from "./useLogStream";
import { useLogFilters } from "./useLogFilters";
import { ConsoleToolbar } from "./ConsoleToolbar";
import { LogList } from "./LogList";

export function ConsoleTab() {
  const { entries, droppedCount, clear } = useLogStream();
  const {
    filters,
    filteredEntries,
    toggleLevel,
    setSearchText,
    levelCounts,
  } = useLogFilters(entries);

  const searchInputRef = useRef<HTMLInputElement>(null);

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
        onSetSearchText={setSearchText}
        onClear={clear}
        levelCounts={levelCounts}
        totalCount={entries.length}
        filteredCount={filteredEntries.length}
        searchInputRef={searchInputRef}
      />

      <div className="flex-1 min-h-0">
        <LogList entries={filteredEntries} droppedCount={droppedCount} />
      </div>
    </div>
  );
}
