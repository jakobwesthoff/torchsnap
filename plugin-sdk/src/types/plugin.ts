// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Plugin Component Props
//
// Prop interfaces for the three types of React components
// a plugin can provide:
//
// - PluginViewProps     — full-screen custom view (launcher)
// - InlineViewProps     — inline result row with custom UI
// - PluginSettingsProps — settings panel in the sidebar
//
// These are the canonical definitions. The host constructs
// objects satisfying these interfaces before rendering
// plugin components.
// =========================================================

import type { RefObject } from "react";
import type { ActionId, FooterState, SourcedEntry } from "./data";
import type { Logger } from "./logger";
import type { UsePluginSetting } from "./settings";

export interface PluginViewProps {
  results: SourcedEntry[];
  data?: unknown;
  query: string;
  matchedPrefix: string;
  goBack: () => void;
  dismiss: () => void;
  mouseActiveRef: RefObject<boolean>;
  onExecute: (entryId: string, actionId: ActionId) => void;
  onFooterChange: (state: FooterState) => void;
  setDisplayQuery: (query: string) => void;
  sendMessage: <TPayload = unknown, TResult = unknown, TStream = never>(
    method: string,
    payload: TPayload,
    onMessage?: (msg: TStream) => void,
  ) => Promise<TResult>;
  logger: Logger;
}

export interface InlineViewProps {
  data: unknown;
  query: string;
  matchedPrefix: string;
  selected: boolean;
  onExecute: (entryId: string, actionId: ActionId) => void;
  onFooterChange: (state: FooterState) => void;
  dismiss: () => void;
  sendMessage: <TPayload = unknown, TResult = unknown, TStream = never>(
    method: string,
    payload: TPayload,
    onMessage?: (msg: TStream) => void,
  ) => Promise<TResult>;
  logger: Logger;
}

export interface PluginSettingsProps {
  pluginId: string;
  usePluginSetting: UsePluginSetting;
  logger: Logger;
}
