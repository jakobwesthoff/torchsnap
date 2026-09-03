// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Developer Tools Types
//
// TypeScript mirrors of the Rust serde-serialized types from
// wasm::logging. Must be kept in sync with the Rust definitions.
//
// LogItem is the universal log stream element with a nested
// discriminated `kind` payload (message, spanStart, spanEnd).
// =========================================================

export type LogLevel = "trace" | "debug" | "info" | "warn" | "error";

export type LogSource = { type: "gadget"; value: string } | { type: "host" };

// =========================================================
// LogItemKind — discriminated payload
// =========================================================

export type LogItemKind =
  | {
      type: "message";
      level: LogLevel;
      message: string;
      metadata: [string, string][];
      spanId: number | null;
    }
  | {
      type: "spanStart";
      spanId: number;
      name: string;
      parentId: number | null;
      depth: number;
      metadata: [string, string][];
    }
  | {
      type: "spanEnd";
      spanId: number;
      name: string;
      parentId: number | null;
      depth: number;
      durationUs: number;
      metadata: [string, string][];
    };

// =========================================================
// LogItem — the universal log stream element
// =========================================================

export interface LogItem {
  seq: number;
  /** Milliseconds since Unix epoch. */
  timestamp: number;
  source: LogSource;
  kind: LogItemKind;
}

// =========================================================
// Type Guards
// =========================================================

type KindOf<T extends LogItemKind["type"]> = Extract<LogItemKind, { type: T }>;

export function isMessage(item: LogItem): item is LogItem & { kind: KindOf<"message"> } {
  return item.kind.type === "message";
}

export function isSpanStart(item: LogItem): item is LogItem & { kind: KindOf<"spanStart"> } {
  return item.kind.type === "spanStart";
}

export function isSpanEnd(item: LogItem): item is LogItem & { kind: KindOf<"spanEnd"> } {
  return item.kind.type === "spanEnd";
}

export function isSpan(item: LogItem): boolean {
  return item.kind.type === "spanStart" || item.kind.type === "spanEnd";
}

// =========================================================
// Wire Protocol
// =========================================================

export type DevToolsMessage =
  { type: "entries"; entries: LogItem[] } | { type: "dropped"; count: number };

export interface LogStats {
  count: number;
  dropped: number;
  totalPushed: number;
}
