// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Frontend Logger
//
// Provides a structured logging API for frontend code.
// Each Logger instance is bound to a source (gadget ID or
// "host") and emits messages and spans into the same log
// stream used by the backend.
//
// All methods are fire-and-forget — they enqueue commands
// into the singleton log worker which processes them
// asynchronously. spanStart() returns a local span ID
// immediately; the worker resolves it to a backend ID.
//
// Usage:
//   const logger = createLogger("my-gadget");
//   logger.info("Something happened");
//
//   const span = logger.spanStart("search");
//   logger.debug("Scoring candidates", [], span);
//   logger.spanEnd(span, [["count", "42"]]);
// =========================================================

import type { LogLevel } from "../devtools/types";
import { allocateLocalId, enqueue } from "./logWorker";

// =========================================================
// Logger Class
// =========================================================

export class Logger {
  constructor(private readonly source: string) {}

  // ----- Level methods -----

  trace(message: string, metadata: [string, string][] = [], spanId?: number): void {
    this.log("trace", message, metadata, spanId);
  }

  debug(message: string, metadata: [string, string][] = [], spanId?: number): void {
    this.log("debug", message, metadata, spanId);
  }

  info(message: string, metadata: [string, string][] = [], spanId?: number): void {
    this.log("info", message, metadata, spanId);
  }

  warn(message: string, metadata: [string, string][] = [], spanId?: number): void {
    this.log("warn", message, metadata, spanId);
  }

  error(message: string, metadata: [string, string][] = [], spanId?: number): void {
    this.log("error", message, metadata, spanId);
  }

  // ----- Span methods -----

  /** Start a span. Returns a local span ID immediately. */
  spanStart(name: string, parentId?: number, metadata: [string, string][] = []): number {
    const localId = allocateLocalId();
    enqueue({
      type: "spanStart",
      source: this.source,
      name,
      localParentId: parentId ?? null,
      metadata,
      localSpanId: localId,
    });
    return localId;
  }

  /** End a span. Pass the local span ID returned by spanStart(). */
  spanEnd(spanId: number, metadata: [string, string][] = []): void {
    enqueue({
      type: "spanEnd",
      localSpanId: spanId,
      metadata,
    });
  }

  // ----- Internal -----

  private log(
    level: LogLevel,
    message: string,
    metadata: [string, string][],
    spanId?: number,
  ): void {
    enqueue({
      type: "message",
      source: this.source,
      level,
      message,
      metadata,
      localSpanId: spanId ?? null,
    });
  }
}

// =========================================================
// Factory
// =========================================================

/** Create a logger bound to the given source. */
export function createLogger(source: string): Logger {
  return new Logger(source);
}
