// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// GadgetContextProvider
//
// The host wraps every gadget component mount in this
// provider before rendering the gadget's React component
// tree. It packs the three slices (info, runtime, optional
// launcher) into a single context value and also drives the
// legacy LoggerContext path so any host component still
// reading via `useLogger()` continues to work.
// =========================================================

import { useMemo, type ReactNode } from "react";
import {
  GadgetContext,
  type LauncherActions,
  type GadgetContextValue,
  type GadgetInfo,
  type GadgetRuntime,
} from "./GadgetContext";
import { LoggerContext } from "./LoggerContext";

export interface GadgetContextProviderProps {
  info: GadgetInfo;
  runtime: GadgetRuntime;
  launcher?: LauncherActions;
  children: ReactNode;
}

export function GadgetContextProvider({
  info,
  runtime,
  launcher,
  children,
}: GadgetContextProviderProps) {
  // Stable value reference unless one of the inputs actually
  // changes. The launcher slice's callbacks are already stable
  // refs at the call sites, so this memo only retriggers on
  // genuine identity changes.
  const value = useMemo<GadgetContextValue>(
    () => ({ info, runtime, launcher }),
    [info, runtime, launcher],
  );

  return (
    <LoggerContext.Provider value={runtime.logger}>
      <GadgetContext.Provider value={value}>{children}</GadgetContext.Provider>
    </LoggerContext.Provider>
  );
}
