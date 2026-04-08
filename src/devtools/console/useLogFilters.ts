// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Log Filter Hook
//
// Manages filter state and provides a filtered view of log
// items. Filtering is done entirely on the frontend — the
// backend always sends all items.
// =========================================================

import { useCallback, useMemo, useState } from "react";
import type { LogItem, LogLevel } from "../types";

const ALL_LEVELS: LogLevel[] = ["trace", "debug", "info", "warn", "error"];

export interface LogFilters {
  levels: Set<LogLevel>;
  showSpans: boolean;
  sources: Set<string> | "all";
  searchText: string;
}

export interface UseLogFiltersReturn {
  filters: LogFilters;
  filteredItems: LogItem[];
  toggleLevel: (level: LogLevel) => void;
  toggleSpans: () => void;
  toggleSource: (source: string) => void;
  setAllSources: () => void;
  setSearchText: (text: string) => void;
  /** Count of items per level (unfiltered, excludes spans). */
  levelCounts: Record<LogLevel, number>;
  /** Count of completed spans (unfiltered, spanEnd only). */
  spanCount: number;
  /** Set of all known plugin sources. */
  knownSources: string[];
}

export function useLogFilters(items: LogItem[]): UseLogFiltersReturn {
  const [levels, setLevels] = useState<Set<LogLevel>>(() => new Set(ALL_LEVELS));
  const [showSpans, setShowSpans] = useState(true);
  const [sources, setSources] = useState<Set<string> | "all">("all");
  const [searchText, setSearchText] = useState("");

  const toggleLevel = useCallback((level: LogLevel) => {
    setLevels((prev) => {
      const next = new Set(prev);
      if (next.has(level)) {
        next.delete(level);
      } else {
        next.add(level);
      }
      return next;
    });
  }, []);

  const toggleSpans = useCallback(() => {
    setShowSpans((prev) => !prev);
  }, []);

  const toggleSource = useCallback((source: string) => {
    setSources((prev) => {
      if (prev === "all") {
        return new Set([source]);
      }
      const next = new Set(prev);
      if (next.has(source)) {
        next.delete(source);
        if (next.size === 0) return "all";
      } else {
        next.add(source);
      }
      return next;
    });
  }, []);

  const setAllSources = useCallback(() => {
    setSources("all");
  }, []);

  // Compute known sources from items.
  const knownSources = useMemo(() => {
    const seen = new Set<string>();
    for (const item of items) {
      if (item.source.type === "plugin") {
        seen.add(item.source.value);
      } else {
        seen.add("host");
      }
    }
    return Array.from(seen).sort();
  }, [items]);

  // Count items per level and spans (unfiltered).
  // Only spanEnd entries count toward spanCount (represents
  // completed spans, not individual span entries).
  const { levelCounts, spanCount } = useMemo(() => {
    const counts: Record<LogLevel, number> = {
      trace: 0,
      debug: 0,
      info: 0,
      warn: 0,
      error: 0,
    };
    let spans = 0;
    for (const item of items) {
      if (item.kind.type === "message") {
        counts[item.kind.level]++;
      } else if (item.kind.type === "spanEnd") {
        spans++;
      }
      // spanStart entries are not counted — they don't
      // represent a completed span.
    }
    return { levelCounts: counts, spanCount: spans };
  }, [items]);

  // Apply filters.
  const filteredItems = useMemo(() => {
    const lowerSearch = searchText.toLowerCase();

    return items.filter((item) => {
      const { kind } = item;

      // Span filter — span entries are a separate category.
      if (kind.type === "spanStart" || kind.type === "spanEnd") {
        if (!showSpans) return false;
      } else {
        // Level filter (only for message items).
        if (!levels.has(kind.level)) return false;
      }

      // Source filter.
      if (sources !== "all") {
        const sourceKey = item.source.type === "plugin" ? item.source.value : "host";
        if (!sources.has(sourceKey)) return false;
      }

      // Text search — match against message for messages,
      // name for span entries.
      if (lowerSearch) {
        const text = kind.type === "message" ? kind.message : kind.name;
        if (!text.toLowerCase().includes(lowerSearch)) {
          return false;
        }
      }

      return true;
    });
  }, [items, levels, showSpans, sources, searchText]);

  return {
    filters: { levels, showSpans, sources, searchText },
    filteredItems,
    toggleLevel,
    toggleSpans,
    toggleSource,
    setAllSources,
    setSearchText,
    levelCounts,
    spanCount,
    knownSources,
  };
}
