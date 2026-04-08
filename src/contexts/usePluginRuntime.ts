// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// usePluginRuntime
//
// Returns the runtime capabilities the host provides for the
// plugin: `sendMessage` for backend RPC and `logger` for
// structured logging.
//
// Available in every plugin component context.
// =========================================================

import { useContext } from "react";
import { PluginContext, type PluginRuntime } from "./PluginContext";

export function usePluginRuntime(): PluginRuntime {
  const ctx = useContext(PluginContext);
  if (!ctx) {
    throw new Error("usePluginRuntime must be called inside a <PluginContextProvider>");
  }
  return ctx.runtime;
}
