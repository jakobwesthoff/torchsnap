// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// GadgetContext
//
// The gadget component contract. The host wraps every gadget
// component mount in a <GadgetContextProvider> (see
// GadgetContextProvider.tsx) that carries:
//
//   - info     — identity (id, enabled flag)
//   - runtime  — host capabilities (sendMessage, logger)
//   - launcher — launcher-only actions (goBack, dismiss, ...)
//                Only present when the component is mounted
//                inside the launcher tree; absent for settings
//                panels.
//
// Gadget components read what they need via the four hooks
// exposed alongside this file:
//
//   - useGadgetInfo()       → info slice
//   - useGadgetRuntime()    → runtime slice
//   - useLauncher()         → launcher slice (throws outside launcher)
//   - useGadgetSetting<T>() → reactive setting accessor
//
// Per-render data (`results`, `data`, `query`, `matchedPrefix`,
// `selected`) intentionally stays as props on GadgetViewProps /
// InlineViewProps because it changes on every keystroke and
// would force every context consumer to re-render if hoisted
// into context.
//
// This file holds **only** the Context object and its value
// types. The Provider component lives in
// GadgetContextProvider.tsx so React Fast Refresh can do its
// component-only file detection.
// =========================================================

import { createContext, type RefObject } from "react";
import type { ActionSlot, FooterState } from "../types";
import type { Logger } from "../lib/logger";

// ---------------------------------------------------------
// Value shape
// ---------------------------------------------------------

export interface GadgetInfo {
  /** Gadget id (e.g. "calculator", "clipboard-manager"). */
  id: string;
  /** Reactive enabled flag from the host's `enabled.<id>` setting. */
  enabled: boolean;
}

export type GadgetSendMessage = <TPayload = unknown, TResult = unknown, TStream = never>(
  method: string,
  payload: TPayload,
  onMessage?: (msg: TStream) => void,
) => Promise<TResult>;

export interface GadgetRuntime {
  sendMessage: GadgetSendMessage;
  logger: Logger;
}

export interface LauncherActions {
  goBack: () => void;
  dismiss: () => void;
  /** Open the Settings window on this gadget's section and close the
   *  launcher, like the `open-settings` post-action from `execute()`.
   *  Rejects, leaving the launcher open, when the window cannot open. */
  openSettings: () => Promise<void>;
  /** Run the action in `slot` of an entry the view received in
   *  `results`. */
  onExecute: (entryId: string, slot: ActionSlot) => void;
  onFooterChange: (state: FooterState) => void;
  setDisplayQuery: (query: string) => void;
  mouseActiveRef: RefObject<boolean>;
}

export interface GadgetContextValue {
  info: GadgetInfo;
  runtime: GadgetRuntime;
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

export const GadgetContext = createContext<GadgetContextValue | null>(null);

// =========================================================
// SDK shim type sync check
//
// `packages/gadget-sdk/src/shims/hooks.ts` mirrors `GadgetInfo`,
// `GadgetRuntime`, `LauncherActions`, and `GadgetSendMessage`
// for the WASM gadget SDK consumers. The shim re-declares
// the shapes locally so gadgets don't need to reach into
// host source via path-based imports for IDE completion.
// The assertions below fail compilation the moment either
// side drifts from the other, ensuring the mirror stays in
// sync without runtime cost.
// =========================================================

import type {
  LauncherActions as SdkLauncherActions,
  GadgetInfo as SdkGadgetInfo,
  GadgetRuntime as SdkGadgetRuntime,
  GadgetSendMessage as SdkGadgetSendMessage,
} from "../../packages/gadget-sdk/src/shims/hooks";

// The tuple must be assignable to `[true, true, ...]` — if any
// pair diverges, one of the `extends` arms resolves to `never`
// and the assignment fails to compile. Consumed with `void` to
// satisfy `noUnusedLocals` without runtime cost.
void [
  true as SdkGadgetInfo extends GadgetInfo ? true : never,
  true as GadgetInfo extends SdkGadgetInfo ? true : never,
  true as SdkGadgetRuntime extends GadgetRuntime ? true : never,
  true as GadgetRuntime extends SdkGadgetRuntime ? true : never,
  true as SdkLauncherActions extends LauncherActions ? true : never,
  true as LauncherActions extends SdkLauncherActions ? true : never,
  true as SdkGadgetSendMessage extends GadgetSendMessage ? true : never,
  true as GadgetSendMessage extends SdkGadgetSendMessage ? true : never,
];
