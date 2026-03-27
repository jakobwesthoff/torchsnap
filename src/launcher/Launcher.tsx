// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { Suspense, useCallback, useEffect, useRef, useState } from "react";
import { Channel, invoke } from "@tauri-apps/api/core";
import { MagnifyingGlassIcon } from "@heroicons/react/24/outline";
import { KeyBindingPill } from "../components/KeyBindingPill";
import { useEmacsBindings } from "../hooks/useEmacsBindings";
import { useSetting } from "../hooks/useSetting";
import { SETTINGS_DEFAULTS } from "../settingsDefaults";
import { getPluginComponent } from "../plugins/registry";
import { useWindowLifecycle } from "./hooks/useWindowLifecycle";
import { useKeyboardNavigation } from "./hooks/useKeyboardNavigation";
import { useSearch } from "./hooks/useSearch";
import { ResultList } from "./ResultList";
import { LauncherFooter } from "./LauncherFooter";
import type { Action, ActionId, FooterState, ScoredEntry } from "./types";

/** Derive a generic FooterState from an entry's action list. */
function actionsToFooterState(actions: Action[]): FooterState {
  const primary = actions[0];
  const hints = actions
    .slice(1)
    .filter((a) => a.keybinding)
    .map((a) => ({
      combo: { modifiers: a.keybinding!.modifiers ?? [], key: a.keybinding!.key },
      label: a.label,
    }));

  return {
    primary: primary
      ? { combo: { modifiers: [], key: "Enter" }, label: primary.label }
      : undefined,
    hints,
  };
}

export function Launcher() {
  const [query, setQuery] = useState("");
  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const mouseActiveRef = useRef(false);

  // Local override for when execute_action returns ShowCustomUI.
  // Takes precedence over the search-driven customPluginView.
  const [executePluginView, setExecutePluginView] = useState<string | null>(
    null,
  );

  const { dismiss } = useWindowLifecycle({
    inputRef,
    mouseActiveRef,
    resetState: useCallback(() => {
      setQuery("");
      setSelectedIndex(0);
      setExecutePluginView(null);
    }, []),
  });

  // =========================================================
  // Search
  // =========================================================

  const {
    results,
    customPluginView: searchPluginView,
    matchedPrefix,
  } = useSearch(query);

  const customPluginView = executePluginView ?? searchPluginView;

  // Reset selection when results change (new query, different
  // result set).
  useEffect(() => {
    setSelectedIndex(0);
    mouseActiveRef.current = false;
  }, [results]);

  // =========================================================
  // Plugin Custom UI
  // =========================================================

  const PluginView = customPluginView
    ? getPluginComponent(customPluginView)
    : undefined;

  // Footer state: either set by the plugin or derived from the
  // selected entry's actions in list mode.
  const [pluginFooter, setPluginFooter] = useState<FooterState | null>(null);

  // Reset plugin footer when leaving plugin mode.
  useEffect(() => {
    if (!customPluginView) {
      setPluginFooter(null);
    }
  }, [customPluginView]);

  const footer =
    pluginFooter ?? actionsToFooterState(results[selectedIndex]?.actions ?? []);

  // Plugin execute handler — wraps the Tauri invoke with the
  // plugin's source ID and handles PostAction.
  const handlePluginExecute = useCallback(
    async (entryId: string, actionId: ActionId) => {
      if (!customPluginView) return;

      const postAction = await invoke<string>("execute_action", {
        source: customPluginView,
        entryId,
        actionId,
      });

      if (postAction === "Dismiss") {
        dismiss();
      }
    },
    [customPluginView, dismiss],
  );

  // Pop back from plugin UI: clear the execute override and
  // reset the query. For prefix-triggered plugins this deactivates
  // the plugin through the normal search flow; for execute-triggered
  // plugins it returns to the empty launcher state.
  // TODO: Snapshot/restore the pre-plugin query state instead of
  // always clearing to empty (see clipboard-plugin-settings todo).
  const handleGoBack = useCallback(() => {
    setExecutePluginView(null);
    setQuery("");
  }, []);

  // Plugin message handler — wraps the Tauri invoke with a
  // streaming channel so plugins can push live updates.
  const sendMessage = useCallback(
    async <TPayload = unknown, TResult = unknown, TStream = never>(
      method: string,
      payload: TPayload,
      onMessage?: (msg: TStream) => void,
    ): Promise<TResult> => {
      if (!customPluginView) {
        throw new Error("sendMessage called without an active plugin view");
      }

      const channel = new Channel<TStream>();
      if (onMessage) {
        channel.onmessage = onMessage;
      }

      return invoke<TResult>("plugin_message", {
        source: customPluginView,
        method,
        payload,
        channel,
      });
    },
    [customPluginView],
  );

  // =========================================================
  // Action Execution (list mode)
  // =========================================================

  const handleExecute = useCallback(
    async (entry?: ScoredEntry, actionIndex = 0) => {
      const target = entry ?? results[selectedIndex];
      if (!target || target.actions.length === 0) return;

      const action = target.actions[actionIndex];
      if (!action) return;

      const postAction = await invoke<string>("execute_action", {
        source: target.source,
        entryId: target.id,
        actionId: action.id,
      });

      if (postAction === "Dismiss") {
        dismiss();
      } else if (postAction === "ShowCustomUI") {
        setExecutePluginView(target.source);
        setQuery("");
      }
    },
    [results, selectedIndex, dismiss],
  );

  // =========================================================
  // Keyboard Navigation (disabled when plugin UI is active)
  // =========================================================

  const selectedActions = results[selectedIndex]?.actions ?? [];

  useKeyboardNavigation({
    dismiss,
    query,
    setQuery,
    resultCount: results.length,
    selectedIndex,
    setSelectedIndex,
    onExecute: handleExecute,
    selectedActions,
    mouseActiveRef,
    enabled: customPluginView === null,
  });

  // Emacs/readline bindings (Ctrl+W, Ctrl+U, Ctrl+K, Ctrl+A, Ctrl+E)
  // for the search input. These are handled outside the keybinding
  // engine because they operate on raw Ctrl which the engine
  // intentionally excludes to avoid macOS Cmd/Ctrl collisions.
  const emacsBindings = useEmacsBindings(inputRef, setQuery);

  const [showMascot] = useSetting("showMascot", SETTINGS_DEFAULTS.showMascot);

  // The prefix and stripped query for the plugin component. The
  // backend sends the matched prefix so we don't have to guess.
  const pluginPrefix = matchedPrefix ?? "";
  const strippedQuery = pluginPrefix ? query.slice(pluginPrefix.length) : query;

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
            <KeyBindingPill modifiers={[]} keyName="Escape" />
          </div>

          {/* Result area + footer */}
          {(results.length > 0 || PluginView) && (
            <>
              <div className="border-t border-border" />
              {PluginView ? (
                <Suspense
                  fallback={
                    <div className="p-4 text-center text-text-muted text-sm">
                      Loading…
                    </div>
                  }
                >
                  <PluginView
                    results={results}
                    query={strippedQuery}
                    matchedPrefix={pluginPrefix}
                    goBack={handleGoBack}
                    dismiss={dismiss}
                    mouseActiveRef={mouseActiveRef}
                    onExecute={handlePluginExecute}
                    onFooterChange={setPluginFooter}
                    sendMessage={sendMessage}
                  />
                </Suspense>
              ) : (
                <ResultList
                  results={results}
                  selectedIndex={selectedIndex}
                  onSelectIndex={setSelectedIndex}
                  onExecute={handleExecute}
                  mouseActiveRef={mouseActiveRef}
                />
              )}
              <LauncherFooter footer={footer} />
            </>
          )}
        </div>
      </div>
    </div>
  );
}
