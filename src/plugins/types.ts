// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Shared props interface for plugin custom UI components (ADR 0013).
 *
 * When a plugin requests custom UI via SearchResponse::CustomUI, the
 * host mounts the plugin's React component with these props. The
 * plugin renders into the result list slot and communicates back to
 * the host via callbacks.
 */

import type { RefObject } from "react";
import type { ActionId, FooterState, ScoredEntry } from "@torchsnap/types";
import type { createPluginSettingHook } from "../hooks/usePluginSetting";

export interface PluginViewProps {
  /** Search results from the normal search() flow. The plugin
   *  decides whether to use them or ignore them. */
  results: ScoredEntry[];
  /** Opaque data from the backend's PluginViewRef.data field.
   *  Only present when the plugin returned CustomUI or InlineUI
   *  with a data payload. */
  data?: unknown;
  /** Current query, stripped of the matched prefix. */
  query: string;
  /** Which prefix activated the plugin. */
  matchedPrefix: string;
  /** Pop back to previous state (clear prefix or restore snapshot). */
  goBack: () => void;
  /** Close the launcher entirely. */
  dismiss: () => void;
  /** Shared mouse-active tracking to suppress hover-selection
   *  during keyboard navigation. */
  mouseActiveRef: RefObject<boolean>;
  /** Delegate execution to the host. The host owns PostAction
   *  handling and dismiss logic. */
  onExecute: (entryId: string, actionId: ActionId) => void;
  /** Set the footer content. The plugin is responsible for keeping
   *  this in sync with its actual keybindings. */
  onFooterChange: (state: FooterState) => void;
  /** Update the search input display without triggering a new search.
   *  Used by plugins like the calculator to show a history entry's
   *  expression in the input field while keeping the search stable. */
  setDisplayQuery: (query: string) => void;
  /** Send a custom message to the plugin's backend handler.
   *
   *  The host routes this to the plugin identified by the active
   *  customPluginView. `onMessage` (if provided) receives streaming
   *  updates pushed by the backend over a Tauri channel before the
   *  promise resolves with the final response. */
  sendMessage: <TPayload = unknown, TResult = unknown, TStream = never>(
    method: string,
    payload: TPayload,
    onMessage?: (msg: TStream) => void,
  ) => Promise<TResult>;
}

// =========================================================
// Plugin View Reference
// =========================================================

/**
 * Reference to a plugin view component, sent from the backend.
 * The frontend resolves this to a React component via the plugin
 * registry: `registry[pluginId].views[view]` for CustomUI,
 * `registry[pluginId].inlineViews[view]` for InlineUI.
 */
export interface PluginViewRef {
  pluginId: string;
  view: string;
  data?: unknown;
}

// =========================================================
// Inline View Props
// =========================================================

/**
 * Props for inline view components (InlineUI). Rendered above the
 * standard result list, these participate in the host's selection
 * model at index 0.
 */
export interface InlineViewProps {
  /** Opaque data from the backend's `PluginViewRef.data`. */
  data: unknown;
  /** Current search query (stripped of prefix). */
  query: string;
  /** Prefix that activated the plugin (empty in heuristic mode). */
  matchedPrefix: string;
  /** Whether the inline slot is currently selected (index 0). */
  selected: boolean;
  /** Delegate execution to the host. */
  onExecute: (entryId: string, actionId: ActionId) => void;
  /** Set the footer content. Called by the inline component to
   *  report its footer to the host (same pattern as PluginViewProps). */
  onFooterChange: (state: FooterState) => void;
  /** Close the launcher. */
  dismiss: () => void;
  /** Send a custom message to the plugin's backend handler.
   *  Same signature as PluginViewProps.sendMessage. */
  sendMessage: <TPayload = unknown, TResult = unknown, TStream = never>(
    method: string,
    payload: TPayload,
    onMessage?: (msg: TStream) => void,
  ) => Promise<TResult>;
}

// =========================================================
// Plugin Settings UI
// =========================================================

/**
 * Props passed to a plugin's settings component when rendered in
 * the settings sidebar. The `usePluginSetting` hook is pre-bound
 * to the plugin's namespace so the component only deals with
 * short key names.
 */
export interface PluginSettingsProps {
  pluginId: string;
  usePluginSetting: ReturnType<typeof createPluginSettingHook>;
}
