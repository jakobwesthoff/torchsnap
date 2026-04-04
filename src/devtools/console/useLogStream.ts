// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Log Stream Hook
//
// Manages the connection to the backend logging system:
// 1. Fetches existing items on mount (devtools_log_history)
// 2. Subscribes to live items (devtools_log_subscribe)
// 3. Merges them into a single array keyed by seq number
// 4. Builds a span depth map from SpanStart items
//
// The items array is stored in a ref (not state) to avoid
// re-renders on every append. Components consume it through
// a snapshot counter that triggers re-renders when batches
// arrive.
// =========================================================

import { useCallback, useEffect, useRef, useState } from "react";
import { Channel } from "@tauri-apps/api/core";
import { command } from "../../lib/command";
import type { DevToolsMessage, LogItem } from "../types";

export interface UseLogStreamReturn {
  /** All log items, ordered by seq. */
  items: LogItem[];
  /** Number of messages dropped due to backend backpressure. */
  droppedCount: number;
  /** Whether the stream is connected. */
  isConnected: boolean;
  /** Clear all items (both local and backend). */
  clear: () => void;
  /** Maps span_id → nesting depth, built from SpanStart items. */
  spanDepthMap: Map<number, number>;
  /** Set of span IDs that have received a SpanEnd item. */
  completedSpanIds: Set<number>;
}

/** Index span items: depths from span-starts, completed IDs from span-ends. */
function indexSpanItems(
  items: LogItem[],
  depthMap: Map<number, number>,
  completedIds: Set<number>,
) {
  for (const item of items) {
    if (item.kind.type === "spanStart") {
      depthMap.set(item.kind.spanId, item.kind.depth);
    } else if (item.kind.type === "spanEnd") {
      completedIds.add(item.kind.spanId);
    }
  }
}

export function useLogStream(): UseLogStreamReturn {
  // Items stored in a ref to avoid re-renders per item.
  // The snapshot counter forces a re-render when we want
  // consumers to pick up new items.
  const itemsRef = useRef<LogItem[]>([]);
  const depthMapRef = useRef<Map<number, number>>(new Map());
  const completedIdsRef = useRef<Set<number>>(new Set());
  const [snapshot, setSnapshot] = useState(0);
  const [droppedCount, setDroppedCount] = useState(0);
  const [isConnected, setIsConnected] = useState(false);

  useEffect(() => {
    let cancelled = false;

    async function connect() {
      // 1. Fetch existing items.
      try {
        const history = await command("devtools_log_history", {
          afterSeq: 0,
          limit: 10_000,
        });
        if (cancelled) return;
        itemsRef.current = history;
        depthMapRef.current = new Map();
        completedIdsRef.current = new Set();
        indexSpanItems(history, depthMapRef.current, completedIdsRef.current);
        setSnapshot((s) => s + 1);
      } catch (e) {
        console.warn("devtools_log_history failed:", e);
      }

      // 2. Subscribe to live items.
      const channel = new Channel<DevToolsMessage>();
      channel.onmessage = (message) => {
        if (cancelled) return;

        if (message.type === "entries") {
          indexSpanItems(message.entries, depthMapRef.current, completedIdsRef.current);
          itemsRef.current = [...itemsRef.current, ...message.entries];
          setSnapshot((s) => s + 1);
        } else if (message.type === "dropped") {
          setDroppedCount((c) => c + message.count);
        }
      };

      try {
        await command("devtools_log_subscribe", { channel });
        if (!cancelled) setIsConnected(true);
      } catch (e) {
        console.warn("devtools_log_subscribe failed:", e);
      }
    }

    connect();

    return () => {
      cancelled = true;
    };
  }, []);

  const clear = useCallback(async () => {
    try {
      await command("devtools_log_clear");
      itemsRef.current = [];
      depthMapRef.current = new Map();
      completedIdsRef.current = new Set();
      setDroppedCount(0);
      setSnapshot((s) => s + 1);
    } catch (e) {
      console.warn("devtools_log_clear failed:", e);
    }
  }, []);

  // Trigger re-read of itemsRef when snapshot changes.
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  void snapshot;

  return {
    items: itemsRef.current,
    droppedCount,
    isConnected,
    clear,
    spanDepthMap: depthMapRef.current,
    completedSpanIds: completedIdsRef.current,
  };
}
