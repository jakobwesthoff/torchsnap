// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { Suspense, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { binarySearch } from "../lib/binarySearch";
import { compareEntries } from "./compareEntries";
import { sendPluginMessage } from "../lib/pluginMessage";
import { MagnifyingGlassIcon } from "@heroicons/react/24/outline";
import { KeyBindingPill } from "../components/KeyBindingPill";
import { useEmacsBindings } from "../hooks/useEmacsBindings";
import { MascotInfoOverlay } from "../components/MascotInfoOverlay";
import { useMascotVariant } from "../hooks/useMascotVariant";
import { useResizeObserver } from "../hooks/useResizeObserver";
import { useSetting } from "../hooks/useSetting";
import { getPluginView, getPluginInlineView } from "../plugins/registry";
import type { PluginViewProps, InlineViewProps } from "../plugins/types";
import { useWindowLifecycle } from "./hooks/useWindowLifecycle";
import { useKeyboardNavigation } from "./hooks/useKeyboardNavigation";
import { useControlChannel } from "./hooks/useControlChannel";
import { useLauncherMascotPlacement } from "./hooks/useLauncherMascotPlacement";
import { useMascotInfo } from "./hooks/useMascotInfo";
import { useSearch } from "./hooks/useSearch";
import { LauncherMascot } from "./LauncherMascot";
import { ResultList } from "./ResultList";
import { LauncherFooter } from "./LauncherFooter";
import { CARD_TOP_OFFSET } from "./layout";
import type { Action, ActionId, FooterState, PluginViewRef, PostAction, SourcedEntry } from "./types";

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

// ESLINT: Registry lookups return referentially stable component
// references for any given pluginId + viewName. The lint cannot
// prove this statically, but no component is truly "created"
// during render.
/* eslint-disable react-hooks/static-components */
function PluginViewContainer({
  pluginId,
  viewName,
  ...props
}: PluginViewProps & { pluginId: string; viewName?: string }) {
  const View = getPluginView(pluginId, viewName);
  if (!View) return null;
  return <View {...props} />;
}

function InlineViewContainer({
  pluginId,
  viewName,
  ...props
}: InlineViewProps & { pluginId: string; viewName: string }) {
  const View = getPluginInlineView(pluginId, viewName);
  if (!View) return null;
  return <View {...props} />;
}
/* eslint-enable react-hooks/static-components */

export function Launcher({ measureDummy, onMeasure }: LauncherProps = {}) {
  // =========================================================
  // Query split: displayQuery vs searchQuery
  //
  // Normally in sync (user typing updates both). Plugins can
  // call setDisplayQuery to update the input visually without
  // triggering a new search. When the user next types,
  // searchQuery syncs to displayQuery.
  // =========================================================

  const [displayQuery, setDisplayQueryState] = useState("");
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const cardRef = useRef<HTMLDivElement>(null);
  const mouseActiveRef = useRef(false);

  // User typing: update both queries.
  const setQuery = useCallback((value: string | ((prev: string) => string)) => {
    if (typeof value === "function") {
      setDisplayQueryState((prev) => {
        const next = value(prev);
        setSearchQuery(next);
        return next;
      });
    } else {
      setDisplayQueryState(value);
      setSearchQuery(value);
    }
  }, []);

  // Plugin-only: update display without triggering search.
  const setDisplayQuery = useCallback((value: string) => {
    setDisplayQueryState(value);
  }, []);

  // When the user types after a setDisplayQuery call, sync
  // searchQuery to whatever is in the input (which includes
  // any display-only changes).
  const handleInputChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const value = e.target.value;
    setDisplayQueryState(value);
    setSearchQuery(value);
  }, []);

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
  // Stored as a plugin ID string (no view name — execute-triggered
  // plugins resolve to the "default" view).
  const [executePluginView, setExecutePluginView] = useState<string | null>(null);

  const resetState = useCallback(() => {
    setDisplayQueryState("");
    setSearchQuery("");
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
  }, [setQuery]);

  // =========================================================
  // Search
  // =========================================================

  const {
    results,
    customPluginView: searchPluginView,
    inlinePluginView,
    matchedPrefix,
  } = useSearch(searchQuery);

  // Resolve the active custom plugin view. Execute-triggered
  // views use just a plugin ID (resolves to "default" view).
  // Search-triggered views use a full PluginViewRef.
  const customPluginView: PluginViewRef | null = useMemo(
    () => (executePluginView ? { pluginId: executePluginView, view: "default" } : searchPluginView),
    [executePluginView, searchPluginView],
  );

  // The inline view is only active when there is no custom view
  // taking over the entire result area.
  const activeInlineView = customPluginView == null ? inlinePluginView : null;

  // Stable refs so sendMessage/handlePluginExecute etc. don't need
  // the view objects as useCallback deps (critical for React.memo).
  //
  // ESLINT: Writing refs during render risks an abandoned concurrent
  // render leaving a stale value that an event handler then reads
  // before the committed render overwrites it. Safe here because
  // callbacks only read `.pluginId`, which is invariant across
  // re-renders of the same plugin — a stale ref still holds the
  // correct plugin ID.
  const customPluginViewRef = useRef(customPluginView);
  // eslint-disable-next-line react-hooks/refs
  customPluginViewRef.current = customPluginView;
  const activeInlineViewRef = useRef(activeInlineView);
  // eslint-disable-next-line react-hooks/refs
  activeInlineViewRef.current = activeInlineView;

  // Reset selection when the search query changes (new search
  // cycle). Done during render (prev-vs-current pattern) to avoid
  // an extra render cycle from a useEffect.
  //
  // Incremental plugin results within the same query do NOT reset
  // selection — the selected entry is preserved at its new sorted
  // position via binary search (handled in useWindowedList / the
  // selection stability logic).
  const [prevSearchQuery, setPrevSearchQuery] = useState(searchQuery);
  const [prevResults, setPrevResults] = useState(results);
  if (prevSearchQuery !== searchQuery) {
    // New query — reset selection to top.
    setPrevSearchQuery(searchQuery);
    setPrevResults(results);
    setSelectedIndex(0);
    // ESLINT: mouseActiveRef is only read from mouse event handlers,
    // which cannot fire during render. Writing it here is safe and
    // ensures the flag is current before any post-render event.
    // eslint-disable-next-line react-hooks/refs
    mouseActiveRef.current = false;
  } else if (prevResults !== results) {
    // Same query, but results changed (incremental plugin merge).
    // Find the previously selected entry in the new sorted array
    // so the selection stays on the same item.
    setPrevResults(results);

    const oldEntry = prevResults[selectedIndex];
    if (oldEntry != null && selectedIndex > 0) {
      const newIndex = binarySearch(results, oldEntry, compareEntries);
      if (newIndex !== -1) {
        setSelectedIndex(newIndex);
      }
    }
  }

  // =========================================================
  // Plugin Custom UI
  // =========================================================

  const hasPluginView = customPluginView != null;

  // Footer state set by the active plugin or inline view via
  // their onFooterChange callback. `null` means no plugin/inline
  // footer — fall back to deriving from the selected entry's actions.
  const [pluginFooter, setPluginFooter] = useState<FooterState | null>(null);
  const [inlineFooter, setInlineFooter] = useState<FooterState | null>(null);

  // Reset plugin footer when leaving plugin mode.
  useEffect(() => {
    if (!customPluginView) {
      setPluginFooter(null);
    }
  }, [customPluginView]);

  // Reset inline footer when the inline view disappears.
  useEffect(() => {
    if (!activeInlineView) {
      setInlineFooter(null);
    }
  }, [activeInlineView]);

  const inlineSelected = activeInlineView != null && selectedIndex === 0;
  const listSelectedIndex = activeInlineView != null ? selectedIndex - 1 : selectedIndex;

  // Footer priority: plugin footer (custom UI) > inline footer
  // (when inline slot is selected) > entry actions (list mode).
  const footer =
    pluginFooter ??
    (inlineSelected && inlineFooter
      ? inlineFooter
      : actionsToFooterState(results[listSelectedIndex]?.actions ?? []));

  // Plugin execute handler — wraps the Tauri invoke with the
  // plugin's source ID and handles PostAction.
  const handlePluginExecute = useCallback(
    async (entryId: string, actionId: ActionId) => {
      const view = customPluginViewRef.current;
      if (!view) return;

      const postAction = await invoke<PostAction>("search_execute", {
        source: view.pluginId,
        entryId,
        actionId,
      });

      switch (postAction) {
        case "Dismiss":
          dismiss();
          break;
        case "Nothing":
        case "KeepOpen":
        case "ShowCustomUI":
          // ShowCustomUI is not meaningful from within a plugin view;
          // Nothing and KeepOpen require no action.
          break;
      }
    },
    [dismiss],
  );

  // Inline view execute handler — routes through the inline
  // view's plugin ID.
  const handleInlineExecute = useCallback(
    async (entryId: string, actionId: ActionId) => {
      const view = activeInlineViewRef.current;
      if (!view) return;

      const postAction = await invoke<PostAction>("search_execute", {
        source: view.pluginId,
        entryId,
        actionId,
      });

      switch (postAction) {
        case "Dismiss":
          dismiss();
          break;
        case "Nothing":
        case "KeepOpen":
        case "ShowCustomUI":
          // ShowCustomUI is not meaningful from an inline view;
          // Nothing and KeepOpen require no action.
          break;
      }
    },
    [dismiss],
  );

  // Inline view message handler — same pattern as the plugin
  // sendMessage but routed through the inline view's plugin ID.
  const sendInlineMessage = useCallback(
    <TPayload = unknown, TResult = unknown, TStream = never>(
      method: string,
      payload: TPayload,
      onMessage?: (msg: TStream) => void,
    ): Promise<TResult> => {
      const view = activeInlineViewRef.current;
      if (!view) {
        throw new Error("sendInlineMessage called without an active inline view");
      }

      return sendPluginMessage<TPayload, TResult, TStream>(
        view.pluginId,
        method,
        payload,
        onMessage,
      );
    },
    [],
  );

  // Stable index adapter for ResultList. When an inline view is
  // active, list indices are offset by 1 (inline occupies index 0).
  const handleListSelectIndex = useCallback((idx: number) => {
    setSelectedIndex(activeInlineViewRef.current != null ? idx + 1 : idx);
  }, []);

  // Pop back from plugin UI: clear the execute override and
  // reset the query. For prefix-triggered plugins this deactivates
  // the plugin through the normal search flow; for execute-triggered
  // plugins it returns to the empty launcher state.
  // TODO: Snapshot/restore the pre-plugin query state instead of
  // always clearing to empty (see clipboard-plugin-settings todo).
  const handleGoBack = useCallback(() => {
    setExecutePluginView(null);
    setQuery("");
  }, [setQuery]);

  // Plugin message handler — delegates to the shared utility
  // with the active plugin view as the source.
  const sendMessage = useCallback(
    <TPayload = unknown, TResult = unknown, TStream = never>(
      method: string,
      payload: TPayload,
      onMessage?: (msg: TStream) => void,
    ): Promise<TResult> => {
      const view = customPluginViewRef.current;
      if (!view) {
        throw new Error("sendMessage called without an active plugin view");
      }

      return sendPluginMessage<TPayload, TResult, TStream>(
        view.pluginId,
        method,
        payload,
        onMessage,
      );
    },
    [],
  );

  // =========================================================
  // Action Execution (list mode)
  // =========================================================

  // Stable entry executor — always receives an explicit entry.
  // Used by ResultList (via React.memo, so stability matters).
  const executeEntry = useCallback(
    async (entry: SourcedEntry, actionIndex = 0) => {
      if (entry.actions.length === 0) return;

      const action = entry.actions[actionIndex];
      if (!action) return;

      const postAction = await invoke<PostAction>("search_execute", {
        source: entry.source,
        entryId: entry.id,
        actionId: action.id,
      });

      switch (postAction) {
        case "Dismiss":
          dismiss();
          break;
        case "ShowCustomUI":
          setExecutePluginView(entry.source);
          setQuery("");
          break;
        case "Nothing":
        case "KeepOpen":
          break;
      }
    },
    [dismiss, setQuery],
  );

  // Keyboard-path executor — resolves the currently selected
  // entry and delegates to executeEntry. Unstable (depends on
  // results/listSelectedIndex), but useKeyboardNavigation stores
  // handlers in refs so instability costs nothing.
  const executeSelected = useCallback(
    (actionIndex?: number) => {
      const target = results[listSelectedIndex];
      if (target) executeEntry(target, actionIndex);
    },
    [results, listSelectedIndex, executeEntry],
  );

  // =========================================================
  // Keyboard Navigation (disabled when plugin UI is active)
  // =========================================================

  // When an inline view is active, the total navigable count
  // includes the inline slot at index 0.
  const totalCount = activeInlineView != null ? results.length + 1 : results.length;

  const selectedActions = results[listSelectedIndex]?.actions ?? [];

  useKeyboardNavigation({
    dismiss,
    query: displayQuery,
    setQuery,
    resultCount: totalCount,
    selectedIndex,
    setSelectedIndex,
    onExecute: executeSelected,
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
  const { variant: mascotVariant } = useMascotVariant();
  const mascotInfo = useMascotInfo();
  const mascotPlacement = useLauncherMascotPlacement(
    mascotVariant,
    mascotMode as "center" | "sidekick" | "off",
  );

  // The prefix and stripped query for the plugin component. The
  // backend sends the matched prefix so we don't have to guess.
  const pluginPrefix = matchedPrefix ?? "";
  const strippedQuery = pluginPrefix ? searchQuery.slice(pluginPrefix.length) : searchQuery;

  // =========================================================
  // Content area
  //
  // Five states:
  //   1. Measurement  — full-height placeholder + dummy footer
  //   2. Empty        — search bar only, no content section
  //   3. List view    — result rows + footer
  //   4. Plugin view  — plugin custom UI + footer
  //   5. Inline + list — inline component above result list
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
          pluginId={customPluginView.pluginId}
          viewName={customPluginView.view}
          results={results}
          data={customPluginView.data}
          query={strippedQuery}
          matchedPrefix={pluginPrefix}
          goBack={handleGoBack}
          dismiss={dismiss}
          mouseActiveRef={mouseActiveRef}
          onExecute={handlePluginExecute}
          onFooterChange={setPluginFooter}
          setDisplayQuery={setDisplayQuery}
          sendMessage={sendMessage}
        />
      </Suspense>
    );
    contentFooter = <LauncherFooter footer={footer} />;
  } else if (activeInlineView != null || results.length > 0) {
    contentBody = (
      <>
        {activeInlineView != null && (
          <Suspense
            fallback={<div className="p-4 text-center text-text-muted text-sm">Loading…</div>}
          >
            <InlineViewContainer
              pluginId={activeInlineView.pluginId}
              viewName={activeInlineView.view}
              data={activeInlineView.data}
              query={strippedQuery}
              matchedPrefix={pluginPrefix}
              selected={inlineSelected}
              onExecute={handleInlineExecute}
              onFooterChange={setInlineFooter}
              dismiss={dismiss}
              sendMessage={sendInlineMessage}
            />
          </Suspense>
        )}
        {results.length > 0 && (
          <ResultList
            results={results}
            selectedIndex={listSelectedIndex}
            onSelectIndex={handleListSelectIndex}
            onExecute={executeEntry}
            mouseActiveRef={mouseActiveRef}
          />
        )}
      </>
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
        {mascotPlacement && mascotMode !== "off" && (
          <LauncherMascot
            mode={mascotMode as "center" | "sidekick"}
            variant={mascotVariant}
            top={mascotPlacement.top}
            right={mascotPlacement.right}
            onInfoClick={mascotInfo.show}
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
          <MascotInfoOverlay
            variant={mascotVariant}
            visible={mascotInfo.visible}
            onDismiss={mascotInfo.hide}
          />

          {/* Search input */}
          <div className="flex items-center gap-3 px-5 py-4">
            <MagnifyingGlassIcon className="h-5 w-5 shrink-0 text-accent" />
            <input
              ref={inputRef}
              type="text"
              value={displayQuery}
              onChange={handleInputChange}
              onKeyDown={emacsBindings.onKeyDown}
              placeholder="Type to search"
              className="flex-1 text-lg bg-transparent focus:outline-none placeholder:text-text-muted text-text-primary"
              // Focus trap: re-focus when blurred so keystrokes always
              // reach the input while the launcher is visible.
              onBlur={() => inputRef.current?.focus()}
            />
            {/* In sidekick mode, align the pill's center with the mascot's
                horizontal center. The offset accounts for half the pill's
                rendered width (~15.2px) plus the search bar's horizontal
                padding (px-5 = 20px). */}
            <KeyBindingPill
              modifiers={[]}
              keyName="Escape"
              style={
                mascotPlacement?.centerX != null
                  ? { marginRight: mascotPlacement.centerX - 35.2 }
                  : undefined
              }
            />
          </div>

          {contentSection}
        </div>
      </div>
    </div>
  );
}
