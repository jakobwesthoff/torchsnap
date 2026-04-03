// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Console Tab
//
// Composes the log stream, log list, and toolbar (added in a
// later step) into the full console experience.
// =========================================================

import { useLogStream } from "./useLogStream";
import { LogList } from "./LogList";

export function ConsoleTab() {
  const { entries, droppedCount, clear } = useLogStream();

  return (
    <div className="flex flex-col h-full">
      {/* Toolbar placeholder — will be replaced with ConsoleToolbar */}
      <div className="flex items-center gap-2 px-3 py-2 border-b border-border bg-surface-inset/30 shrink-0">
        <span className="text-xs text-text-muted tabular-nums">
          {entries.length} entries
        </span>
        <div className="flex-1" />
        <button
          className="p-1.5 rounded-md text-text-tertiary hover:text-text-primary hover:bg-surface-hover transition-colors"
          onClick={clear}
          title="Clear log"
        >
          <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" d="m14.74 9-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 0 1-2.244 2.077H8.084a2.25 2.25 0 0 1-2.244-2.077L4.772 5.79m14.456 0a48.108 48.108 0 0 0-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.11 0 0 1 3.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 0 0-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 0 0-7.5 0" />
          </svg>
        </button>
      </div>

      {/* Log list */}
      <div className="flex-1 min-h-0">
        <LogList entries={entries} droppedCount={droppedCount} />
      </div>
    </div>
  );
}
