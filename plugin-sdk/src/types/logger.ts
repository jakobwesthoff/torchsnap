// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Logger Interface
//
// Minimal interface matching the public surface of the
// host's Logger class (src/lib/logger.ts). Plugins receive
// a Logger instance as a prop — this interface describes
// what methods are available without coupling to the host's
// concrete implementation or its internal dependencies
// (logWorker, LogLevel enum, etc.).
// =========================================================

export type LogLevel = "trace" | "debug" | "info" | "warn" | "error";

export interface Logger {
  trace(message: string, metadata?: [string, string][], spanId?: number): void;
  debug(message: string, metadata?: [string, string][], spanId?: number): void;
  info(message: string, metadata?: [string, string][], spanId?: number): void;
  warn(message: string, metadata?: [string, string][], spanId?: number): void;
  error(message: string, metadata?: [string, string][], spanId?: number): void;

  /** Start a span. Returns a local span ID immediately. */
  spanStart(name: string, parentId?: number, metadata?: [string, string][]): number;

  /** End a span. Pass the local span ID returned by spanStart(). */
  spanEnd(spanId: number, metadata?: [string, string][]): void;
}
