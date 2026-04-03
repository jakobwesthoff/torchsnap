// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Log Entry Row
//
// Renders a single log entry as a horizontal line with:
// - Colored left border (level indicator)
// - Timestamp in monospace
// - Level badge (colored text)
// - Plugin source with colored dot
// - Message in monospace
// - Duration badge for span-end entries
// - Expandable metadata
// =========================================================

import { memo, useCallback, useState } from "react";
import { cn } from "../../lib/cn";
import type { LogEntry, LogLevel } from "../types";

// =========================================================
// Copy to Clipboard
// =========================================================

function formatEntryForClipboard(entry: LogEntry): string {
  const time = formatTimestamp(entry.timestamp);
  const level = entry.level.toUpperCase();
  const source =
    entry.source.type === "plugin" ? entry.source.value : "host";
  const meta =
    entry.metadata.length > 0
      ? ` {${entry.metadata.map(([k, v]) => `${k}=${v}`).join(", ")}}`
      : "";
  const span = entry.span
    ? ` [${entry.span.name} ${formatDuration(entry.span.durationUs)}]`
    : "";
  return `[${time}] [${level}] [${source}] ${entry.message}${meta}${span}`;
}

// =========================================================
// Level Colors
// =========================================================

const levelBorderColor: Record<LogLevel, string> = {
  error: "border-l-red-500",
  warn: "border-l-amber-500",
  info: "border-l-blue-500",
  debug: "border-l-gray-400",
  trace: "border-l-gray-300",
};

const levelTextColor: Record<LogLevel, string> = {
  error: "text-red-400",
  warn: "text-amber-400",
  info: "text-blue-400",
  debug: "text-text-tertiary",
  trace: "text-text-muted",
};

// =========================================================
// Plugin Colors (16-color palette, deterministic by hash)
// =========================================================

const PLUGIN_COLORS = [
  "bg-orange-400", "bg-sky-400", "bg-emerald-400", "bg-violet-400",
  "bg-rose-400", "bg-teal-400", "bg-indigo-400", "bg-lime-400",
  "bg-amber-400", "bg-cyan-400", "bg-green-400", "bg-purple-400",
  "bg-pink-400", "bg-blue-400", "bg-yellow-400", "bg-fuchsia-400",
];

function pluginColorIndex(pluginId: string): number {
  let hash = 0;
  for (let i = 0; i < pluginId.length; i++) {
    hash = ((hash << 5) - hash + pluginId.charCodeAt(i)) | 0;
  }
  return Math.abs(hash) % PLUGIN_COLORS.length;
}

// =========================================================
// Duration Formatting
// =========================================================

function formatDuration(durationUs: number): string {
  const ms = durationUs / 1000;
  if (ms < 1) return `${(ms).toFixed(2)}ms`;
  if (ms < 1000) return `${ms.toFixed(1)}ms`;
  return `${(ms / 1000).toFixed(2)}s`;
}

function durationColor(durationUs: number): string {
  const ms = durationUs / 1000;
  if (ms < 1) return "text-emerald-400";
  if (ms < 10) return "text-text-tertiary";
  if (ms < 100) return "text-amber-400";
  return "text-red-400";
}

// =========================================================
// Timestamp Formatting
// =========================================================

function formatTimestamp(timestamp: string): string {
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

// =========================================================
// Component
// =========================================================

interface LogEntryRowProps {
  entry: LogEntry;
  index: number;
}

export const LogEntryRow = memo(function LogEntryRow({
  entry,
  index,
}: LogEntryRowProps) {
  const [metadataExpanded, setMetadataExpanded] = useState(false);

  const copyToClipboard = useCallback(() => {
    navigator.clipboard.writeText(formatEntryForClipboard(entry));
  }, [entry]);
  const hasMetadata =
    entry.metadata.length > 0 ||
    (entry.span?.metadata && entry.span.metadata.length > 0);

  const pluginId =
    entry.source.type === "plugin" ? entry.source.value : null;

  return (
    <div
      className={cn(
        "group flex items-start gap-2 px-3 py-1 text-xs border-b border-border-divider/50",
        "hover:bg-surface-hover/50 transition-colors",
        "border-l-2",
        levelBorderColor[entry.level],
        index % 2 === 0 && "bg-surface-inset/20",
      )}
    >
      {/* Timestamp */}
      <span className="shrink-0 w-[70px] tabular-nums text-text-muted font-mono text-[11px] leading-5 select-text">
        {formatTimestamp(entry.timestamp)}
      </span>

      {/* Level badge */}
      <span
        className={cn(
          "shrink-0 w-[42px] text-center text-[10px] font-semibold uppercase tracking-wider leading-5",
          levelTextColor[entry.level],
        )}
      >
        {entry.level}
      </span>

      {/* Source */}
      <span className="shrink-0 w-[120px] flex items-center gap-1">
        {pluginId ? (
          <>
            <span
              className={cn(
                "w-1.5 h-1.5 rounded-full shrink-0",
                PLUGIN_COLORS[pluginColorIndex(pluginId)],
              )}
            />
            <span className="text-[11px] text-text-secondary truncate font-medium">
              {pluginId}
            </span>
          </>
        ) : (
          <span className="text-[11px] text-text-muted italic">host</span>
        )}
      </span>

      {/* Message + span info */}
      <div className="flex-1 min-w-0 flex flex-col">
        <div className="flex items-start gap-1">
          <span className="flex-1 min-w-0 font-mono text-[11px] leading-5 text-text-primary break-words select-text">
            {entry.message}
          </span>

          {/* Duration badge for span-end entries */}
          {entry.span && (
            <span className="inline-flex items-center gap-1 ml-2 shrink-0">
              <span
                className={cn(
                  "text-[10px] font-mono tabular-nums",
                  durationColor(entry.span.durationUs),
                )}
              >
                {formatDuration(entry.span.durationUs)}
              </span>
            </span>
          )}

          {/* Metadata toggle */}
          {hasMetadata && (
            <button
              className="shrink-0 p-0.5 text-text-muted hover:text-text-secondary transition-colors"
              onClick={() => setMetadataExpanded(!metadataExpanded)}
            >
              <svg
                className={cn(
                  "w-3 h-3 transition-transform",
                  metadataExpanded && "rotate-90",
                )}
                fill="none"
                viewBox="0 0 24 24"
                strokeWidth={2}
                stroke="currentColor"
              >
                <path strokeLinecap="round" strokeLinejoin="round" d="m8.25 4.5 7.5 7.5-7.5 7.5" />
              </svg>
            </button>
          )}

          {/* Copy button (visible on hover) */}
          <button
            className="opacity-0 group-hover:opacity-100 shrink-0 p-0.5 text-text-muted hover:text-text-primary transition-opacity"
            onClick={copyToClipboard}
            title="Copy to clipboard"
          >
            <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" d="M15.666 3.888A2.25 2.25 0 0 0 13.5 2.25h-3c-1.03 0-1.9.693-2.166 1.638m7.332 0c.055.194.084.4.084.612v0a.75.75 0 0 1-.75.75H9.75a.75.75 0 0 1-.75-.75v0c0-.212.03-.418.084-.612m7.332 0c.646.049 1.288.11 1.927.184 1.1.128 1.907 1.077 1.907 2.185V19.5a2.25 2.25 0 0 1-2.25 2.25H6.75A2.25 2.25 0 0 1 4.5 19.5V6.257c0-1.108.806-2.057 1.907-2.185a48.208 48.208 0 0 1 1.927-.184" />
            </svg>
          </button>
        </div>

        {/* Expanded metadata */}
        {metadataExpanded && hasMetadata && (
          <div className="mt-1 flex flex-wrap gap-x-4 gap-y-0.5">
            {entry.metadata.map(([key, value]) => (
              <span key={key} className="text-[10px] font-mono">
                <span className="text-violet-400">{key}</span>
                <span className="text-text-muted">=</span>
                <span className="text-text-secondary select-text">{value}</span>
              </span>
            ))}
            {entry.span?.metadata.map(([key, value]) => (
              <span key={`span-${key}`} className="text-[10px] font-mono">
                <span className="text-violet-400">{key}</span>
                <span className="text-text-muted">=</span>
                <span className="text-text-secondary select-text">{value}</span>
              </span>
            ))}
          </div>
        )}
      </div>
    </div>
  );
});
