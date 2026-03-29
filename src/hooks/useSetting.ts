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
 * - Referential stability: `setValue` is stable across renders, and
 *   the returned value only triggers a re-render when it actually
 *   differs (shallow equality).
 */

import { useCallback, useEffect, useState } from "react";
import { getSettingSync, setSetting, subscribe } from "../settingsStore";

export function useSetting<T>(key: string): [value: T, setValue: (v: T) => Promise<void>] {
  // Initial value is read synchronously from the pre-loaded cache.
  // No async gap, no loading state, no default parameter needed.
  const [value, setValueState] = useState<T>(() => getSettingSync<T>(key));

  // Subscribe to changes (same-window and cross-window). The updater
  // form of setState gives us access to the current value without a
  // ref, so we can skip no-op updates without re-subscribing on every
  // state change.
  useEffect(
    () =>
      subscribe((changedKey, newValue) => {
        if (changedKey !== key) return;
        const resolved = newValue as T;
        setValueState((prev) => (resolved === prev ? prev : resolved));
      }),
    [key],
  );

  const setValue = useCallback((v: T) => setSetting(key, v), [key]);

  return [value, setValue];
}
