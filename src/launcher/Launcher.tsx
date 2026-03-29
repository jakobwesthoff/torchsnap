// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { Suspense, useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { sendPluginMessage } from "../lib/pluginMessage";
import { MagnifyingGlassIcon } from "@heroicons/react/24/outline";
import { KeyBindingPill } from "../components/KeyBindingPill";
import { useEmacsBindings } from "../hooks/useEmacsBindings";
import { useResizeObserver } from "../hooks/useResizeObserver";
import { useSetting } from "../hooks/useSetting";
import { getPluginComponent } from "../plugins/registry";
import type { PluginViewProps } from "../plugins/types";
import { useWindowLifecycle } from "./hooks/useWindowLifecycle";
import { useKeyboardNavigation } from "./hooks/useKeyboardNavigation";
import { useControlChannel } from "./hooks/useControlChannel";
import { useSearch } from "./hooks/useSearch";
import { ResultList } from "./ResultList";
import { LauncherFooter } from "./LauncherFooter";
import { CARD_TOP_OFFSET } from "./layout";
import type { Action, ActionId, FooterState, ScoredEntry } from "./types";

/** Derive a generic FooterState from an entry's action list. */
function actionsToFooterState(actions: Action[]): FooterState {
  const primary = actions[0];
  const hints = actions
    .slice(1)
    .filter((a) => a.keybinding)
    .map((a) => ({
      combo: {
        modifiers: a.keybinding!.modifiers ?? [],
        key: a.keybinding!.key,
      },
      label: a.label,
    }));

  return {
    primary: primary ? { combo: { modifiers: [], key: "Enter" }, label: primary.label } : undefined,
    hints,
  };
}

/** Dummy footer state used during measurement to ensure the footer
 *  renders at its real height. */
const MEASURE_FOOTER: FooterState = {
  primary: { combo: { modifiers: [], key: "Enter" }, label: "Action" },
  hints: [],
};

interface LauncherProps {
  /** When true, renders an empty content area at max height + a dummy
   *  footer instead of real content. Used for the initial layout
   *  measurement before the first show. */
  measureDummy?: boolean;
  /** Called with the card's dimensions whenever its size changes.
   *  When undefined, no ResizeObserver is attached. */
  onMeasure?: (width: number, height: number) => void;
}

// ESLINT: `getPluginComponent` performs a static registry lookup — the
// returned component reference is referentially stable for any given
// pluginId. The `static-components` rule cannot prove this statically,
// so the lint fires even though no component is truly "created" during
// render.
/* eslint-disable react-hooks/static-components */
function PluginViewContainer({ pluginId, ...props }: PluginViewProps & { pluginId: string }) {
  const View = getPluginComponent(pluginId);
  if (!View) return null;
  return <View {...props} />;
}
/* eslint-enable react-hooks/static-components */

export function Launcher({ measureDummy, onMeasure }: LauncherProps = {}) {
  const [query, setQuery] = useState("");
  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const cardRef = useRef<HTMLDivElement>(null);
  const mouseActiveRef = useRef(false);

  // =========================================================
  // Card size observation
  // =========================================================

  const resizeCallback = useCallback(
    (entry: ResizeObserverEntry) => {
      if (!onMeasure) return;
      const { width, height } = entry.contentRect;
      onMeasure(width, height);
    },
    [onMeasure],
  );

  useResizeObserver(cardRef, onMeasure ? resizeCallback : undefined);

  // Local override for when execute_action returns ShowCustomUI.
  // Takes precedence over the search-driven customPluginView.
  const [executePluginView, setExecutePluginView] = useState<string | null>(null);

  const resetState = useCallback(() => {
    setQuery("");
    setSelectedIndex(0);
    setExecutePluginView(null);
  }, []);

  const { dismiss } = useWindowLifecycle({
    inputRef,
    mouseActiveRef,
    resetState,
  });

  // =========================================================
  // Control API — external command channel
  // =========================================================
  useControlChannel({ resetState, setQuery });

  // =========================================================
  // Plugin activation via global shortcut
  //
  // The backend emits `activate-plugin-custom-ui` when a plugin
  // shortcut fires and the handler returns ShowCustomUI. We
  // clear the query and switch to that plugin's view.
  // =========================================================

  useEffect(() => {
    const unlisten = listen<{ pluginId: string }>("activate-plugin-custom-ui", (event) => {
      setQuery("");
      setSelectedIndex(0);
      setExecutePluginView(event.payload.pluginId);
      inputRef.current?.focus();
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  // =========================================================
  // Search
  // =========================================================

  const { results, customPluginView: searchPluginView, matchedPrefix } = useSearch(query);

  const customPluginView = executePluginView ?? searchPluginView;

  // Reset selection when results change (new query, different
  // result set). Done during render (prev-vs-current pattern) to
  // avoid an extra render cycle from a useEffect.
  const [prevResults, setPrevResults] = useState(results);
  if (prevResults !== results) {
    setPrevResults(results);
    setSelectedIndex(0);
    // ESLINT: mouseActiveRef is only read from mouse event handlers,
    // which cannot fire during render. Writing it here is safe and
    // ensures the flag is current before any post-render event.
    // eslint-disable-next-line react-hooks/refs
    mouseActiveRef.current = false;
  }

  // =========================================================
  // Plugin Custom UI
  // =========================================================

  const hasPluginView = customPluginView != null;

  // Footer state: either set by the plugin or derived from the
  // selected entry's actions in list mode.
  const [pluginFooter, setPluginFooter] = useState<FooterState | null>(null);

  // Reset plugin footer when leaving plugin mode.
  useEffect(() => {
    if (!customPluginView) {
      setPluginFooter(null);
    }
  }, [customPluginView]);

  const footer = pluginFooter ?? actionsToFooterState(results[selectedIndex]?.actions ?? []);

  // Plugin execute handler — wraps the Tauri invoke with the
  // plugin's source ID and handles PostAction.
  const handlePluginExecute = useCallback(
    async (entryId: string, actionId: ActionId) => {
      if (!customPluginView) return;

      const postAction = await invoke<string>("search_execute", {
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

  // Plugin message handler — delegates to the shared utility
  // with the active plugin view as the source.
  const sendMessage = useCallback(
    <TPayload = unknown, TResult = unknown, TStream = never>(
      method: string,
      payload: TPayload,
      onMessage?: (msg: TStream) => void,
    ): Promise<TResult> => {
      if (!customPluginView) {
        throw new Error("sendMessage called without an active plugin view");
      }

      return sendPluginMessage<TPayload, TResult, TStream>(
        customPluginView,
        method,
        payload,
        onMessage,
      );
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

      const postAction = await invoke<string>("search_execute", {
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

  const [mascotMode] = useSetting<string>("mascotMode");

  // The prefix and stripped query for the plugin component. The
  // backend sends the matched prefix so we don't have to guess.
  const pluginPrefix = matchedPrefix ?? "";
  const strippedQuery = pluginPrefix ? query.slice(pluginPrefix.length) : query;

  // =========================================================
  // Content area
  //
  // Four states:
  //   1. Measurement  — full-height placeholder + dummy footer
  //   2. Empty        — search bar only, no content section
  //   3. List view    — result rows + footer
  //   4. Plugin view  — plugin custom UI + footer
  //
  // The max-h-[448px] constraint on the content wrapper defines
  // the maximum height all views must fit within.
  // =========================================================

  let contentBody: React.ReactNode = null;
  let contentFooter: React.ReactNode = null;

  if (measureDummy) {
    contentBody = <div className="h-[448px]" />;
    contentFooter = <LauncherFooter footer={MEASURE_FOOTER} />;
  } else if (hasPluginView) {
    contentBody = (
      <Suspense fallback={<div className="p-4 text-center text-text-muted text-sm">Loading…</div>}>
        <PluginViewContainer
          pluginId={customPluginView}
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
    );
    contentFooter = <LauncherFooter footer={footer} />;
  } else if (results.length > 0) {
    contentBody = (
      <ResultList
        results={results}
        selectedIndex={selectedIndex}
        onSelectIndex={setSelectedIndex}
        onExecute={handleExecute}
        mouseActiveRef={mouseActiveRef}
      />
    );
    contentFooter = <LauncherFooter footer={footer} />;
  }

  const contentSection = contentBody ? (
    <>
      <div className="border-t border-border" />
      <div className="max-h-[448px]">{contentBody}</div>
      {contentFooter}
    </>
  ) : null;

  return (
    <div
      className="fixed inset-0 flex flex-col items-center"
      style={{ paddingTop: CARD_TOP_OFFSET }}
      onClick={dismiss}
    >
      <div className="relative" onClick={(e) => e.stopPropagation()}>
        {/* Mascot — decorative, positioned relative to the launcher card */}
        {mascotMode === "sidekick" && (
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
        {mascotMode === "center" && (
          <img
            src="/images/mascot/snappy-original-192.webp"
            srcSet="/images/mascot/snappy-original-192.webp 1x, /images/mascot/snappy-original-384.webp 2x"
            width={192}
            height={192}
            alt=""
            draggable={false}
            className="absolute -top-[156px] left-1/2 -translate-x-1/2 z-10 pointer-events-none"
            style={{ WebkitUserDrag: "none" } as React.CSSProperties}
          />
        )}
        {/* Launcher card */}
        <div
          ref={cardRef}
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

          {contentSection}
        </div>
      </div>
    </div>
  );
}
