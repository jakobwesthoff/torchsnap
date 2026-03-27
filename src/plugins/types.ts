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

export interface PluginViewProps {
  /** Search results from the normal search() flow. The plugin
   *  decides whether to use them or ignore them. */
  results: ScoredEntry[];
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
