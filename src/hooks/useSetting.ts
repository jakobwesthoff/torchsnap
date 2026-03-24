/**
 * Reactive hook for a single key in the cross-window settings store.
 *
 * ```tsx
 * const [shortcut, setShortcut, ready] = useSetting('globalShortcut', 'CmdOrCtrl+Shift+Space');
 * ```
 *
 * - Returns `defaultValue` while the store is loading and when the key
 *   is not set.
 * - `ready` flips to `true` once the initial read completes.
 * - Re-renders automatically when the value changes, whether the write
 *   originated in this webview or another one.
 * - Referential stability: `setValue` is stable across renders, and
 *   the returned value only triggers a re-render when it actually
 *   differs (shallow equality).
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { getSetting, setSetting, subscribe } from "../settingsStore";

export function useSetting<T>(
  key: string,
  defaultValue: T,
): [value: T, setValue: (v: T) => Promise<void>, ready: boolean] {
  const [value, setValueState] = useState<T>(defaultValue);
  const [ready, setReady] = useState(false);

  // Keep a ref to the latest value so the subscription callback can
  // compare without re-subscribing on every state change.
  const valueRef = useRef<T>(defaultValue);

  // Initial read.
  useEffect(() => {
    let cancelled = false;

    getSetting<T>(key).then((stored) => {
      if (cancelled) {
        return;
      }
      const resolved = stored ?? defaultValue;
      valueRef.current = resolved;
      setValueState(resolved);
      setReady(true);
    });

    return () => {
      cancelled = true;
    };
    // defaultValue is intentionally omitted — it is a static initial
    // value, not something that should re-trigger the load.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);

  // Subscribe to changes (same-window and cross-window).
  useEffect(
    () =>
      subscribe((changedKey, newValue) => {
        if (changedKey !== key) {
          return;
        }
        const resolved = (newValue as T) ?? defaultValue;
        // Only update state when the value actually changed.
        if (resolved !== valueRef.current) {
          valueRef.current = resolved;
          setValueState(resolved);
        }
      }),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [key],
  );

  const setValue = useCallback((v: T) => setSetting(key, v), [key]);

  return [value, setValue, ready];
}
