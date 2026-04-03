// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Log Stream Hook
//
// Manages the connection to the backend logging system:
// 1. Fetches existing entries on mount (devtools_log_history)
// 2. Subscribes to live entries (devtools_log_subscribe)
// 3. Merges them into a single array keyed by seq number
//
// The entries array is stored in a ref (not state) to avoid
// re-renders on every append. Components consume it through
// a snapshot counter that triggers re-renders when batches
// arrive.
// =========================================================

import { useCallback, useEffect, useRef, useState } from "react";
import { Channel } from "@tauri-apps/api/core";
import { command } from "../../lib/command";
import type { DevToolsMessage, LogEntry } from "../types";

export interface UseLogStreamReturn {
  /** All log entries, ordered by seq. */
  entries: LogEntry[];
  /** Number of messages dropped due to backend backpressure. */
  droppedCount: number;
  /** Whether the stream is connected. */
  isConnected: boolean;
  /** Clear all entries (both local and backend). */
  clear: () => void;
}

export function useLogStream(): UseLogStreamReturn {
  // Entries stored in a ref to avoid re-renders per entry.
  // The snapshot counter forces a re-render when we want
  // consumers to pick up new entries.
  const entriesRef = useRef<LogEntry[]>([]);
  const [snapshot, setSnapshot] = useState(0);
  const [droppedCount, setDroppedCount] = useState(0);
  const [isConnected, setIsConnected] = useState(false);

  useEffect(() => {
    let cancelled = false;

    async function connect() {
      // 1. Fetch existing entries.
      try {
        const history = await command("devtools_log_history", {
          afterSeq: 0,
          limit: 10_000,
        });
        if (cancelled) return;
        entriesRef.current = history;
        setSnapshot((s) => s + 1);
      } catch (e) {
        console.warn("devtools_log_history failed:", e);
      }

      // 2. Subscribe to live entries.
      const channel = new Channel<DevToolsMessage>();
      channel.onmessage = (message) => {
        if (cancelled) return;

        if (message.type === "entries") {
          entriesRef.current = [...entriesRef.current, ...message.entries];
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
      entriesRef.current = [];
      setDroppedCount(0);
      setSnapshot((s) => s + 1);
    } catch (e) {
      console.warn("devtools_log_clear failed:", e);
    }
  }, []);

  // Trigger re-read of entriesRef when snapshot changes.
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  void snapshot;

  return {
    entries: entriesRef.current,
    droppedCount,
    isConnected,
    clear,
  };
}
