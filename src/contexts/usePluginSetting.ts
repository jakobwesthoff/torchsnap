// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// usePluginSetting
//
// Reactive accessor for a single setting in the active
// plugin's namespace. Reads from `plugins.<id>.<key>` in the
// global settings store; writes propagate cross-window.
//
// The plugin id is derived from the surrounding
// PluginContextProvider via `usePluginInfo()`, so plugin
// authors only deal with short relative key names:
//
//   const [days, setDays] = usePluginSetting<number>("retentionDays");
// =========================================================

import { useSetting } from "../hooks/useSetting";
import { usePluginInfo } from "./usePluginInfo";

export function usePluginSetting<T>(key: string): [value: T, setValue: (v: T) => Promise<void>] {
  const { id } = usePluginInfo();
  return useSetting<T>(`plugins.${id}.${key}`);
}
