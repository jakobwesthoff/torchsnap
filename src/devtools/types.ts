// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Developer Tools Types
//
// TypeScript mirrors of the Rust serde-serialized types from
// wasm::logging::commands and wasm::logging::mod. Must be
// kept in sync with the Rust definitions.
// =========================================================

export type LogLevel = "trace" | "debug" | "info" | "warn" | "error";

export type LogSource =
  | { type: "plugin"; value: string }
  | { type: "host" };

export interface SpanInfo {
  spanId: number;
  parentId: number | null;
  name: string;
  durationUs: number;
  metadata: [string, string][];
}

export interface LogEntry {
  seq: number;
  timestamp: string;
  level: LogLevel;
  source: LogSource;
  message: string;
  metadata: [string, string][];
  spanId: number | null;
  span: SpanInfo | null;
}

export type DevToolsMessage =
  | { type: "entries"; entries: LogEntry[] }
  | { type: "dropped"; count: number };

export interface LogStats {
  count: number;
  dropped: number;
  totalPushed: number;
}
