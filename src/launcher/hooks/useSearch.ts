// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Search hook — calls the Rust search command on every query change
 * and streams results via a Tauri channel.
 *
 * Results arrive progressively: catalog results first, then query
 * plugin results as each plugin completes. Each `searchResults`
 * message is merged into the accumulated sorted array using a
 * sorted merge (entries arrive pre-sorted from the backend).
 *
 * A generation counter guards against stale results: if the query
 * changes before the previous search completes, the old results are
 * discarded.
 */

import { useEffect, useRef, useState } from "react";
import { invoke, Channel } from "@tauri-apps/api/core";
import { sortedMerge } from "../../lib/sortedMerge";
import type { PluginViewRef, ScoredEntry, SearchMessage } from "../types";

/**
 * Comparator for the deterministic sort order used across the
 * entire search pipeline: score descending, source ascending,
 * id ascending. Both the backend and this merge use the same key.
 */
function compareEntries(a: ScoredEntry, b: ScoredEntry): number {
  if (b.score !== a.score) return b.score - a.score;
  if (a.source < b.source) return -1;
  if (a.source > b.source) return 1;
  if (a.id < b.id) return -1;
  if (a.id > b.id) return 1;
  return 0;
}

interface UseSearchResult {
  results: ScoredEntry[];
  /** View reference when the active plugin requested custom UI. */
  customPluginView: PluginViewRef | null;
  /** View reference when the active plugin requested inline UI. */
  inlinePluginView: PluginViewRef | null;
  /** The prefix that triggered exclusive routing (e.g., ":"). */
  matchedPrefix: string | null;
  loading: boolean;
}

export function useSearch(query: string): UseSearchResult {
  const [results, setResults] = useState<ScoredEntry[]>([]);
  const [customPluginView, setCustomPluginView] = useState<PluginViewRef | null>(null);
  const [inlinePluginView, setInlinePluginView] = useState<PluginViewRef | null>(null);
  const [matchedPrefix, setMatchedPrefix] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const generationRef = useRef(0);

  // Accumulator ref — holds the current merged result array so
  // the channel callback can merge into it without stale closures.
  const accumulatorRef = useRef<ScoredEntry[]>([]);

  // Signal loading=true as soon as the query changes, during the same
  // render that receives the new query. This is separated from the
  // effect below because setting state inside a useEffect would cause
  // an extra render cycle (render → effect → setState → render again).
  // The effect is still responsible for clearing loading=false once
  // the backend sends a "done" message.
  const [prevQuery, setPrevQuery] = useState(query);
  if (prevQuery !== query) {
    setPrevQuery(query);
    setLoading(true);

    // When the query is cleared (e.g. goBack / Escape), synchronously
    // reset plugin state so plugin views unmount on the same render.
    // Without this, stale plugin views remain mounted until the async
    // search result arrives, and their effects can re-inject partial
    // state (like a matched prefix) into the display query.
    if (!query) {
      accumulatorRef.current = [];
      setResults([]);
      setCustomPluginView(null);
      setInlinePluginView(null);
      setMatchedPrefix(null);
    }
  }

  useEffect(() => {
    const generation = ++generationRef.current;

    // Reset accumulator and view state for the new query. This
    // ensures stale view refs from the previous generation don't
    // block incoming updates (the "first non-null wins" logic
    // below only applies within a single generation).
    accumulatorRef.current = [];
    setCustomPluginView(null);
    setInlinePluginView(null);
    setMatchedPrefix(null);

    const channel = new Channel<SearchMessage>();

    channel.onmessage = (message) => {
      // Discard results from a superseded query.
      if (generationRef.current !== generation) return;

      switch (message.type) {
        case "searchResults": {
          // Merge incoming pre-sorted entries into the accumulator.
          const merged = sortedMerge(
            accumulatorRef.current,
            message.entries,
            compareEntries,
          );
          accumulatorRef.current = merged;
          setResults(merged);

          // First non-null view refs win within this generation.
          if (message.customPluginView != null) {
            setCustomPluginView((prev) => prev ?? message.customPluginView);
          }
          if (message.inlinePluginView != null) {
            setInlinePluginView((prev) => prev ?? message.inlinePluginView);
          }
          if (message.matchedPrefix != null) {
            setMatchedPrefix((prev) => prev ?? message.matchedPrefix);
          }
          break;
        }
        case "done":
          setLoading(false);
          break;
      }
    };

    invoke("search", { query, onResults: channel });

    // When the query changes, silence the old channel so its closure
    // (and the state setters it captures) can be garbage-collected
    // once the backend finishes sending on it.
    return () => {
      channel.onmessage = () => {};
    };
  }, [query]);

  return { results, customPluginView, inlinePluginView, matchedPrefix, loading };
}
