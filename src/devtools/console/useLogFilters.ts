// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Log Filter Hook
//
// Manages filter state and provides a filtered view of log
// entries. Filtering is done entirely on the frontend — the
// backend always sends all entries.
// =========================================================

import { useCallback, useMemo, useState } from "react";
import type { LogEntry, LogLevel } from "../types";

const ALL_LEVELS: LogLevel[] = ["trace", "debug", "info", "warn", "error"];

export interface LogFilters {
  levels: Set<LogLevel>;
  sources: Set<string> | "all";
  searchText: string;
}

export interface UseLogFiltersReturn {
  filters: LogFilters;
  filteredEntries: LogEntry[];
  toggleLevel: (level: LogLevel) => void;
  toggleSource: (source: string) => void;
  setAllSources: () => void;
  setSearchText: (text: string) => void;
  /** Count of entries per level (unfiltered). */
  levelCounts: Record<LogLevel, number>;
  /** Set of all known plugin sources. */
  knownSources: string[];
}

export function useLogFilters(entries: LogEntry[]): UseLogFiltersReturn {
  const [levels, setLevels] = useState<Set<LogLevel>>(
    () => new Set(ALL_LEVELS),
  );
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

  const toggleSource = useCallback((source: string) => {
    setSources((prev) => {
      if (prev === "all") {
        // Switching from "all" to a specific exclusion: include
        // all known sources except the toggled one. We'll need
        // the known sources list, but for simplicity we just
        // create a set with the single source removed.
        // Actually, toggling from "all" should select only this one.
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

  // Compute known sources from entries.
  const knownSources = useMemo(() => {
    const seen = new Set<string>();
    for (const entry of entries) {
      if (entry.source.type === "plugin") {
        seen.add(entry.source.value);
      } else {
        seen.add("host");
      }
    }
    return Array.from(seen).sort();
  }, [entries]);

  // Count entries per level (unfiltered).
  const levelCounts = useMemo(() => {
    const counts: Record<LogLevel, number> = {
      trace: 0,
      debug: 0,
      info: 0,
      warn: 0,
      error: 0,
    };
    for (const entry of entries) {
      counts[entry.level]++;
    }
    return counts;
  }, [entries]);

  // Apply filters.
  const filteredEntries = useMemo(() => {
    const lowerSearch = searchText.toLowerCase();

    return entries.filter((entry) => {
      // Level filter.
      if (!levels.has(entry.level)) return false;

      // Source filter.
      if (sources !== "all") {
        const sourceKey =
          entry.source.type === "plugin" ? entry.source.value : "host";
        if (!sources.has(sourceKey)) return false;
      }

      // Text search.
      if (lowerSearch && !entry.message.toLowerCase().includes(lowerSearch)) {
        return false;
      }

      return true;
    });
  }, [entries, levels, sources, searchText]);

  return {
    filters: { levels, sources, searchText },
    filteredEntries,
    toggleLevel,
    toggleSource,
    setAllSources,
    setSearchText,
    levelCounts,
    knownSources,
  };
}
