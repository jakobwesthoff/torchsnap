// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// useLauncher
//
// Returns the launcher-only action slice: navigation
// (`goBack`, `dismiss`, `openSettings`), execution delegation
// (`onExecute`), footer reporting (`onFooterChange`), display
// query updates (`setDisplayQuery`), and mouse-active tracking
// (`mouseActiveRef`).
//
// Calling this hook from a settings panel context (where the
// GadgetContextProvider has no `launcher` slice) is a bug —
// the hook throws a clear error rather than returning
// undefined-laced callbacks.
// =========================================================

import { useContext } from "react";
import { GadgetContext, type LauncherActions } from "./GadgetContext";

export function useLauncher(): LauncherActions {
  const ctx = useContext(GadgetContext);
  if (!ctx) {
    throw new Error("useLauncher must be called inside a <GadgetContextProvider>");
  }
  if (!ctx.launcher) {
    throw new Error(
      "useLauncher called outside the launcher tree (settings panels do not have launcher actions)",
    );
  }
  return ctx.launcher;
}
