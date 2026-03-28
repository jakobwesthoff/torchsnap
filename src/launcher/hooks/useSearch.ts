// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Search hook — calls the Rust search command on every query change
 * and streams results via a Tauri channel.
 *
 * Returns the current result list and a loading flag. Results update
 * progressively as channel messages arrive (currently only catalog
 * results, but query plugin results will stream in later).
 *
 * A generation counter guards against stale results: if the query
 * changes before the previous search completes, the old results are
 * discarded.
 */

import { useEffect, useRef, useState } from "react";
import { invoke, Channel } from "@tauri-apps/api/core";
import type { ScoredEntry, SearchMessage } from "../types";

interface UseSearchResult {
  results: ScoredEntry[];
  /** Plugin ID when the active plugin requested custom UI. */
  customPluginView: string | null;
  /** The prefix that triggered exclusive routing (e.g., ":"). */
  matchedPrefix: string | null;
  loading: boolean;
}

export function useSearch(query: string): UseSearchResult {
  const [results, setResults] = useState<ScoredEntry[]>([]);
  const [customPluginView, setCustomPluginView] = useState<string | null>(null);
  const [matchedPrefix, setMatchedPrefix] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const generationRef = useRef(0);

  useEffect(() => {
    const generation = ++generationRef.current;
    setLoading(true);

    const channel = new Channel<SearchMessage>();

    channel.onmessage = (message) => {
      // Discard results from a superseded query.
      if (generationRef.current !== generation) return;

      switch (message.type) {
        case "catalogResults":
          setResults(message.entries);
          setCustomPluginView(message.customPluginView);
          setMatchedPrefix(message.matchedPrefix);
          break;
        case "done":
          setLoading(false);
          break;
      }
    };

    invoke("search_query", { query, onResults: channel });
  }, [query]);

  return { results, customPluginView, matchedPrefix, loading };
}
