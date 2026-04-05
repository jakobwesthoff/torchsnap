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
// Uses useSyncExternalStore to bridge the mutable backing
// store (items array, span maps) into React's rendering
// model. The store emits new snapshots when data arrives;
// React re-renders only when the snapshot identity changes.
// =========================================================

import { useCallback, useState, useSyncExternalStore } from "react";
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

// =========================================================
// Snapshot — immutable value returned by getSnapshot
// =========================================================

interface LogStreamSnapshot {
  items: LogItem[];
  droppedCount: number;
  isConnected: boolean;
  spanDepthMap: Map<number, number>;
  completedSpanIds: Set<number>;
}

// =========================================================
// Mutable backing store
//
// Holds the actual data and a list of subscribers to notify
// when the data changes. useSyncExternalStore calls subscribe
// once and getSnapshot on every render — React re-renders
// only when the snapshot reference changes.
// =========================================================

class LogStreamStore {
  private items: LogItem[] = [];
  private droppedCount = 0;
  private isConnected = false;
  private depthMap = new Map<number, number>();
  private completedIds = new Set<number>();

  private snapshot: LogStreamSnapshot = this.buildSnapshot();
  private listeners = new Set<() => void>();

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  getSnapshot = (): LogStreamSnapshot => {
    return this.snapshot;
  };

  // --- Mutations (called from effects / callbacks) ---

  setHistory(items: LogItem[]) {
    this.items = items;
    this.depthMap = new Map();
    this.completedIds = new Set();
    this.indexSpanItems(items);
    this.emit();
  }

  appendEntries(entries: LogItem[]) {
    this.indexSpanItems(entries);
    this.items = [...this.items, ...entries];
    this.emit();
  }

  addDropped(count: number) {
    this.droppedCount += count;
    this.emit();
  }

  setConnected(connected: boolean) {
    this.isConnected = connected;
    this.emit();
  }

  clear() {
    this.items = [];
    this.droppedCount = 0;
    this.depthMap = new Map();
    this.completedIds = new Set();
    this.emit();
  }

  // --- Internal ---

  private indexSpanItems(items: LogItem[]) {
    for (const item of items) {
      if (item.kind.type === "spanStart") {
        this.depthMap.set(item.kind.spanId, item.kind.depth);
      } else if (item.kind.type === "spanEnd") {
        this.completedIds.add(item.kind.spanId);
      }
    }
  }

  private buildSnapshot(): LogStreamSnapshot {
    return {
      items: this.items,
      droppedCount: this.droppedCount,
      isConnected: this.isConnected,
      spanDepthMap: this.depthMap,
      completedSpanIds: this.completedIds,
    };
  }

  private emit() {
    this.snapshot = this.buildSnapshot();
    for (const listener of this.listeners) {
      listener();
    }
  }
}

// =========================================================
// Hook
// =========================================================

export function useLogStream(): UseLogStreamReturn {
  // Stable store instance — survives across renders. useState
  // with an initializer function runs exactly once and does not
  // involve ref access during render.
  const [store] = useState(() => new LogStreamStore());

  // subscribe is called once by useSyncExternalStore. It sets
  // up the backend connection and returns a cleanup function.
  const subscribe = useCallback(
    (onStoreChange: () => void) => {
      const unsubscribe = store.subscribe(onStoreChange);
      let cancelled = false;

      async function connect() {
        // 1. Fetch existing items.
        try {
          const history = await command("devtools_log_history", {
            afterSeq: 0,
            limit: 10_000,
          });
          if (cancelled) return;
          store.setHistory(history);
        } catch (e) {
          console.warn("devtools_log_history failed:", e);
        }

        // 2. Subscribe to live items.
        const channel = new Channel<DevToolsMessage>();
        channel.onmessage = (message) => {
          if (cancelled) return;

          if (message.type === "entries") {
            store.appendEntries(message.entries);
          } else if (message.type === "dropped") {
            store.addDropped(message.count);
          }
        };

        try {
          await command("devtools_log_subscribe", { channel });
          if (!cancelled) store.setConnected(true);
        } catch (e) {
          console.warn("devtools_log_subscribe failed:", e);
        }
      }

      connect();

      return () => {
        cancelled = true;
        unsubscribe();
      };
    },
    [store],
  );

  const getSnapshot = useCallback(() => store.getSnapshot(), [store]);
  const snapshot = useSyncExternalStore(subscribe, getSnapshot);

  const clear = useCallback(async () => {
    try {
      await command("devtools_log_clear");
      store.clear();
    } catch (e) {
      console.warn("devtools_log_clear failed:", e);
    }
  }, [store]);

  return {
    items: snapshot.items,
    droppedCount: snapshot.droppedCount,
    isConnected: snapshot.isConnected,
    clear,
    spanDepthMap: snapshot.spanDepthMap,
    completedSpanIds: snapshot.completedSpanIds,
  };
}
