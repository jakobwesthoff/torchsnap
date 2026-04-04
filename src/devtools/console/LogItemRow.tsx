// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Log Item Row
//
// Renders a single log item as a horizontal line with:
// - Colored left border (level indicator, cyan for spans)
// - Depth-based indentation for nested spans
// - Timestamp in monospace
// - Level badge ("SPAN" for span entries)
// - Plugin source with colored dot
// - Message / span name in monospace
// - Duration badge (span-end) or "..." (span-start)
// - Chevron (always allocated, invisible when no metadata)
// - Copy button on hover
//
// The entire row is clickable to toggle metadata expansion.
// =========================================================

import { memo, useCallback } from "react";
import {
  ChevronRightIcon,
  ClipboardDocumentIcon,
} from "@heroicons/react/24/outline";
import { PlayIcon, StopIcon } from "@heroicons/react/16/solid";
import { cn } from "../../lib/cn";
import type { LogItem, LogLevel } from "../types";
import { PLUGIN_COLORS, pluginColorIndex } from "./pluginColors";
import {
  formatTimestamp,
  formatDuration,
  durationColor,
  formatItemForClipboard,
} from "./formatters";

// =========================================================
// Level Colors
// =========================================================

const levelBorderColor: Record<LogLevel, string> = {
  error: "border-l-red-500",
  warn: "border-l-amber-500",
  info: "border-l-blue-500",
  debug: "border-l-gray-400",
  trace: "border-l-gray-300",
};

const levelTextColor: Record<LogLevel, string> = {
  error: "text-red-400",
  warn: "text-amber-400",
  info: "text-blue-400",
  debug: "text-text-tertiary",
  trace: "text-text-muted",
};

// =========================================================
// Component
// =========================================================

interface LogItemRowProps {
  item: LogItem;
  index: number;
  expanded: boolean;
  onToggleExpand: () => void;
  depth: number;
  /** Span IDs that have completed — used to hide the "running" pill once a span ends. */
  completedSpanIds?: Set<number>;
}

export const LogItemRow = memo(function LogItemRow({
  item,
  index,
  expanded,
  onToggleExpand,
  depth,
  completedSpanIds,
}: LogItemRowProps) {
  const { kind } = item;
  const isSpan = kind.type !== "message";
  const metadata = kind.metadata;
  const hasMetadata = metadata.length > 0;

  const pluginId =
    item.source.type === "plugin" ? item.source.value : null;

  const copyToClipboard = useCallback((e: React.MouseEvent) => {
    e.stopPropagation();
    navigator.clipboard.writeText(formatItemForClipboard(item));
  }, [item]);

  // Determine display text and level badge.
  const messageText = kind.type === "message" ? kind.message : kind.name;

  return (
    <div
      onClick={hasMetadata ? onToggleExpand : undefined}
      className={cn(
        "group flex items-start gap-2 px-3 py-1 text-xs border-b border-border-divider/50",
        "hover:bg-surface-hover/50 transition-colors",
        "border-l-2",
        isSpan
          ? "border-l-cyan-500"
          : levelBorderColor[kind.level],
        index % 2 === 0 && "bg-surface-inset/20",
        hasMetadata && "cursor-pointer",
      )}
    >
      {/* Timestamp */}
      <span className="shrink-0 w-[85px] tabular-nums text-text-muted font-mono text-[11px] leading-5 select-text">
        {formatTimestamp(item.timestamp)}
      </span>

      {/* Level badge — shows "SPAN" + direction icon for span entries */}
      <span
        className={cn(
          "shrink-0 w-[52px] text-left text-[10px] font-semibold uppercase tracking-wider leading-5 inline-flex items-center gap-0.5",
          isSpan ? "text-cyan-400" : levelTextColor[kind.level],
        )}
      >
        {isSpan
          ? <>span{kind.type === "spanStart"
              ? <PlayIcon className="w-2.5 h-2.5" />
              : <StopIcon className="w-2.5 h-2.5" />}</>
          : kind.level}
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

      {/* Message + trailing controls */}
      <div className="flex-1 min-w-0 flex flex-col">
        <div className="flex items-start gap-1">
          {/* Depth indentation */}
          {depth > 0 && (
            <span
              style={{ width: `${depth * 16}px` }}
              className="shrink-0"
            />
          )}

          {/* Message */}
          <span className="flex-1 min-w-0 font-mono text-[11px] leading-5 text-text-primary break-words select-text">
            {messageText}
          </span>

          {/* Duration badge — fixed-width slot */}
          <span className="shrink-0 w-[70px] text-right">
            {kind.type === "spanEnd" && (
              <span
                className={cn(
                  "text-[10px] font-mono tabular-nums",
                  durationColor(kind.durationUs),
                )}
              >
                {formatDuration(kind.durationUs)}
              </span>
            )}
            {kind.type === "spanStart" && !(completedSpanIds?.has(kind.spanId)) && (
              <span className="inline-flex items-center px-1.5 py-0 rounded-full text-[9px] font-medium bg-cyan-500/10 text-cyan-400 border border-cyan-500/20">
                running
              </span>
            )}
          </span>

          {/* Chevron — always occupies space, invisible when no metadata */}
          <span className="shrink-0 w-4 flex items-center justify-center">
            {hasMetadata && (
              <ChevronRightIcon
                className={cn(
                  "w-3 h-3 transition-transform text-text-muted",
                  expanded && "rotate-90",
                )}
              />
            )}
          </span>

          {/* Copy button (visible on hover) */}
          <button
            className="opacity-0 group-hover:opacity-100 shrink-0 p-0.5 text-text-muted hover:text-text-primary transition-opacity"
            onClick={copyToClipboard}
            title="Copy to clipboard"
          >
            <ClipboardDocumentIcon className="w-3.5 h-3.5" />
          </button>
        </div>

        {/* Expanded metadata */}
        {expanded && hasMetadata && (
          <div className="mt-1 flex flex-wrap gap-x-4 gap-y-0.5">
            {metadata.map(([key, value]) => (
              <span key={key} className="text-[10px] font-mono">
                <span className="text-violet-400">{key}</span>
                <span className="text-text-muted">=</span>
                <span className="text-text-secondary select-text">{value}</span>
              </span>
            ))}
          </div>
        )}
      </div>
    </div>
  );
});
