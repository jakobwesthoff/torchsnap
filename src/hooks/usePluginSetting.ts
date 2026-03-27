// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Factory for creating a plugin-scoped settings hook.
 *
 * Each plugin's settings are stored under `plugins.<id>.<key>` in the
 * global settings store. This factory returns a hook that transparently
 * adds the prefix, so plugin components only deal with short key names:
 *
 * ```tsx
 * const useClipboardSetting = createPluginSettingHook("clipboard-manager");
 *
 * function ClipboardSettings() {
 *   const [retentionDays, setRetentionDays] = useClipboardSetting<number>("retentionDays");
 *   // reads/writes "plugins.clipboard-manager.retentionDays"
 * }
 * ```
 */

import { useSetting } from "./useSetting";

export function createPluginSettingHook(pluginId: string) {
  return function usePluginSetting<T>(
    key: string,
  ): [value: T, setValue: (v: T) => Promise<void>] {
    return useSetting<T>(`plugins.${pluginId}.${key}`);
  };
}
