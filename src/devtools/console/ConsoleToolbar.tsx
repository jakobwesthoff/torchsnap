// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Console Toolbar
//
// Filter bar with level pills, span pill, source dropdown,
// search input, and action buttons. Groups are separated by
// thin vertical dividers.
// =========================================================

import { useEffect, useRef, useState } from "react";
import { Bars3Icon, ChevronDownIcon, FunnelIcon, QueueListIcon, TrashIcon } from "@heroicons/react/24/outline";
import { cn } from "../../lib/cn";
import type { LogLevel } from "../types";
import type { LogFilters } from "./useLogFilters";
import { PLUGIN_COLORS, pluginColorIndex } from "./pluginColors";

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
// Source Filter Dropdown
// =========================================================

interface SourceDropdownProps {
  filters: LogFilters;
  knownSources: string[];
  onToggleSource: (source: string) => void;
  onSetAllSources: () => void;
}

function SourceDropdown({
  filters,
  knownSources,
  onToggleSource,
  onSetAllSources,
}: SourceDropdownProps) {
  const [open, setOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  // Close on outside click.
  useEffect(() => {
    if (!open) return;
    function handleClick(e: MouseEvent) {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    }
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [open]);

  const isAllSources = filters.sources === "all";
  const selectedCount =
    filters.sources === "all" ? knownSources.length : filters.sources.size;

  let label: string;
  if (isAllSources) {
    label = "All sources";
  } else if (selectedCount === 1) {
    const selected = filters.sources as Set<string>;
    label = Array.from(selected)[0];
  } else {
    label = `${selectedCount} sources`;
  }

  return (
    <div ref={containerRef} className="relative">
      <button
        onClick={() => setOpen(!open)}
        className={cn(
          "inline-flex items-center gap-1.5 px-2.5 py-1 text-xs transition-colors",
          "bg-surface border border-border-input rounded-md hover:border-border-hover",
          !isAllSources ? "text-text-primary" : "text-text-secondary",
        )}
      >
        <FunnelIcon className="w-3.5 h-3.5" />
        <span className="max-w-[120px] truncate">{label}</span>
        <ChevronDownIcon className={cn("w-3 h-3 text-text-muted transition-transform", open && "rotate-180")} />
      </button>

      {open && (
        <div className="absolute top-full left-0 mt-1 z-30 min-w-[180px] py-1 bg-surface border border-border rounded-lg shadow-lg">
          {/* All sources option */}
          <button
            onClick={() => { onSetAllSources(); setOpen(false); }}
            className={cn(
              "flex items-center gap-2 w-full px-3 py-1.5 text-xs text-left hover:bg-surface-hover transition-colors",
              isAllSources && "text-text-primary font-medium",
              !isAllSources && "text-text-secondary",
            )}
          >
            <span className={cn(
              "w-3.5 h-3.5 flex items-center justify-center rounded border",
              isAllSources ? "bg-accent border-accent" : "border-border-input",
            )}>
              {isAllSources && (
                <svg className="w-2.5 h-2.5 text-white" fill="none" viewBox="0 0 24 24" strokeWidth={3} stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" d="m4.5 12.75 6 6 9-13.5" />
                </svg>
              )}
            </span>
            All sources
          </button>

          <div className="mx-2 my-1 border-t border-border-divider" />

          {/* Individual sources */}
          {knownSources.map((source) => {
            const checked = isAllSources || (filters.sources !== "all" && filters.sources.has(source));
            const isPlugin = source !== "host";

            return (
              <button
                key={source}
                onClick={() => onToggleSource(source)}
                className="flex items-center gap-2 w-full px-3 py-1.5 text-xs text-left text-text-primary hover:bg-surface-hover transition-colors"
              >
                <span className={cn(
                  "w-3.5 h-3.5 flex items-center justify-center rounded border",
                  checked ? "bg-accent border-accent" : "border-border-input",
                )}>
                  {checked && (
                    <svg className="w-2.5 h-2.5 text-white" fill="none" viewBox="0 0 24 24" strokeWidth={3} stroke="currentColor">
                      <path strokeLinecap="round" strokeLinejoin="round" d="m4.5 12.75 6 6 9-13.5" />
                    </svg>
                  )}
                </span>
                {isPlugin && (
                  <span className={cn(
                    "w-1.5 h-1.5 rounded-full shrink-0",
                    PLUGIN_COLORS[pluginColorIndex(source)],
                  )} />
                )}
                <span className={cn(!isPlugin && "italic text-text-secondary")}>
                  {source}
                </span>
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}

// =========================================================
// ConsoleToolbar
// =========================================================

export type ViewMode = "flat" | "tree";

interface ConsoleToolbarProps {
  filters: LogFilters;
  onToggleLevel: (level: LogLevel) => void;
  onToggleSpans: () => void;
  onToggleSource: (source: string) => void;
  onSetAllSources: () => void;
  onSetSearchText: (text: string) => void;
  onClear: () => void;
  levelCounts: Record<LogLevel, number>;
  spanCount: number;
  knownSources: string[];
  totalCount: number;
  filteredCount: number;
  searchInputRef?: React.RefObject<HTMLInputElement | null>;
  viewMode: ViewMode;
  onSetViewMode: (mode: ViewMode) => void;
}

export function ConsoleToolbar({
  filters,
  onToggleLevel,
  onToggleSpans,
  onToggleSource,
  onSetAllSources,
  onSetSearchText,
  onClear,
  levelCounts,
  spanCount,
  knownSources,
  totalCount,
  filteredCount,
  searchInputRef,
  viewMode,
  onSetViewMode,
}: ConsoleToolbarProps) {
  const localSearchRef = useRef<HTMLInputElement>(null);
  const inputRef = searchInputRef ?? localSearchRef;

  return (
    <div className="shrink-0 border-b border-border bg-surface-inset/30">
      {/* Row 1: Level + span filter pills, clear action right-aligned */}
      <div className="flex items-center gap-1 px-3 pt-2 pb-1.5">
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

        {/* Spacer pushes clear to the right */}
        <div className="flex-1" />

        {/* Clear action */}
        <button
          className="p-1.5 rounded-md text-text-tertiary hover:text-text-primary hover:bg-surface-hover transition-colors"
          onClick={onClear}
          title="Clear log (⌘K)"
        >
          <TrashIcon className="w-4 h-4" />
        </button>
      </div>

      {/* Row 2: Source filter, count, view mode, search, actions */}
      <div className="flex items-center gap-2 px-3 pb-2">
        {/* Source filter dropdown */}
        <SourceDropdown
          filters={filters}
          knownSources={knownSources}
          onToggleSource={onToggleSource}
          onSetAllSources={onSetAllSources}
        />

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

        {/* Search input — fills remaining space */}
        <div className="flex-1">
          <input
            ref={inputRef}
            type="text"
            placeholder="Filter..."
            value={filters.searchText}
            onChange={(e) => onSetSearchText(e.target.value)}
            className="w-full h-7 px-2.5 text-xs bg-surface border border-border-input rounded-md placeholder:text-text-muted focus:outline-none focus:border-accent/50 focus:ring-1 focus:ring-accent/20"
          />
        </div>

        {/* Divider */}
        <div className="w-px h-4 bg-border-divider" />

        {/* View mode toggle */}
        <div className="flex items-center rounded-md border border-border-input overflow-hidden">
          <button
            onClick={() => onSetViewMode("flat")}
            className={cn(
              "px-1.5 py-1 transition-colors",
              viewMode === "flat"
                ? "bg-accent/15 text-accent"
                : "text-text-muted hover:text-text-secondary",
            )}
            title="Flat view"
          >
            <Bars3Icon className="w-3.5 h-3.5" />
          </button>
          <button
            onClick={() => onSetViewMode("tree")}
            className={cn(
              "px-1.5 py-1 transition-colors",
              viewMode === "tree"
                ? "bg-accent/15 text-accent"
                : "text-text-muted hover:text-text-secondary",
            )}
            title="Tree view"
          >
            <QueueListIcon className="w-3.5 h-3.5" />
          </button>
        </div>
      </div>
    </div>
  );
}
