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
import { compareEntries } from "../compareEntries";
import type { PluginViewRef, SourcedEntry, SearchMessage } from "../types";

interface UseSearchResult {
  results: SourcedEntry[];
  /** View reference when the active plugin requested custom UI. */
  customPluginView: PluginViewRef | null;
  /** View reference when the active plugin requested inline UI. */
  inlinePluginView: PluginViewRef | null;
  /** The prefix that triggered exclusive routing (e.g., ":"). */
  matchedPrefix: string | null;
  loading: boolean;
}

export function useSearch(query: string): UseSearchResult {
  const [results, setResults] = useState<SourcedEntry[]>([]);
  const [customPluginView, setCustomPluginView] = useState<PluginViewRef | null>(null);
  const [inlinePluginView, setInlinePluginView] = useState<PluginViewRef | null>(null);
  const [matchedPrefix, setMatchedPrefix] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const generationRef = useRef(0);

  // Accumulator ref — holds the current merged result array so
  // the channel callback can merge into it without stale closures.
  const accumulatorRef = useRef<SourcedEntry[]>([]);

  // =========================================================
  // View ref generation tracking
  //
  // View refs (customPluginView, inlinePluginView, matchedPrefix)
  // need different handling than entries:
  //
  // WITHIN a generation (multiple messages for one query):
  //   First non-null wins. Catalog results arrive first with null
  //   views, then a query plugin may arrive with an inline view.
  //   We don't want the catalog's null to overwrite a pending
  //   inline view that hasn't arrived yet.
  //
  // ACROSS generations (new keystroke / query change):
  //   The old view refs are stale and must be replaced. But we
  //   must NOT eagerly reset to null — that would cause a visible
  //   flash (plugin view unmounts for one frame, then remounts
  //   when the new message arrives).
  //
  // Solution: track the generation that last wrote each view ref.
  // On the first message of a new generation, unconditionally
  // overwrite (even with null — handles transitions like prefix
  // mode → non-prefix mode). On subsequent messages within the
  // same generation, only overwrite if the current value is null
  // (first-non-null-wins).
  //
  // This gives us:
  // - Prefix mode: first (and only) message carries the updated
  //   custom view → overwrites immediately, no null gap, no flash.
  // - Non-prefix mode: catalog message is first → clears stale
  //   custom view. Query plugin message arrives later → first-
  //   non-null sets inline view.
  // - Transition from prefix to non-prefix: catalog message is
  //   first → correctly clears the stale custom view (different
  //   generation, unconditional overwrite).
  // - Query cleared: the synchronous !query reset below handles
  //   this case before any effect runs — no stale views persist.
  // =========================================================

  const viewRefGenerationRef = useRef(0);

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
    // reset ALL state so plugin views unmount on the same render.
    // Without this, stale plugin views remain mounted until the async
    // search result arrives, and their effects can re-inject partial
    // state (like a matched prefix) into the display query.
    //
    // This is the only place where we eagerly null out view refs.
    // For non-empty query changes (typing within prefix mode), we
    // intentionally do NOT reset view refs here — the generation-
    // aware logic in the channel handler takes care of replacing
    // stale refs without a null gap.
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

    // Reset the entry accumulator for the new query. We do NOT
    // reset view refs here — see the generation tracking comment
    // above for why. Resetting them here would cause a null gap
    // (effect runs → view refs null → plugin unmounts → message
    // arrives → view refs set → plugin remounts = visible flash).
    accumulatorRef.current = [];

    // Track whether this generation has delivered its first
    // message yet. Used to decide between "unconditional
    // overwrite" (first message) and "first-non-null wins"
    // (subsequent messages).
    let isFirstMessageOfGeneration = true;

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

          // -------------------------------------------------
          // View ref update logic
          //
          // On the first message of a new generation: always
          // overwrite, even with null. This handles:
          //   - Prefix mode: replaces stale data from the
          //     previous keystroke with fresh data.
          //   - Transition to non-prefix: catalog message has
          //     null views, correctly clearing the old prefix
          //     view.
          //
          // On subsequent messages within the same generation:
          // only set if currently null (first-non-null wins).
          // This handles:
          //   - Non-prefix mode: catalog (null views) arrives
          //     first, then query plugin (inline view) arrives
          //     second. The plugin's view is not clobbered by
          //     the catalog's null.
          // -------------------------------------------------

          if (isFirstMessageOfGeneration) {
            // Unconditional overwrite — replace whatever the
            // previous generation left behind.
            setCustomPluginView(message.customPluginView);
            setInlinePluginView(message.inlinePluginView);
            setMatchedPrefix(message.matchedPrefix);

            // Record that this generation has now written its
            // view refs so the viewRefGenerationRef stays
            // accurate for external consumers if needed.
            viewRefGenerationRef.current = generation;
            isFirstMessageOfGeneration = false;
          } else {
            // Within-generation accumulation: first non-null
            // wins. Only overwrite if the current value is
            // null (hasn't been set yet this generation).
            if (message.customPluginView != null) {
              setCustomPluginView((prev) => prev ?? message.customPluginView);
            }
            if (message.inlinePluginView != null) {
              setInlinePluginView((prev) => prev ?? message.inlinePluginView);
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
