// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Virtualized Log List (Flat View)
//
// Uses @tanstack/react-virtual for efficient rendering of
// up to 10k log items. Supports auto-scroll to bottom
// with a "scroll to bottom" button when the user scrolls up.
//
// Each item receives a depth value from the spanDepthMap for
// indentation of nested spans and their log messages.
// =========================================================

import { useCallback, useEffect, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import {
  CommandLineIcon,
  ExclamationTriangleIcon,
  ArrowDownIcon,
} from "@heroicons/react/24/outline";
import { LogItemRow } from "./LogItemRow";
import type { LogItem } from "../types";

// Distance from bottom (in px) within which auto-scroll
// remains active.
const AUTO_SCROLL_THRESHOLD = 50;

interface LogListProps {
  items: LogItem[];
  droppedCount: number;
  spanDepthMap: Map<number, number>;
  completedSpanIds: Set<number>;
}

/** Compute the nesting depth for a log item using the span depth map. */
function getItemDepth(item: LogItem, depthMap: Map<number, number>): number {
  const { kind } = item;
  if (kind.type === "spanStart" || kind.type === "spanEnd") {
    return depthMap.get(kind.spanId) ?? 0;
  }
  if (kind.type === "message" && kind.spanId != null) {
    return depthMap.get(kind.spanId) ?? 0;
  }
  return 0;
}

export function LogList({ items, droppedCount, spanDepthMap, completedSpanIds }: LogListProps) {
  const parentRef = useRef<HTMLDivElement>(null);
  const [expandedSeqs, setExpandedSeqs] = useState<Set<number>>(() => new Set());
  const [isAtBottom, setIsAtBottom] = useState(true);
  const [newSinceScroll, setNewSinceScroll] = useState(0);
  const prevLengthRef = useRef(items.length);

  // ESLINT: useVirtualizer returns a mutable ref-backed object that the
  // React Compiler cannot safely memoize. This is a known architectural
  // limitation of @tanstack/react-virtual — the library is on React's
  // explicit incompatible-library list (facebook/react#31820). The
  // compiler already skips memoization for this component automatically;
  // suppressing the warning just silences the noise. Correctness and
  // rendering are unaffected.
  // Track: TanStack/virtual#736, TanStack/virtual#1119
  // eslint-disable-next-line react-hooks/incompatible-library
  const virtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 28,
    overscan: 20,
    // Measure actual row heights so expanded metadata pushes
    // subsequent rows down instead of overlapping.
    measureElement: (el) => el.getBoundingClientRect().height,
  });

  // Track whether we're at the bottom.
  const handleScroll = useCallback(() => {
    const el = parentRef.current;
    if (!el) return;
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < AUTO_SCROLL_THRESHOLD;
    setIsAtBottom(atBottom);
    if (atBottom) setNewSinceScroll(0);
  }, []);

  // Auto-scroll when new items arrive and we're at the bottom.
  useEffect(() => {
    const newCount = items.length - prevLengthRef.current;
    prevLengthRef.current = items.length;

    if (newCount <= 0) return;

    if (isAtBottom) {
      virtualizer.scrollToIndex(items.length - 1, { align: "end" });
    } else {
      setNewSinceScroll((c) => c + newCount);
    }
  }, [items.length, isAtBottom, virtualizer]);

  const scrollToBottom = useCallback(() => {
    virtualizer.scrollToIndex(items.length - 1, { align: "end" });
    setIsAtBottom(true);
    setNewSinceScroll(0);
  }, [items.length, virtualizer]);

  // Empty state.
  if (items.length === 0 && droppedCount === 0) {
    return (
      <div className="flex flex-col items-center justify-center h-full gap-3 text-text-muted">
        <CommandLineIcon className="w-10 h-10 text-text-muted/50" />
        <p className="text-sm">No log entries yet</p>
        <p className="text-xs text-text-muted/70">Log output from gadgets will appear here</p>
      </div>
    );
  }

  return (
    <div className="relative h-full">
      {/* Dropped messages banner */}
      {droppedCount > 0 && (
        <div className="flex items-center justify-center gap-2 py-1.5 bg-red-500/10 border-b border-red-500/20 text-red-400 text-[11px]">
          <ExclamationTriangleIcon className="w-3.5 h-3.5" />
          <span>{droppedCount} messages dropped due to high volume</span>
        </div>
      )}

      {/* Scrollable log list */}
      <div ref={parentRef} className="h-full overflow-y-auto" onScroll={handleScroll}>
        <div
          style={{
            height: `${virtualizer.getTotalSize()}px`,
            width: "100%",
            position: "relative",
          }}
        >
          {virtualizer.getVirtualItems().map((virtualItem) => {
            const logItem = items[virtualItem.index];
            return (
              <div
                key={virtualItem.key}
                ref={virtualizer.measureElement}
                data-index={virtualItem.index}
                style={{
                  position: "absolute",
                  top: 0,
                  left: 0,
                  width: "100%",
                  transform: `translateY(${virtualItem.start}px)`,
                }}
              >
                <LogItemRow
                  item={logItem}
                  index={virtualItem.index}
                  expanded={expandedSeqs.has(logItem.seq)}
                  onToggleExpand={() => {
                    const seq = logItem.seq;
                    setExpandedSeqs((prev) => {
                      const next = new Set(prev);
                      if (next.has(seq)) next.delete(seq);
                      else next.add(seq);
                      return next;
                    });
                  }}
                  depth={getItemDepth(logItem, spanDepthMap)}
                  completedSpanIds={completedSpanIds}
                />
              </div>
            );
          })}
        </div>
      </div>

      {/* Scroll to bottom button */}
      {!isAtBottom && (
        <div className="absolute bottom-4 right-4 z-20">
          <button
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-full bg-accent text-white text-xs font-medium shadow-lg hover:bg-accent-hover transition-colors"
            onClick={scrollToBottom}
          >
            <ArrowDownIcon className="w-3.5 h-3.5" />
            {newSinceScroll > 0 && <span className="tabular-nums">{newSinceScroll} new</span>}
          </button>
        </div>
      )}
    </div>
  );
}
