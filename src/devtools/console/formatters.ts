// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Shared Formatting Utilities
//
// Timestamp, duration, and clipboard formatting used by both
// LogItemRow (flat view) and TreeLogList (tree view).
// =========================================================

import type { LogItem } from "../types";

export function formatTimestamp(timestamp: number): string {
  try {
    const date = new Date(timestamp);
    const h = date.getHours().toString().padStart(2, "0");
    const m = date.getMinutes().toString().padStart(2, "0");
    const s = date.getSeconds().toString().padStart(2, "0");
    const ms = date.getMilliseconds().toString().padStart(3, "0");
    return `${h}:${m}:${s}.${ms}`;
  } catch {
    return "??:??:??.???";
  }
}

export function formatDuration(durationUs: number): string {
  const ms = durationUs / 1000;
  if (ms < 1) return `${ms.toFixed(2)}ms`;
  if (ms < 1000) return `${ms.toFixed(1)}ms`;
  return `${(ms / 1000).toFixed(2)}s`;
}

export function durationColor(durationUs: number): string {
  const ms = durationUs / 1000;
  if (ms < 1) return "text-emerald-400";
  if (ms < 10) return "text-text-tertiary";
  if (ms < 100) return "text-amber-400";
  return "text-red-400";
}

export function formatItemForClipboard(item: LogItem): string {
  const time = formatTimestamp(item.timestamp);
  const source = item.source.type === "plugin" ? item.source.value : "host";
  const { kind } = item;

  if (kind.type === "message") {
    const label = kind.level.toUpperCase();
    const meta =
      kind.metadata.length > 0 ? ` {${kind.metadata.map(([k, v]) => `${k}=${v}`).join(", ")}}` : "";
    return `[${time}] [${label}] [${source}] ${kind.message}${meta}`;
  }

  if (kind.type === "spanStart") {
    const meta =
      kind.metadata.length > 0 ? ` {${kind.metadata.map(([k, v]) => `${k}=${v}`).join(", ")}}` : "";
    return `[${time}] [SPAN▶] [${source}] ${kind.name}${meta}`;
  }

  // spanEnd
  const meta =
    kind.metadata.length > 0 ? ` {${kind.metadata.map(([k, v]) => `${k}=${v}`).join(", ")}}` : "";
  const dur = formatDuration(kind.durationUs);
  return `[${time}] [SPAN◀] [${source}] ${kind.name} ${dur}${meta}`;
}
