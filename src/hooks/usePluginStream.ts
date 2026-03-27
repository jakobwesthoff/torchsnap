// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Bridge streaming backend data into React state via
 * `useSyncExternalStore`.
 *
 * On mount, calls `sendMessage(method, payload, onUpdate)` to
 * establish a subscription. Each streaming message from the backend
 * updates the internal snapshot and triggers a synchronous re-render.
 * The initial response from the resolved promise is also stored as the
 * first snapshot.
 *
 * Re-subscribes when `method` or `payload` identity changes (use a
 * stable reference or `useMemo` for objects). Cleanup on unmount
 * drops the channel — the backend should detect the closed channel
 * and stop sending.
 */

import { useCallback, useEffect, useRef, useSyncExternalStore } from "react";
import type { PluginViewProps } from "../plugins/types";

type SendMessage = PluginViewProps["sendMessage"];

export function usePluginStream<TPayload, TSnapshot>(
  sendMessage: SendMessage,
  method: string,
  payload: TPayload,
): TSnapshot | null {
  // Mutable snapshot and subscriber list — the external store
  // contract requires synchronous getSnapshot and subscribe.
  const snapshotRef = useRef<TSnapshot | null>(null);
  const listenersRef = useRef(new Set<() => void>());

  const subscribe = useCallback((listener: () => void) => {
    listenersRef.current.add(listener);
    return () => listenersRef.current.delete(listener);
  }, []);

  const getSnapshot = useCallback(() => snapshotRef.current, []);

  // Establish the subscription whenever the method or payload
  // identity changes.
  useEffect(() => {
    let cancelled = false;

    const update = (value: TSnapshot) => {
      if (cancelled) return;
      snapshotRef.current = value;
      for (const listener of listenersRef.current) {
        listener();
      }
    };

    sendMessage<TPayload, TSnapshot, TSnapshot>(
      method,
      payload,
      (msg) => update(msg),
    ).then(
      (result) => update(result),
      (err) => {
        if (!cancelled) {
          console.error(`usePluginStream(${method}) error:`, err);
        }
      },
    );

    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- payload identity is the trigger
  }, [sendMessage, method, payload]);

  return useSyncExternalStore(subscribe, getSnapshot);
}
