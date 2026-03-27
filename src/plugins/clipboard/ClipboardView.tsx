// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Clipboard History — plugin custom UI component.
 *
 * Split-pane layout: left panel is a scrollable entry list, right
 * panel shows a preview of the selected entry. The component
 * subscribes to live updates from the backend via `usePluginStream`
 * so new clipboard captures appear immediately.
 *
 * Keybindings:
 *   - ArrowUp/Down: navigate entries
 *   - Enter: paste the selected entry (writes to clipboard, dismisses)
 *   - Delete/Backspace: remove the selected entry
 *   - Escape: go back to the launcher
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import {
  ClipboardDocumentListIcon,
  DocumentTextIcon,
  PhotoIcon,
} from "@heroicons/react/24/outline";
import { useKeyBindings } from "../../keybindings";
import {
  LAYER,
  type KeyBindingDefinition,
} from "../../keybindings/matching";
import { usePluginStream } from "../../hooks/usePluginStream";
import type { PluginViewProps } from "../types";
import type { ClipboardHistoryEntry } from "./types";

const PLUGIN_LAYER = LAYER.COMPONENT + 2;

// =========================================================
// Relative Time Formatting
// =========================================================

function relativeTime(iso: string): string {
  const now = Date.now();
  const then = new Date(iso).getTime();
  const diffSec = Math.floor((now - then) / 1000);

  if (diffSec < 60) return "just now";
  if (diffSec < 3600) return `${Math.floor(diffSec / 60)}m ago`;
  if (diffSec < 86400) return `${Math.floor(diffSec / 3600)}h ago`;
  return `${Math.floor(diffSec / 86400)}d ago`;
}

// =========================================================
// ClipboardView Component
// =========================================================

export default function ClipboardView({
  query,
  goBack,
  dismiss,
  mouseActiveRef,
  sendMessage,
  onFooterChange,
}: PluginViewProps) {
  const [selectedIndex, setSelectedIndex] = useState(0);
  const listRef = useRef<HTMLDivElement>(null);

  // Stable payload reference — only changes when the query does.
  const subscribePayload = useMemo(
    () => ({ query: query || null, limit: 50 }),
    [query],
  );

  // Subscribe to live clipboard history from the backend.
  const history = usePluginStream<
    { query: string | null; limit: number },
    ClipboardHistoryEntry[]
  >(sendMessage, "subscribe", subscribePayload);

  const entries = history ?? [];

  // Clamp selection when entries change.
  useEffect(() => {
    setSelectedIndex((prev) =>
      entries.length === 0 ? 0 : Math.min(prev, entries.length - 1),
    );
  }, [entries.length]);

  // Keep the selected item scrolled into view.
  useEffect(() => {
    const container = listRef.current;
    if (!container) return;
    const item = container.children[selectedIndex] as HTMLElement | undefined;
    item?.scrollIntoView({ block: "nearest" });
  }, [selectedIndex]);

  // -------------------------------------------------------
  // Actions
  // -------------------------------------------------------

  const handlePaste = useCallback(() => {
    const entry = entries[selectedIndex];
    if (!entry) return;
    // Fire and forget — dismiss immediately without waiting for
    // the clipboard write to complete. The macOS pasteboard API
    // is synchronous and slow enough to feel laggy if we await.
    sendMessage("paste", { id: entry.id });
    dismiss();
  }, [entries, selectedIndex, sendMessage, dismiss]);

  const handleDelete = useCallback(async () => {
    const entry = entries[selectedIndex];
    if (!entry) return;
    await sendMessage("delete", { id: entry.id });
  }, [entries, selectedIndex, sendMessage]);

  // -------------------------------------------------------
  // Footer
  // -------------------------------------------------------

  useEffect(() => {
    onFooterChange({
      primary: {
        combo: { modifiers: [], key: "Enter" },
        label: "Copy to Clipboard",
      },
      hints: [
        {
          combo: { modifiers: ["Meta"], key: "Backspace" },
          label: "Remove",
        },
      ],
    });
  }, [onFooterChange]);

  // -------------------------------------------------------
  // Keybindings
  // -------------------------------------------------------

  const bindings: KeyBindingDefinition[] = useMemo(
    () => [
      {
        id: "clipboard-up",
        layer: PLUGIN_LAYER,
        order: 0,
        handler: () => {
          mouseActiveRef.current = false;
          setSelectedIndex((i) => Math.max(0, i - 1));
        },
        keybindings: [
          { combo: { modifiers: [], key: "ArrowUp" }, allowInInput: true },
        ],
      },
      {
        id: "clipboard-down",
        layer: PLUGIN_LAYER,
        order: 1,
        handler: () => {
          mouseActiveRef.current = false;
          setSelectedIndex((i) => Math.min(entries.length - 1, i + 1));
        },
        keybindings: [
          { combo: { modifiers: [], key: "ArrowDown" }, allowInInput: true },
        ],
      },
      {
        id: "clipboard-paste",
        layer: PLUGIN_LAYER,
        order: 2,
        handler: () => void handlePaste(),
        keybindings: [
          { combo: { modifiers: [], key: "Enter" }, allowInInput: true },
        ],
      },
      {
        id: "clipboard-delete",
        layer: PLUGIN_LAYER,
        order: 3,
        handler: () => void handleDelete(),
        keybindings: [
          { combo: { modifiers: ["Meta"], key: "Backspace" }, allowInInput: true },
        ],
      },
      {
        id: "clipboard-escape",
        layer: PLUGIN_LAYER,
        order: 10,
        handler: () => goBack(),
        keybindings: [
          { combo: { modifiers: [], key: "Escape" }, allowInInput: true },
        ],
      },
    ],
    [entries.length, handlePaste, handleDelete, goBack, mouseActiveRef],
  );

  useKeyBindings(bindings);

  // -------------------------------------------------------
  // Selected entry for preview
  // -------------------------------------------------------

  const selected = entries[selectedIndex] ?? null;

  // -------------------------------------------------------
  // Render
  // -------------------------------------------------------

  if (entries.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center py-12 text-text-muted text-sm gap-2">
        <ClipboardDocumentListIcon className="h-8 w-8" />
        <span>{query ? "No matches found" : "Clipboard history is empty"}</span>
      </div>
    );
  }

  return (
    <div className="flex h-[360px]">
      {/* Left panel — entry list */}
      <div
        ref={listRef}
        className="w-[40%] overflow-y-auto border-r border-border"
        onMouseMove={() => {
          mouseActiveRef.current = true;
        }}
      >
        {entries.map((entry, index) => (
          <div
            key={entry.id}
            className={`flex items-center gap-2 px-3 py-2 cursor-default text-sm ${
              index === selectedIndex
                ? "bg-accent/10 text-text-primary"
                : "text-text-secondary hover:bg-surface-hover"
            }`}
            onMouseEnter={() => {
              if (mouseActiveRef.current) setSelectedIndex(index);
            }}
            onClick={() => void handlePaste()}
          >
            {/* Content type icon */}
            {entry.formats.includes("image") ? (
              <PhotoIcon className="h-4 w-4 shrink-0 text-text-muted" />
            ) : (
              <DocumentTextIcon className="h-4 w-4 shrink-0 text-text-muted" />
            )}

            {/* Preview text */}
            <span className="flex-1 truncate">{entry.preview}</span>

            {/* Timestamp */}
            <span className="shrink-0 text-xs text-text-muted">
              {relativeTime(entry.capturedAt)}
            </span>
          </div>
        ))}
      </div>

      {/* Right panel — preview */}
      <div className="w-[60%] overflow-auto p-4">
        {selected && (
          <>
            {selected.imagePath ? (
              <img
                src={convertFileSrc(selected.imagePath)}
                alt="Clipboard image"
                className="max-w-full max-h-full object-contain rounded"
                draggable={false}
              />
            ) : (
              <pre className="text-sm text-text-secondary whitespace-pre-wrap break-words select-none pointer-events-none font-mono leading-relaxed">
                {selected.preview}
              </pre>
            )}
          </>
        )}
      </div>
    </div>
  );
}
