// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// usePluginInfo
//
// Returns the identity / static info slice of the plugin
// context: the plugin id and its reactive enabled flag.
//
// Available in every plugin component context (settings,
// launcher view, inline view).
// =========================================================

import { useContext } from "react";
import { PluginContext, type PluginInfo } from "./PluginContext";

export function usePluginInfo(): PluginInfo {
  const ctx = useContext(PluginContext);
  if (!ctx) {
    throw new Error("usePluginInfo must be called inside a <PluginContextProvider>");
  }
  return ctx.info;
}
