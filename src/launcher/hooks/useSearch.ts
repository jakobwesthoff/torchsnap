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
import { Channel } from "@tauri-apps/api/core";
import { command } from "../../lib/command";
import { compareEntries } from "../compareEntries";
import {
  resultSourceKey,
  type PluginViewRef,
  type SearchMessage,
  type SourcedEntry,
} from "../../types";

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

  // Per-source accumulator — each `searchResults` message
  // replaces (not merges) the entries for its `source`. A plugin
  // transitioning from results to empty on a subsequent keystroke
  // emits an empty batch that evicts its prior contribution here,
  // which is the mechanism that fixes stale bang entries from
  // lingering after the trigger is backspaced away.
  //
  // Keyed by `resultSourceKey(message.source)` rather than the
  // raw discriminated union — a string key is what `Map` expects,
  // and the helper guarantees plugin/catalog keys can never
  // collide.
  const accumulatorRef = useRef<Map<string, SourcedEntry[]>>(new Map());

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

    // ESLINT: When the query is cleared (e.g. goBack / Escape),
    // synchronously reset ALL state so plugin views unmount on the same
    // render. Without this, stale plugin views remain mounted until the
    // async search result arrives, and their effects can re-inject
    // partial state (like a matched prefix) into the display query.
    // Moving this ref write to a useEffect would reintroduce that
    // visual glitch — there would be a render frame where the query is
    // empty but stale results are still visible.
    //
    // This is the only place where we eagerly null out view refs.
    // For non-empty query changes (typing within prefix mode), we
    // intentionally do NOT reset view refs here — the generation-
    // aware logic in the channel handler takes care of replacing
    // stale refs without a null gap.
    if (!query) {
      // eslint-disable-next-line react-hooks/refs
      accumulatorRef.current = new Map();
      setResults([]);
      setCustomPluginView(null);
      setInlinePluginView(null);
      setMatchedPrefix(null);
    }
  }

  useEffect(() => {
    const generation = ++generationRef.current;

    // Reset the per-source accumulator for the new query. We do
    // NOT reset view refs here — see the generation tracking
    // comment above for why. Resetting them here would cause a
    // null gap (effect runs → view refs null → plugin unmounts
    // → message arrives → view refs set → plugin remounts =
    // visible flash).
    accumulatorRef.current = new Map();

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
          // Per-source replacement: the batch replaces (not
          // merges with) whatever this source contributed on the
          // previous message for this query. That lets a plugin
          // evict its prior entries by emitting an empty batch.
          const key = resultSourceKey(message.source);
          const prev = accumulatorRef.current.get(key) ?? [];
          const next = message.entries;
          const entriesChanged = !(prev.length === 0 && next.length === 0);

          if (entriesChanged) {
            if (next.length === 0) {
              accumulatorRef.current.delete(key);
            } else {
              accumulatorRef.current.set(key, next);
            }

            // Recompute the flat sorted list across every
            // source. The per-source batches arrive pre-sorted
            // within themselves, but sources are independent,
            // so we sort the flattened view. At launcher scale
            // (tens of entries per source, ≤10 sources) this
            // is well under a millisecond; a k-way merge would
            // only matter if either bound grew substantially.
            const flat: SourcedEntry[] = [];
            for (const batch of accumulatorRef.current.values()) {
              flat.push(...batch);
            }
            flat.sort(compareEntries);
            setResults(flat);
          }
          // Empty-to-empty short-circuits the `setResults`
          // call — no state change, no re-render. The
          // view-ref update still runs below because an
          // empty-entries batch can still carry a view
          // reference (e.g., a plugin that surfaces its UI
          // via `inline-ui` with no list entries).

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

    command("search", { query, onResults: channel });

    // When the query changes, silence the old channel so its closure
    // (and the state setters it captures) can be garbage-collected
    // once the backend finishes sending on it.
    return () => {
      channel.onmessage = () => {};
    };
  }, [query]);

  return { results, customPluginView, inlinePluginView, matchedPrefix, loading };
}
