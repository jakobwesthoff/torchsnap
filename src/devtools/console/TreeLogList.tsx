// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Tree Log List
//
// Virtualized tree view of log items. Spans are collapsible
// nodes; their children (messages and nested spans) are
// indented under them.
//
// Reuses LogItemRow for regular message rows. Span header
// rows are rendered inline with collapse/expand controls.
// =========================================================

import { useCallback, useEffect, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import {
  CommandLineIcon,
  ExclamationTriangleIcon,
  ArrowDownIcon,
  ChevronRightIcon,
} from "@heroicons/react/24/outline";
import { cn } from "../../lib/cn";
import type { LogItem } from "../types";
import { LogItemRow } from "./LogItemRow";
import type { FlatRow, TreeNode } from "./useTreeView";
import {
  formatTimestamp,
  formatDuration,
  durationColor,
} from "./formatters";
import { PLUGIN_COLORS, pluginColorIndex } from "./pluginColors";

const AUTO_SCROLL_THRESHOLD = 50;

interface TreeLogListProps {
  items: LogItem[];
  flatRows: FlatRow[];
  toggleSpan: (spanId: number) => void;
  collapsedSpans: Set<number>;
  resetCollapse: () => void;
  droppedCount: number;
}

// =========================================================
// Span Header Row
// =========================================================

interface SpanHeaderRowProps {
  node: TreeNode;
  depth: number;
  collapsed: boolean;
  onToggleCollapse: () => void;
  index: number;
}

function SpanHeaderRow({
  node,
  depth,
  collapsed,
  onToggleCollapse,
  index,
}: SpanHeaderRowProps) {
  const startKind = node.spanStart.kind;
  // Type narrowing — we know this is a spanStart item.
  if (startKind.type !== "spanStart") return null;

  const pluginId =
    node.spanStart.source.type === "plugin"
      ? node.spanStart.source.value
      : null;

  const childCount = node.children.length;

  return (
    <div
      onClick={onToggleCollapse}
      className={cn(
        "group flex items-start gap-2 px-3 py-1 text-xs border-b border-border-divider/50",
        "hover:bg-surface-hover/50 transition-colors cursor-pointer",
        "border-l-2 border-l-cyan-500",
        index % 2 === 0 && "bg-surface-inset/20",
      )}
    >
      {/* Timestamp */}
      <span className="shrink-0 w-[85px] tabular-nums text-text-muted font-mono text-[11px] leading-5 select-text">
        {formatTimestamp(node.spanStart.timestamp)}
      </span>

      {/* SPAN badge */}
      <span className="shrink-0 w-[42px] text-center text-[10px] font-semibold uppercase tracking-wider leading-5 text-cyan-400">
        span
      </span>

      {/* Source */}
      <span className="shrink-0 w-[120px] flex items-center gap-1">
        {pluginId ? (
          <>
            <span
              className={cn(
                "w-1.5 h-1.5 rounded-full shrink-0",
                PLUGIN_COLORS[pluginColorIndex(pluginId)],
              )}
            />
            <span className="text-[11px] text-text-secondary truncate font-medium">
              {pluginId}
            </span>
          </>
        ) : (
          <span className="text-[11px] text-text-muted italic">host</span>
        )}
      </span>

      {/* Collapse chevron + indentation + name + duration */}
      <div className="flex-1 min-w-0 flex items-start gap-1">
        {/* Depth indentation */}
        {depth > 0 && (
          <span
            style={{ width: `${depth * 16}px` }}
            className="shrink-0"
          />
        )}

        {/* Collapse/expand chevron */}
        <ChevronRightIcon
          className={cn(
            "w-3.5 h-3.5 shrink-0 mt-0.5 transition-transform text-text-muted",
            !collapsed && "rotate-90",
          )}
        />

        {/* Span name */}
        <span className="flex-1 min-w-0 font-mono text-[11px] leading-5 text-text-primary break-words select-text">
          {startKind.name}
        </span>

        {/* Child count badge when collapsed */}
        {collapsed && childCount > 0 && (
          <span className="shrink-0 text-[10px] text-text-muted tabular-nums">
            ({childCount})
          </span>
        )}

        {/* Duration badge */}
        <span className="shrink-0 w-[70px] text-right">
          {node.spanEnd && node.spanEnd.kind.type === "spanEnd" && (
            <span
              className={cn(
                "text-[10px] font-mono tabular-nums",
                durationColor(node.spanEnd.kind.durationUs),
              )}
            >
              {formatDuration(node.spanEnd.kind.durationUs)}
            </span>
          )}
          {node.inProgress && (
            <span className="inline-flex items-center px-1.5 py-0 rounded-full text-[9px] font-medium bg-cyan-500/10 text-cyan-400 border border-cyan-500/20">
              running
            </span>
          )}
        </span>

        {/* Spacer for chevron + copy alignment with LogItemRow */}
        <span className="shrink-0 w-4" />
        <span className="shrink-0 w-[22px]" />
      </div>
    </div>
  );
}

// =========================================================
// TreeLogList
// =========================================================

export function TreeLogList({
  items,
  flatRows,
  toggleSpan,
  collapsedSpans,
  resetCollapse,
  droppedCount,
}: TreeLogListProps) {
  const parentRef = useRef<HTMLDivElement>(null);
  const [expandedSeqs, setExpandedSeqs] = useState<Set<number>>(
    () => new Set(),
  );
  const [isAtBottom, setIsAtBottom] = useState(true);
  const [newSinceScroll, setNewSinceScroll] = useState(0);
  const prevLengthRef = useRef(flatRows.length);

  // Reset collapse state when items are cleared.
  const prevItemsLength = useRef(items.length);
  useEffect(() => {
    if (items.length === 0 && prevItemsLength.current > 0) {
      resetCollapse();
      setExpandedSeqs(new Set());
    }
    prevItemsLength.current = items.length;
  }, [items.length, resetCollapse]);

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
    count: flatRows.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 28,
    overscan: 20,
    measureElement: (el) => el.getBoundingClientRect().height,
  });

  const handleScroll = useCallback(() => {
    const el = parentRef.current;
    if (!el) return;
    const atBottom =
      el.scrollHeight - el.scrollTop - el.clientHeight < AUTO_SCROLL_THRESHOLD;
    setIsAtBottom(atBottom);
    if (atBottom) setNewSinceScroll(0);
  }, []);

  useEffect(() => {
    const newCount = flatRows.length - prevLengthRef.current;
    prevLengthRef.current = flatRows.length;

    if (newCount <= 0) return;

    if (isAtBottom) {
      virtualizer.scrollToIndex(flatRows.length - 1, { align: "end" });
    } else {
      setNewSinceScroll((c) => c + newCount);
    }
  }, [flatRows.length, isAtBottom, virtualizer]);

  const scrollToBottom = useCallback(() => {
    virtualizer.scrollToIndex(flatRows.length - 1, { align: "end" });
    setIsAtBottom(true);
    setNewSinceScroll(0);
  }, [flatRows.length, virtualizer]);

  // Empty state.
  if (items.length === 0 && droppedCount === 0) {
    return (
      <div className="flex flex-col items-center justify-center h-full gap-3 text-text-muted">
        <CommandLineIcon className="w-10 h-10 text-text-muted/50" />
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
          <ExclamationTriangleIcon className="w-3.5 h-3.5" />
          <span>{droppedCount} messages dropped due to high volume</span>
        </div>
      )}

      {/* Scrollable tree list */}
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
          {virtualizer.getVirtualItems().map((virtualItem) => {
            const row = flatRows[virtualItem.index];
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
                {row.kind === "span-header" ? (
                  <SpanHeaderRow
                    node={row.node}
                    depth={row.depth}
                    collapsed={collapsedSpans.has(row.node.spanId)}
                    onToggleCollapse={() => toggleSpan(row.node.spanId)}
                    index={virtualItem.index}
                  />
                ) : (
                  <LogItemRow
                    item={row.item}
                    index={virtualItem.index}
                    expanded={expandedSeqs.has(row.item.seq)}
                    onToggleExpand={() => {
                      const seq = row.item.seq;
                      setExpandedSeqs((prev) => {
                        const next = new Set(prev);
                        if (next.has(seq)) next.delete(seq);
                        else next.add(seq);
                        return next;
                      });
                    }}
                    depth={row.depth}
                  />
                )}
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
            {newSinceScroll > 0 && (
              <span className="tabular-nums">{newSinceScroll} new</span>
            )}
          </button>
        </div>
      )}
    </div>
  );
}
