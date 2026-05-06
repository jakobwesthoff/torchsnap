// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// useGadgetSetting
//
// Reactive accessor for a single setting in the active
// gadget's namespace. Reads from `gadgets.<id>.<key>` in the
// global settings store; writes propagate cross-window.
//
// The gadget id is derived from the surrounding
// GadgetContextProvider via `useGadgetInfo()`, so gadget
// authors only deal with short relative key names:
//
//   const [days, setDays] = useGadgetSetting<number>("retentionDays");
// =========================================================

import { useSetting } from "../hooks/useSetting";
import { useGadgetInfo } from "./useGadgetInfo";

export function useGadgetSetting<T>(key: string): [value: T, setValue: (v: T) => Promise<void>] {
  const { id } = useGadgetInfo();
  return useSetting<T>(`gadgets.${id}.${key}`);
}
