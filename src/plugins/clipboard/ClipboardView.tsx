// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Clipboard History — plugin custom UI component.
 *
 * Split-pane layout: left panel is a virtually-scrolled entry list,
 * right panel shows a detail preview of the selected entry. The list
 * subscribes to lightweight `ClipboardListEntry` updates via
 * `usePluginStream`. Full entry detail is loaded on demand when the
 * selection changes, with an LRU cache to avoid re-fetching during
 * rapid keyboard navigation.
 *
 * Keybindings:
 *   - ArrowUp/Down: navigate entries
 *   - Enter: paste the selected entry (writes to clipboard, dismisses)
 *   - Meta+Backspace: remove the selected entry
 *   - Ctrl+D / Ctrl+U: scroll detail preview down/up by half a page
 *   - Escape: go back to the launcher
 */

import { useCallback, useEffect, useMemo, useRef, useState, type RefObject } from "react";
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
import { useWindowedList } from "../../launcher/hooks/useWindowedList";
import { LruCache } from "../../lib/LruCache";
import { useHalfPageScroll } from "../../hooks/useHalfPageScroll";
import type { FooterHint } from "@torchsnap/types";
import type { PluginViewProps } from "../types";
import type { ClipboardHistoryEntry, ClipboardListEntry } from "./types";

const PLUGIN_LAYER = LAYER.COMPONENT + 2;

/** Number of list rows visible at once in the clipboard panel. */
const PAGE_SIZE = 10;

/** Capacity of the detail entry LRU cache. */
const DETAIL_CACHE_CAPACITY = 20;

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
// Sub-components
// =========================================================

function EmptyState({ query }: { query: string }) {
  return (
    <div className="flex flex-col items-center justify-center w-full h-full text-text-muted text-sm gap-2">
      <ClipboardDocumentListIcon className="h-8 w-8" />
      <span>{query ? "No matches found" : "Clipboard history is empty"}</span>
    </div>
  );
}

function EntryRow({
  entry,
  selected,
  onSelect,
  onPaste,
  mouseActiveRef,
}: {
  entry: ClipboardListEntry;
  selected: boolean;
  onSelect: () => void;
  onPaste: () => void;
  mouseActiveRef: RefObject<boolean>;
}) {
  return (
    <div
      className={`flex items-center gap-2 px-3 py-2 cursor-default text-sm ${
        selected
          ? "bg-accent/10 text-text-primary"
          : "text-text-secondary"
      }`}
      onMouseEnter={() => {
        if (mouseActiveRef.current) onSelect();
      }}
      onClick={onPaste}
    >
      {entry.primaryFormat === "image" ? (
        <PhotoIcon className="h-4 w-4 shrink-0 text-text-muted" />
      ) : (
        <DocumentTextIcon className="h-4 w-4 shrink-0 text-text-muted" />
      )}
      <span className="flex-1 truncate">{entry.displayText}</span>
      <span className="shrink-0 text-xs text-text-muted">
        {relativeTime(entry.capturedAt)}
      </span>
    </div>
  );
}

function EntryList({
  entries,
  selectedIndex,
  windowStart,
  onSelect,
  onPaste,
  mouseActiveRef,
  wheelRef,
}: {
  entries: ClipboardListEntry[];
  selectedIndex: number;
  windowStart: number;
  onSelect: (index: number) => void;
  onPaste: () => void;
  mouseActiveRef: RefObject<boolean>;
  wheelRef: RefObject<HTMLDivElement | null>;
}) {
  return (
    <div
      ref={wheelRef}
      className="w-[40%] h-full overflow-hidden border-r border-border"
      onMouseMove={() => {
        mouseActiveRef.current = true;
      }}
    >
      {entries.map((entry, visualIndex) => {
        const absoluteIndex = windowStart + visualIndex;
        return (
          <EntryRow
            key={entry.id}
            entry={entry}
            selected={absoluteIndex === selectedIndex}
            onSelect={() => onSelect(absoluteIndex)}
            onPaste={onPaste}
            mouseActiveRef={mouseActiveRef}
          />
        );
      })}
    </div>
  );
}

function DetailPreview({
  detail,
  loading,
  scrollRef,
}: {
  detail: ClipboardHistoryEntry | null;
  loading: boolean;
  scrollRef: RefObject<HTMLDivElement | null>;
}) {
  if (loading && !detail) {
    return (
      <div className="w-[60%] overflow-hidden p-4 flex items-center justify-center text-text-muted text-sm">
        Loading…
      </div>
    );
  }

  if (!detail) {
    return <div className="w-[60%] overflow-hidden p-4" />;
  }

  // TODO: Add type-specific detail views for files, html, etc.
  // For now, images get a preview and everything else shows display text.
  const imageFormat = detail.formats.image;
  if (detail.primaryFormat === "image" && imageFormat?.type === "asset") {
    return (
      <div ref={scrollRef} className="w-[60%] overflow-y-auto p-4 scrollbar-accent">
        <img
          src={convertFileSrc(imageFormat.path)}
          alt="Clipboard image"
          className="max-w-full max-h-full object-contain rounded"
          draggable={false}
        />
      </div>
    );
  }

  return (
    <div ref={scrollRef} className="w-[60%] overflow-y-auto p-4 scrollbar-accent">
      <pre className="text-sm text-text-secondary whitespace-pre-wrap break-words select-none font-mono leading-relaxed">
        {detail.displayText}
      </pre>
    </div>
  );
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
  const wheelRef = useRef<HTMLDivElement>(null);
  const detailRef = useRef<HTMLDivElement>(null);

  // Stable payload reference — only changes when the query does.
  const subscribePayload = useMemo(
    () => ({ query: query || null }),
    [query],
  );

  // Subscribe to lightweight list entries from the backend.
  const history = usePluginStream<
    { query: string | null },
    ClipboardListEntry[]
  >(sendMessage, "subscribe", subscribePayload);

  const entries = history ?? [];

  // -------------------------------------------------------
  // Virtual Scroll
  // -------------------------------------------------------

  const { windowStart } = useWindowedList({
    selectedIndex,
    setSelectedIndex,
    resultCount: entries.length,
    pageSize: PAGE_SIZE,
    wheelRef,
  });

  const visibleEntries = entries.slice(windowStart, windowStart + PAGE_SIZE);

  // Clamp selection when entries change.
  useEffect(() => {
    setSelectedIndex((prev) =>
      entries.length === 0 ? 0 : Math.min(prev, entries.length - 1),
    );
  }, [entries.length]);

  // -------------------------------------------------------
  // Detail Loading with LRU Cache
  // -------------------------------------------------------

  const detailCacheRef = useRef(
    new LruCache<string, ClipboardHistoryEntry>(DETAIL_CACHE_CAPACITY),
  );
  const [detail, setDetail] = useState<ClipboardHistoryEntry | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [detailOverflows, setDetailOverflows] = useState(false);

  const selectedEntry = entries[selectedIndex] ?? null;
  const selectedId = selectedEntry?.id ?? null;

  useEffect(() => {
    if (!selectedId) {
      setDetail(null);
      return;
    }

    // Check cache first.
    const cached = detailCacheRef.current.get(selectedId);
    if (cached) {
      setDetail(cached);
      return;
    }

    let cancelled = false;
    setDetailLoading(true);

    sendMessage<{ id: string }, ClipboardHistoryEntry | null>(
      "load_full_entry",
      { id: selectedId },
    ).then(
      (result) => {
        if (cancelled) return;
        if (result) {
          detailCacheRef.current.set(selectedId, result);
        }
        setDetail(result);
        setDetailLoading(false);
      },
      (err) => {
        if (cancelled) return;
        console.error("load_full_entry failed:", err);
        setDetail(null);
        setDetailLoading(false);
      },
    );

    return () => {
      cancelled = true;
    };
  }, [selectedId, sendMessage]);

  // -------------------------------------------------------
  // Actions
  // -------------------------------------------------------

  const handlePaste = useCallback(async () => {
    const entry = entries[selectedIndex];
    if (!entry) return;
    // The backend spawns the actual clipboard write on a background
    // thread, so this await only covers the fast SQL query. Dismiss
    // after to keep ordering predictable.
    await sendMessage("paste", { id: entry.id });
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
    const hints: FooterHint[] = [
      {
        combo: { modifiers: ["Meta"], key: "Backspace" },
        label: "Remove",
      },
    ];

    if (detailOverflows) {
      hints.push(
        {
          combo: { modifiers: ["Ctrl"], key: "d" },
          label: "Scroll Down",
        },
        {
          combo: { modifiers: ["Ctrl"], key: "u" },
          label: "Scroll Up",
        },
      );
    }

    onFooterChange({
      primary: {
        combo: { modifiers: [], key: "Enter" },
        label: "Copy to Clipboard",
      },
      hints,
    });
  }, [onFooterChange, detailOverflows]);

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
  // Half-Page Scroll (Ctrl-U / Ctrl-D)
  // -------------------------------------------------------

  useHalfPageScroll({
    ref: detailRef,
    layer: PLUGIN_LAYER,
    order: 20,
    allowInInput: true,
  });

  // Reset detail scroll position when the selection changes so
  // each entry starts at the top.
  useEffect(() => {
    if (detailRef.current) {
      detailRef.current.scrollTop = 0;
    }
  }, [selectedId]);

  // Detect whether the detail panel content overflows its container.
  // Used to conditionally show Ctrl-U/D scroll hints in the footer.
  useEffect(() => {
    const el = detailRef.current;
    setDetailOverflows(el ? el.scrollHeight > el.clientHeight : false);
  }, [detail]);

  // -------------------------------------------------------
  // Render
  //
  // wheelRef is on the EntryList so wheel events over the list
  // navigate entries, while wheel events over the detail panel
  // scroll the preview natively.
  // -------------------------------------------------------

  return (
    <div className="flex h-[360px]">
      {entries.length === 0 ? (
        <EmptyState query={query} />
      ) : (
        <>
          <EntryList
            entries={visibleEntries}
            selectedIndex={selectedIndex}
            windowStart={windowStart}
            onSelect={setSelectedIndex}
            onPaste={() => void handlePaste()}
            mouseActiveRef={mouseActiveRef}
            wheelRef={wheelRef}
          />
          <DetailPreview detail={detail} loading={detailLoading} scrollRef={detailRef} />
        </>
      )}
    </div>
  );
}
