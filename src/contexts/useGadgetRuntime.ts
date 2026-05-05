// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// useGadgetRuntime
//
// Returns the runtime capabilities the host provides for the
// plugin: `sendMessage` for backend RPC and `logger` for
// structured logging.
//
// Available in every plugin component context.
// =========================================================

import { useContext } from "react";
import { GadgetContext, type GadgetRuntime } from "./GadgetContext";

export function useGadgetRuntime(): GadgetRuntime {
  const ctx = useContext(GadgetContext);
  if (!ctx) {
    throw new Error("useGadgetRuntime must be called inside a <GadgetContextProvider>");
  }
  return ctx.runtime;
}
