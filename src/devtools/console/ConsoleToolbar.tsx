// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Console Toolbar
//
// Filter bar with level pills, span pill, source filter,
// search input, and action buttons. Groups are separated by
// thin vertical dividers.
// =========================================================

import { useRef } from "react";
import { TrashIcon } from "@heroicons/react/24/outline";
import { cn } from "../../lib/cn";
import type { LogLevel } from "../types";
import type { LogFilters } from "./useLogFilters";

// =========================================================
// Level Pill Configuration
// =========================================================

interface LevelStyle {
  active: string;
  dot: string;
  text: string;
}

const levelStyles: Record<LogLevel, LevelStyle> = {
  error: {
    active: "bg-red-500/15 border-red-500/30",
    dot: "bg-red-500",
    text: "text-red-400",
  },
  warn: {
    active: "bg-amber-500/15 border-amber-500/30",
    dot: "bg-amber-500",
    text: "text-amber-400",
  },
  info: {
    active: "bg-blue-500/15 border-blue-500/30",
    dot: "bg-blue-500",
    text: "text-blue-400",
  },
  debug: {
    active: "bg-gray-500/15 border-gray-500/30",
    dot: "bg-gray-400",
    text: "text-text-tertiary",
  },
  trace: {
    active: "bg-gray-500/10 border-gray-500/20",
    dot: "bg-gray-300",
    text: "text-text-muted",
  },
};

const LEVEL_ORDER: LogLevel[] = ["error", "warn", "info", "debug", "trace"];

// =========================================================
// Component
// =========================================================

interface ConsoleToolbarProps {
  filters: LogFilters;
  onToggleLevel: (level: LogLevel) => void;
  onToggleSpans: () => void;
  onSetSearchText: (text: string) => void;
  onClear: () => void;
  levelCounts: Record<LogLevel, number>;
  spanCount: number;
  totalCount: number;
  filteredCount: number;
  searchInputRef?: React.RefObject<HTMLInputElement | null>;
}

export function ConsoleToolbar({
  filters,
  onToggleLevel,
  onToggleSpans,
  onSetSearchText,
  onClear,
  levelCounts,
  spanCount,
  totalCount,
  filteredCount,
  searchInputRef,
}: ConsoleToolbarProps) {
  const localSearchRef = useRef<HTMLInputElement>(null);
  const inputRef = searchInputRef ?? localSearchRef;

  return (
    <div className="flex items-center gap-2 px-3 py-2 border-b border-border bg-surface-inset/30 shrink-0">
      {/* Level filter pills */}
      <div className="flex items-center gap-1">
        {LEVEL_ORDER.map((level) => {
          const enabled = filters.levels.has(level);
          const style = levelStyles[level];
          const count = levelCounts[level];

          return (
            <button
              key={level}
              onClick={() => onToggleLevel(level)}
              className={cn(
                "inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-semibold uppercase tracking-wider transition-all",
                "border",
                enabled
                  ? cn(style.active, style.text)
                  : "bg-transparent border-border text-text-muted opacity-50",
              )}
            >
              <span
                className={cn("w-1.5 h-1.5 rounded-full", style.dot)}
              />
              {level}
              {count > 0 && (
                <span className="ml-0.5 tabular-nums">{count}</span>
              )}
            </button>
          );
        })}

        {/* Span filter pill */}
        <button
          onClick={onToggleSpans}
          className={cn(
            "inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-semibold uppercase tracking-wider transition-all",
            "border",
            filters.showSpans
              ? "bg-cyan-500/15 border-cyan-500/30 text-cyan-400"
              : "bg-transparent border-border text-text-muted opacity-50",
          )}
        >
          <span className="w-1.5 h-1.5 rounded-full bg-cyan-500" />
          span
          {spanCount > 0 && (
            <span className="ml-0.5 tabular-nums">{spanCount}</span>
          )}
        </button>
      </div>

      {/* Divider */}
      <div className="w-px h-4 bg-border-divider" />

      {/* Entry count */}
      <span className="text-[10px] text-text-muted tabular-nums shrink-0">
        {filteredCount === totalCount
          ? `${totalCount}`
          : `${filteredCount}/${totalCount}`}
      </span>

      {/* Divider */}
      <div className="w-px h-4 bg-border-divider" />

      {/* Search input */}
      <div className="flex-1 max-w-xs">
        <input
          ref={inputRef}
          type="text"
          placeholder="Filter..."
          value={filters.searchText}
          onChange={(e) => onSetSearchText(e.target.value)}
          className="w-full h-7 px-2.5 text-xs bg-surface border border-border-input rounded-md placeholder:text-text-muted focus:outline-none focus:border-accent/50 focus:ring-1 focus:ring-accent/20"
        />
      </div>

      {/* Spacer */}
      <div className="flex-1" />

      {/* Actions */}
      <div className="flex items-center gap-1">
        {/* Clear */}
        <button
          className="p-1.5 rounded-md text-text-tertiary hover:text-text-primary hover:bg-surface-hover transition-colors"
          onClick={onClear}
          title="Clear log (⌘K)"
        >
          <TrashIcon className="w-4 h-4" />
        </button>
      </div>
    </div>
  );
}
