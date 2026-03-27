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

import { useCallback, useEffect, useRef, useState } from "react";
import { getSettingSync, setSetting, subscribe } from "../settingsStore";

export function useSetting<T>(
  key: string,
): [value: T, setValue: (v: T) => Promise<void>] {
  // Initial value is read synchronously from the pre-loaded cache.
  // No async gap, no loading state, no default parameter needed.
  const [value, setValueState] = useState<T>(() => getSettingSync<T>(key));

  // Keep a ref to the latest value so the subscription callback can
  // compare without re-subscribing on every state change.
  const valueRef = useRef<T>(value);
  valueRef.current = value;

  // Subscribe to changes (same-window and cross-window).
  useEffect(
    () =>
      subscribe((changedKey, newValue) => {
        if (changedKey !== key) {
          return;
        }
        const resolved = newValue as T;
        // Only update state when the value actually changed.
        if (resolved !== valueRef.current) {
          valueRef.current = resolved;
          setValueState(resolved);
        }
      }),
    [key],
  );

  const setValue = useCallback((v: T) => setSetting(key, v), [key]);

  return [value, setValue];
}
