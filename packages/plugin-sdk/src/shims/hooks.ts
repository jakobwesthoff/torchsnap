// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Plugin Context Hooks Shim
//
// Bridges plugin code's
// `import { usePluginInfo } from "@torchsnap/plugin-sdk/hooks"`
// to the host-provided hook implementations on
// `window.__torchsnap.hooks`.
//
// Same pattern as the React shim — at build time, the plugin's
// Vite resolves the subpath import to this file. The shim
// reads from the global at runtime, so plugin bundles ship
// only the import declaration, not the hook implementations.
//
// The hook *types* are mirrored locally so plugin code gets
// proper IDE completion without depending on host source paths
// (which would only work in-monorepo). When the SDK is
// eventually published to npm, the type-only path stays
// stable.
// =========================================================

import "./global";
import type { RefObject } from "react";
import type { ActionId, FooterState } from "../types/data";
import type { Logger } from "../types/logger";

// ---------------------------------------------------------
// Ambient global
//
// The host's `initPluginSdk()` populates `window.__torchsnap`
// before any plugin bundle loads. This ambient declaration
// teaches the plugin's tsc about the shape so the lazy access
// inside `hostHooks()` typechecks cleanly without leaking
// implementation details into the plugin.
// ---------------------------------------------------------

// The `TorchsnapGlobal` interface is declared in each shim
// file with only the slice that shim cares about. TypeScript
// merges sibling interface declarations across files, so the
// final shape — assembled by whichever combination of shims
// the consuming plugin imports — has every contributed slice
// available without any single shim needing to know the
// other shims' property types.
declare global {
  interface TorchsnapGlobal {
    hooks: {
      usePluginInfo: () => PluginInfo;
      usePluginRuntime: () => PluginRuntime;
      useLauncher: () => LauncherActions;
      usePluginSetting: <T>(
        key: string,
      ) => [value: T, setValue: (v: T) => Promise<void>];
      useWindowedList: (params: UseWindowedListParams) => UseWindowedListResult;
    };
  }
}

// ---------------------------------------------------------
// useWindowedList types (mirrors src/launcher/hooks/useWindowedList.ts)
// ---------------------------------------------------------

export interface UseWindowedListParams {
  selectedIndex: number;
  setSelectedIndex: (index: number) => void;
  resultCount: number;
  pageSize: number;
}

export interface UseWindowedListResult {
  windowStart: number;
  wheelRef: (el: HTMLDivElement | null) => void;
}

// ---------------------------------------------------------
// Hook return shapes (mirrors src/contexts/PluginContext.tsx)
// ---------------------------------------------------------

export interface PluginInfo {
  id: string;
  enabled: boolean;
}

/**
 * Sends a custom message to the plugin's backend handler.
 *
 * The host routes the call to the plugin identified by the
 * surrounding `PluginContext` provider's `info.id`. The
 * returned promise resolves with the backend's response.
 *
 * **WASM plugins do not support streaming.** The `onMessage`
 * callback is part of the public type signature for
 * symmetry with native plugins, but WASM plugin bridges
 * silently drop the streaming channel — `onMessage` is
 * never invoked for a WASM-backed plugin no matter what
 * the backend tries to push. This is enforced by the
 * `WasmPluginBridge::handle_message` override (see ADR
 * 0030 for the rationale and the future
 * `messaging-stream` sub-interface that would lift this
 * restriction).
 *
 * If you're writing a plugin that needs streaming, your
 * options are: (a) keep it native, (b) poll via repeated
 * one-shot calls, or (c) wait for the streaming
 * sub-interface.
 */
export type PluginSendMessage = <
  TPayload = unknown,
  TResult = unknown,
  TStream = never,
>(
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

// ---------------------------------------------------------
// Shim hooks
//
// Plugin authors call these directly. Each one resolves the
// host-provided implementation lazily on first call so the
// shim can load before initPluginSdk() runs without throwing
// at module-evaluation time. (initPluginSdk runs before any
// plugin bundle is fetched, but defensive reads keep the
// shim robust against future loader reordering.)
// ---------------------------------------------------------

function hostHooks() {
  const t = window.__torchsnap;
  if (!t) {
    throw new Error(
      "@torchsnap/plugin-sdk/hooks: window.__torchsnap is not initialized — call initPluginSdk() before loading plugin bundles",
    );
  }
  return t.hooks;
}

export function usePluginInfo(): PluginInfo {
  return hostHooks().usePluginInfo();
}

export function usePluginRuntime(): PluginRuntime {
  return hostHooks().usePluginRuntime();
}

export function useLauncher(): LauncherActions {
  return hostHooks().useLauncher();
}

export function usePluginSetting<T>(
  key: string,
): [value: T, setValue: (v: T) => Promise<void>] {
  return hostHooks().usePluginSetting<T>(key);
}

export function useWindowedList(
  params: UseWindowedListParams,
): UseWindowedListResult {
  return hostHooks().useWindowedList(params);
}
