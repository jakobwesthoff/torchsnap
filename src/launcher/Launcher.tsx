// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { MagnifyingGlassIcon } from "@heroicons/react/24/outline";
import { useEmacsBindings } from "../hooks/useEmacsBindings";
import { useSetting } from "../hooks/useSetting";
import { SETTINGS_DEFAULTS } from "../settingsDefaults";
import { useWindowLifecycle } from "./hooks/useWindowLifecycle";
import { useKeyboardNavigation } from "./hooks/useKeyboardNavigation";
import { useSearch } from "./hooks/useSearch";
import { ResultList } from "./ResultList";
import { LauncherFooter } from "./LauncherFooter";
import type { ScoredEntry } from "./types";

export function Launcher() {
  const [query, setQuery] = useState("");
  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const mouseActiveRef = useRef(false);

  const { dismiss } = useWindowLifecycle({
    inputRef,
    mouseActiveRef,
    resetState: useCallback(() => {
      setQuery("");
      setSelectedIndex(0);
    }, []),
  });

  // =========================================================
  // Search
  // =========================================================

  const { results } = useSearch(query);

  // Reset selection when results change (new query, different
  // result set).
  useEffect(() => {
    setSelectedIndex(0);
    mouseActiveRef.current = false;
  }, [results]);

  // =========================================================
  // Action Execution
  // =========================================================

  const handleExecute = useCallback(
    (entry?: ScoredEntry) => {
      const target = entry ?? results[selectedIndex];
      if (!target || target.actions.length === 0) return;

      const primaryAction = target.actions[0];
      invoke("execute_action", {
        source: target.source,
        entryId: target.id,
        actionId: primaryAction.id,
      });
    },
    [results, selectedIndex],
  );

  // =========================================================
  // Keyboard Navigation
  // =========================================================

  useKeyboardNavigation({
    dismiss,
    resultCount: results.length,
    selectedIndex,
    setSelectedIndex,
    onExecute: handleExecute,
    mouseActiveRef,
  });

  // Emacs/readline bindings (Ctrl+W, Ctrl+U, Ctrl+K, Ctrl+A, Ctrl+E)
  // for the search input. These are handled outside the keybinding
  // engine because they operate on raw Ctrl which the engine
  // intentionally excludes to avoid macOS Cmd/Ctrl collisions.
  const emacsBindings = useEmacsBindings(inputRef, setQuery);

  const [showMascot] = useSetting("showMascot", SETTINGS_DEFAULTS.showMascot);

  return (
    <div
      className="fixed inset-0 flex flex-col items-center pt-[25vh]"
      onClick={dismiss}
    >
      <div className="relative" onClick={(e) => e.stopPropagation()}>
        {/* Mascot — decorative, positioned above the top-right corner */}
        {showMascot && (
          <img
            src="/images/mascot/snappy-original-96.webp"
            srcSet="/images/mascot/snappy-original-96.webp 1x, /images/mascot/snappy-original-192.webp 2x"
            width={96}
            height={96}
            alt=""
            draggable={false}
            className="absolute -top-[72px] -right-2.5 z-10 pointer-events-none -scale-x-100"
            style={{ WebkitUserDrag: "none" } as React.CSSProperties}
          />
        )}
        {/* Launcher card */}
        <div
          className="w-[680px] rounded-2xl bg-surface overflow-hidden"
          style={{
            boxShadow: [
              "0 0 0 1px var(--color-border)",
              "inset 0 1px 0 0 rgba(255,255,255,0.06)",
              "0 4px 16px rgba(0,0,0,0.12)",
              "0 16px 48px rgba(0,0,0,0.16)",
            ].join(", "),
          }}
        >
          {/* Search input */}
          <div className="flex items-center gap-3 px-5 py-4">
            <MagnifyingGlassIcon className="h-5 w-5 shrink-0 text-accent" />
            <input
              ref={inputRef}
              type="text"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={emacsBindings.onKeyDown}
              placeholder="Type to search"
              className="flex-1 text-lg bg-transparent focus:outline-none placeholder:text-text-muted text-text-primary"
              // Focus trap: re-focus when blurred so keystrokes always
              // reach the input while the launcher is visible.
              onBlur={() => inputRef.current?.focus()}
            />
            <kbd className="shrink-0 rounded border border-border bg-surface-inset px-1.5 py-0.5 font-mono text-[11px] text-text-muted shadow-[0_1px_0_var(--color-border)]">
              ESC
            </kbd>
          </div>

          {/* Result list + action footer */}
          {results.length > 0 && (
            <>
              <div className="border-t border-border" />
              <ResultList
                results={results}
                selectedIndex={selectedIndex}
                onSelectIndex={setSelectedIndex}
                onExecute={handleExecute}
                mouseActiveRef={mouseActiveRef}
              />
              <LauncherFooter
                actions={results[selectedIndex]?.actions ?? []}
              />
            </>
          )}
        </div>
      </div>
    </div>
  );
}
