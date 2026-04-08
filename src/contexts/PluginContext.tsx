// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// PluginContext
//
// The plugin component contract. Replaces the old props-based
// approach where every plugin component had to destructure
// `pluginId`, `sendMessage`, `logger`, `goBack`, `dismiss`,
// `onExecute`, etc. directly from its props.
//
// The host wraps every plugin component mount in a
// <PluginContextProvider> (see PluginContextProvider.tsx)
// that carries:
//
//   - info     — identity (id, enabled flag)
//   - runtime  — host capabilities (sendMessage, logger)
//   - launcher — launcher-only actions (goBack, dismiss, ...)
//                Only present when the component is mounted
//                inside the launcher tree; absent for settings
//                panels.
//
// Plugin components read what they need via the four hooks
// exposed alongside this file:
//
//   - usePluginInfo()       → info slice
//   - usePluginRuntime()    → runtime slice
//   - useLauncher()         → launcher slice (throws outside launcher)
//   - usePluginSetting<T>() → reactive setting accessor
//
// Per-render data (`results`, `data`, `query`, `matchedPrefix`,
// `selected`) intentionally stays as props on PluginViewProps /
// InlineViewProps because it changes on every keystroke and
// would force every context consumer to re-render if hoisted
// into context.
//
// This file holds **only** the Context object and its value
// types. The Provider component lives in
// PluginContextProvider.tsx so React Fast Refresh can do its
// component-only file detection.
// =========================================================

import { createContext, type RefObject } from "react";
import type { ActionId, FooterState } from "../types";
import type { Logger } from "../lib/logger";

// ---------------------------------------------------------
// Value shape
// ---------------------------------------------------------

export interface PluginInfo {
  /** Plugin id (e.g. "calculator", "clipboard-manager"). */
  id: string;
  /** Reactive enabled flag from the host's `enabled.<id>` setting. */
  enabled: boolean;
}

export type PluginSendMessage = <TPayload = unknown, TResult = unknown, TStream = never>(
  method: string,
  payload: TPayload,
  onMessage?: (msg: TStream) => void,
) => Promise<TResult>;

export interface PluginRuntime {
  sendMessage: PluginSendMessage;
  logger: Logger;
}

export interface LauncherActions {
  goBack: () => void;
  dismiss: () => void;
  onExecute: (entryId: string, actionId: ActionId) => void;
  onFooterChange: (state: FooterState) => void;
  setDisplayQuery: (query: string) => void;
  mouseActiveRef: RefObject<boolean>;
}

export interface PluginContextValue {
  info: PluginInfo;
  runtime: PluginRuntime;
  /** Present when mounted inside the launcher; absent in settings. */
  launcher?: LauncherActions;
}

// ---------------------------------------------------------
// Context
//
// `null` sentinel rather than a default object so the hooks
// can throw a clear error if a component is rendered outside
// a provider (which is always a bug, never an intended state).
// ---------------------------------------------------------

export const PluginContext = createContext<PluginContextValue | null>(null);

// =========================================================
// SDK shim type sync check
//
// `plugin-sdk/src/shims/hooks.ts` mirrors `PluginInfo`,
// `PluginRuntime`, `LauncherActions`, and `PluginSendMessage`
// for the WASM plugin SDK consumers. The shim re-declares
// the shapes locally so plugins don't need to reach into
// host source via path-based imports for IDE completion.
// The assertions below fail compilation the moment either
// side drifts from the other, ensuring the mirror stays in
// sync without runtime cost.
// =========================================================

import type {
  LauncherActions as SdkLauncherActions,
  PluginInfo as SdkPluginInfo,
  PluginRuntime as SdkPluginRuntime,
  PluginSendMessage as SdkPluginSendMessage,
} from "../../plugin-sdk/src/shims/hooks";

// The tuple must be assignable to `[true, true, ...]` — if any
// pair diverges, one of the `extends` arms resolves to `never`
// and the assignment fails to compile. Consumed with `void` to
// satisfy `noUnusedLocals` without runtime cost.
void [
  true as SdkPluginInfo extends PluginInfo ? true : never,
  true as PluginInfo extends SdkPluginInfo ? true : never,
  true as SdkPluginRuntime extends PluginRuntime ? true : never,
  true as PluginRuntime extends SdkPluginRuntime ? true : never,
  true as SdkLauncherActions extends LauncherActions ? true : never,
  true as LauncherActions extends SdkLauncherActions ? true : never,
  true as SdkPluginSendMessage extends PluginSendMessage ? true : never,
  true as PluginSendMessage extends SdkPluginSendMessage ? true : never,
];
