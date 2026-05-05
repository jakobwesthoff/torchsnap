// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// useGadgetInfo
//
// Returns the identity / static info slice of the plugin
// context: the plugin id and its reactive enabled flag.
//
// Available in every plugin component context (settings,
// launcher view, inline view).
// =========================================================

import { useContext } from "react";
import { GadgetContext, type GadgetInfo } from "./GadgetContext";

export function useGadgetInfo(): GadgetInfo {
  const ctx = useContext(GadgetContext);
  if (!ctx) {
    throw new Error("useGadgetInfo must be called inside a <GadgetContextProvider>");
  }
  return ctx.info;
}
