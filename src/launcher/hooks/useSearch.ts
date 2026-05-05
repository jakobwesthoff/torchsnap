// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Search hook — streams results from the Rust search pipeline
 * via a Tauri channel, one `searchResults` message per source
 * (catalog layer + each query plugin).
 *
 * Each message replaces that source's entries in a per-source
 * Map; the displayed list is always the flattened sorted view
 * of the Map. A source emitting an empty batch evicts its
 * prior contribution — that's how a plugin transitioning from
 * results to empty (e.g. bangs losing its trigger mid-query)
 * disappears from the list atomically.
 *
 * A generation counter discards messages from superseded
 * queries.
 */

import { useEffect, useRef, useState } from "react";
import { Channel } from "@tauri-apps/api/core";
import { command } from "../../lib/command";
import { compareEntries } from "../compareEntries";
import {
  resultSourceKey,
  type GadgetViewRef,
  type SearchMessage,
  type SourcedEntry,
} from "../../types";

interface UseSearchResult {
  results: SourcedEntry[];
  /** View reference when the active plugin requested custom UI. */
  customGadgetView: GadgetViewRef | null;
  /** View reference when the active plugin requested inline UI. */
  inlineGadgetView: GadgetViewRef | null;
  /** The prefix that triggered exclusive routing (e.g., ":"). */
  matchedPrefix: string | null;
  loading: boolean;
}

export function useSearch(query: string): UseSearchResult {
  const [results, setResults] = useState<SourcedEntry[]>([]);
  const [customGadgetView, setCustomGadgetView] = useState<GadgetViewRef | null>(null);
  const [inlineGadgetView, setInlineGadgetView] = useState<GadgetViewRef | null>(null);
  const [matchedPrefix, setMatchedPrefix] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const generationRef = useRef(0);

  // Per-source accumulator: each `searchResults` replaces its
  // source's batch. The displayed list is always the flattened
  // sorted view of these values.
  const accumulatorRef = useRef<Map<string, SourcedEntry[]>>(new Map());

  // =========================================================
  // View ref generation tracking
  //
  // View refs (customGadgetView, inlineGadgetView, matchedPrefix)
  // need different handling than entries — entries get replaced
  // atomically from the accumulator on every message, but view
  // refs accumulate "first non-null wins" within a generation
  // so the catalog's null views don't clobber a pending inline
  // view from a later plugin message. The `isFirstMessageOfGeneration`
  // flag flips that rule for the first message, which
  // unconditionally overwrites even with null so stale refs
  // from the previous generation don't leak through.
  // =========================================================

  // Signal loading=true during render to avoid the extra
  // render cycle a useEffect-based setState would cost.
  const [prevQuery, setPrevQuery] = useState(query);
  if (prevQuery !== query) {
    setPrevQuery(query);
    setLoading(true);

    // Empty query (Escape / goBack) needs a synchronous reset
    // so plugin view components unmount on the same render and
    // their effects don't re-inject a matched-prefix echo into
    // the display query. For non-empty query changes the
    // channel handler replaces the accumulator atomically on
    // the first message, so no synchronous clear is needed
    // there (and avoiding one prevents a flash frame).
    if (!query) {
      // eslint-disable-next-line react-hooks/refs
      accumulatorRef.current = new Map();
      setResults([]);
      setCustomGadgetView(null);
      setInlineGadgetView(null);
      setMatchedPrefix(null);
    }
  }

  useEffect(() => {
    const generation = ++generationRef.current;
    accumulatorRef.current = new Map();
    let isFirstMessageOfGeneration = true;

    const channel = new Channel<SearchMessage>();

    channel.onmessage = (message) => {
      if (generationRef.current !== generation) return;

      switch (message.type) {
        case "searchResults": {
          const key = resultSourceKey(message.source);
          if (message.entries.length === 0) {
            accumulatorRef.current.delete(key);
          } else {
            accumulatorRef.current.set(key, message.entries);
          }

          // OPTIMIZATION POTENTIAL: flatten + full sort on
          // every message is O(n log n) in total entry count
          // per message, where n can realistically reach
          // thousands (emoji-picker, app-launcher catalogs).
          // Each incoming batch is already pre-sorted, and
          // only one source's batch changes per message, so a
          // k-way merge across the Map's sorted batches would
          // cut this to O(n). Swap it in if the render path
          // shows up in profiling.
          const flat: SourcedEntry[] = [];
          for (const batch of accumulatorRef.current.values()) {
            flat.push(...batch);
          }
          flat.sort(compareEntries);
          setResults(flat);

          if (isFirstMessageOfGeneration) {
            setCustomGadgetView(message.customGadgetView);
            setInlineGadgetView(message.inlineGadgetView);
            setMatchedPrefix(message.matchedPrefix);
            isFirstMessageOfGeneration = false;
          } else {
            if (message.customGadgetView != null) {
              setCustomGadgetView((prev) => prev ?? message.customGadgetView);
            }
            if (message.inlineGadgetView != null) {
              setInlineGadgetView((prev) => prev ?? message.inlineGadgetView);
            }
            if (message.matchedPrefix != null) {
              setMatchedPrefix((prev) => prev ?? message.matchedPrefix);
            }
          }
          break;
        }
        case "done":
          setLoading(false);
          break;
      }
    };

    command("search", { query, onResults: channel });

    return () => {
      channel.onmessage = () => {};
    };
  }, [query]);

  return { results, customGadgetView, inlineGadgetView, matchedPrefix, loading };
}
