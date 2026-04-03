// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Virtualized Log List
//
// Uses @tanstack/react-virtual for efficient rendering of
// up to 10k log entries. Supports auto-scroll to bottom
// with a "scroll to bottom" button when the user scrolls up.
// =========================================================

import { useCallback, useEffect, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { LogEntryRow } from "./LogEntryRow";
import type { LogEntry } from "../types";

// Distance from bottom (in px) within which auto-scroll
// remains active.
const AUTO_SCROLL_THRESHOLD = 50;

interface LogListProps {
  entries: LogEntry[];
  droppedCount: number;
}

export function LogList({ entries, droppedCount }: LogListProps) {
  const parentRef = useRef<HTMLDivElement>(null);
  const [isAtBottom, setIsAtBottom] = useState(true);
  const [newSinceScroll, setNewSinceScroll] = useState(0);
  const prevLengthRef = useRef(entries.length);

  const virtualizer = useVirtualizer({
    count: entries.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 28,
    overscan: 20,
  });

  // Track whether we're at the bottom.
  const handleScroll = useCallback(() => {
    const el = parentRef.current;
    if (!el) return;
    const atBottom =
      el.scrollHeight - el.scrollTop - el.clientHeight < AUTO_SCROLL_THRESHOLD;
    setIsAtBottom(atBottom);
    if (atBottom) setNewSinceScroll(0);
  }, []);

  // Auto-scroll when new entries arrive and we're at the bottom.
  useEffect(() => {
    const newCount = entries.length - prevLengthRef.current;
    prevLengthRef.current = entries.length;

    if (newCount <= 0) return;

    if (isAtBottom) {
      virtualizer.scrollToIndex(entries.length - 1, { align: "end" });
    } else {
      setNewSinceScroll((c) => c + newCount);
    }
  }, [entries.length, isAtBottom, virtualizer]);

  const scrollToBottom = useCallback(() => {
    virtualizer.scrollToIndex(entries.length - 1, { align: "end" });
    setIsAtBottom(true);
    setNewSinceScroll(0);
  }, [entries.length, virtualizer]);

  // Empty state.
  if (entries.length === 0 && droppedCount === 0) {
    return (
      <div className="flex flex-col items-center justify-center h-full gap-3 text-text-muted">
        <svg className="w-10 h-10 text-text-muted/50" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" d="m6.75 7.5 3 2.25-3 2.25m4.5 0h3m-9 8.25h13.5A2.25 2.25 0 0 0 21 18V6a2.25 2.25 0 0 0-2.25-2.25H5.25A2.25 2.25 0 0 0 3 6v12a2.25 2.25 0 0 0 2.25 2.25Z" />
        </svg>
        <p className="text-sm">No log entries yet</p>
        <p className="text-xs text-text-muted/70">
          Log output from plugins will appear here
        </p>
      </div>
    );
  }

  return (
    <div className="relative h-full">
      {/* Dropped messages banner */}
      {droppedCount > 0 && (
        <div className="flex items-center justify-center gap-2 py-1.5 bg-red-500/10 border-b border-red-500/20 text-red-400 text-[11px]">
          <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" d="M12 9v3.75m-9.303 3.376c-.866 1.5.217 3.374 1.948 3.374h14.71c1.73 0 2.813-1.874 1.948-3.374L13.949 3.378c-.866-1.5-3.032-1.5-3.898 0L2.697 16.126ZM12 15.75h.007v.008H12v-.008Z" />
          </svg>
          <span>{droppedCount} messages dropped due to high volume</span>
        </div>
      )}

      {/* Scrollable log list */}
      <div
        ref={parentRef}
        className="h-full overflow-y-auto"
        onScroll={handleScroll}
      >
        <div
          style={{
            height: `${virtualizer.getTotalSize()}px`,
            width: "100%",
            position: "relative",
          }}
        >
          {virtualizer.getVirtualItems().map((virtualItem) => (
            <div
              key={virtualItem.key}
              style={{
                position: "absolute",
                top: 0,
                left: 0,
                width: "100%",
                transform: `translateY(${virtualItem.start}px)`,
              }}
            >
              <LogEntryRow
                entry={entries[virtualItem.index]}
                index={virtualItem.index}
              />
            </div>
          ))}
        </div>
      </div>

      {/* Scroll to bottom button */}
      {!isAtBottom && (
        <div className="absolute bottom-4 right-4 z-20">
          <button
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-full bg-accent text-white text-xs font-medium shadow-lg hover:bg-accent-hover transition-colors"
            onClick={scrollToBottom}
          >
            <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" d="M19.5 13.5 12 21m0 0-7.5-7.5M12 21V3" />
            </svg>
            {newSinceScroll > 0 && (
              <span className="tabular-nums">{newSinceScroll} new</span>
            )}
          </button>
        </div>
      )}
    </div>
  );
}
