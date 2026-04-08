// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// PluginContextProvider
//
// The host wraps every plugin component mount in this
// provider before rendering the plugin's React component
// tree. It packs the three slices (info, runtime, optional
// launcher) into a single context value and also drives the
// legacy LoggerContext path so any host component still
// reading via `useLogger()` continues to work.
// =========================================================

import { useMemo, type ReactNode } from "react";
import {
  PluginContext,
  type LauncherActions,
  type PluginContextValue,
  type PluginInfo,
  type PluginRuntime,
} from "./PluginContext";
import { LoggerContext } from "./LoggerContext";

export interface PluginContextProviderProps {
  info: PluginInfo;
  runtime: PluginRuntime;
  launcher?: LauncherActions;
  children: ReactNode;
}

export function PluginContextProvider({
  info,
  runtime,
  launcher,
  children,
}: PluginContextProviderProps) {
  // Stable value reference unless one of the inputs actually
  // changes. The launcher slice's callbacks are already stable
  // refs at the call sites, so this memo only retriggers on
  // genuine identity changes.
  const value = useMemo<PluginContextValue>(
    () => ({ info, runtime, launcher }),
    [info, runtime, launcher],
  );

  return (
    <LoggerContext.Provider value={runtime.logger}>
      <PluginContext.Provider value={value}>{children}</PluginContext.Provider>
    </LoggerContext.Provider>
  );
}
