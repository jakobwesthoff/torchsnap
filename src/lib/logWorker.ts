// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Log Worker (Singleton)
//
// Async command queue that processes frontend log commands
// sequentially. All Logger instances push commands into this
// queue via enqueue(). The worker drains the queue in order,
// calling the appropriate Tauri commands.
//
// Span IDs are local (incrementing counter) on the frontend.
// The worker resolves them to backend IDs by awaiting the
// logger_span_start response, maintaining a local→backend
// mapping. This allows spanStart() to return immediately
// while the IPC happens asynchronously.
// =========================================================

import { command } from "./command";
import type { LogLevel } from "../devtools/types";

// =========================================================
// Command Types
// =========================================================

type LogCommand =
  | {
      type: "message";
      source: string;
      level: LogLevel;
      message: string;
      metadata: [string, string][];
      localSpanId: number | null;
    }
  | {
      type: "spanStart";
      source: string;
      name: string;
      localParentId: number | null;
      metadata: [string, string][];
      localSpanId: number;
    }
  | {
      type: "spanEnd";
      localSpanId: number;
      metadata: [string, string][];
    };

// =========================================================
// Worker State
// =========================================================

let localIdCounter = 0;
const queue: LogCommand[] = [];
const localToBackendId = new Map<number, number>();
let draining = false;

// =========================================================
// Public API
// =========================================================

/** Allocate a local span ID. Returns immediately. */
export function allocateLocalId(): number {
  return ++localIdCounter;
}

/** Enqueue a log command for async processing. */
export function enqueue(cmd: LogCommand): void {
  queue.push(cmd);
  if (!draining) {
    draining = true;
    drain();
  }
}

// =========================================================
// Drain Loop
// =========================================================

/** Resolve a local span ID to its backend ID. */
function resolveSpanId(localId: number | null): number | null {
  if (localId == null) return null;
  return localToBackendId.get(localId) ?? null;
}

async function drain(): Promise<void> {
  while (queue.length > 0) {
    const cmd = queue.shift()!;

    try {
      switch (cmd.type) {
        case "message": {
          const resolvedSpanId = resolveSpanId(cmd.localSpanId);
          await command("logger_emit", {
            source: cmd.source,
            level: cmd.level,
            message: cmd.message,
            metadata: cmd.metadata,
            spanId: resolvedSpanId,
          });
          break;
        }

        case "spanStart": {
          const resolvedParentId = resolveSpanId(cmd.localParentId);
          const backendId = await command("logger_span_start", {
            source: cmd.source,
            name: cmd.name,
            parentId: resolvedParentId,
            metadata: cmd.metadata,
          });
          if (backendId !== 0) {
            localToBackendId.set(cmd.localSpanId, backendId);
          }
          break;
        }

        case "spanEnd": {
          const backendId = localToBackendId.get(cmd.localSpanId);
          if (backendId != null) {
            await command("logger_span_end", {
              spanId: backendId,
              metadata: cmd.metadata,
            });
            localToBackendId.delete(cmd.localSpanId);
          }
          break;
        }
      }
    } catch (e) {
      // Log worker errors should not crash the app. The logging
      // system is diagnostic — silently drop failed commands.
      console.warn("Log worker command failed:", e);
    }
  }

  draining = false;
}
