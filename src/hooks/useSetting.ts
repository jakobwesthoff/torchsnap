// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Reactive hook for a single key in the cross-window settings store.
 *
 * ```tsx
 * const [shortcut, setShortcut] = useSetting<string>("globalShortcut");
 * ```
 *
 * - The value is available synchronously on the first render because
 *   `initStore()` is awaited before React mounts, and the backend
 *   ensures all defaults exist in the store at startup.
 * - Re-renders automatically when the value changes, whether the write
 *   originated in this webview or another one.
 * - Referential stability: `setValue` is stable while `key` stays the
 *   same, and the returned value only triggers a re-render when it
 *   actually differs (`Object.is`).
 */

import { useCallback, useSyncExternalStore } from "react";
import { getSettingSync, setSetting, subscribe } from "../settingsStore";

export function useSetting<T>(key: string): [value: T, setValue: (v: T) => Promise<void>] {
  // The store's cache is the source of truth and is updated before
  // listeners run, so React reads the value straight from it. Reading
  // on every render (instead of once into state) means a changed `key`
  // returns the new key's value immediately; components such as the
  // launcher switch keys without remounting.
  const subscribeToKey = useCallback(
    (onChange: () => void) =>
      subscribe((changedKey) => {
        if (changedKey === key) onChange();
      }),
    [key],
  );
  const value = useSyncExternalStore(subscribeToKey, () => getSettingSync<T>(key));

  const setValue = useCallback((v: T) => setSetting(key, v), [key]);

  return [value, setValue];
}
