// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Bridge streaming backend data into React state via
 * `useSyncExternalStore`.
 *
 * On mount, calls `sendMessage(method, payload, onStream)` to issue
 * a command with a streaming channel. Returns three pieces of state:
 *
 * - `established` — whether the invoke Promise has resolved (the
 *   backend has processed the command).
 * - `result` — the command's return value, available once established.
 * - `snapshot` — the latest value pushed through the channel, or
 *   `undefined` if no channel message has arrived yet.
 * - `error` — the rejection message if the command failed, otherwise
 *   `undefined`.
 *
 * These two data paths (return value vs. channel) are kept strictly
 * separate — neither overwrites the other.
 *
 * A failed command leaves `snapshot` undefined, which is
 * indistinguishable from an empty result set. Callers that render a
 * "nothing here" state must check `error` first, or a backend failure
 * shows up as a plausible-looking empty list.
 *
 * Re-issues the command when `method` or `payload` identity changes
 * (use a stable reference or `useMemo` for objects). Cleanup on
 * unmount drops the channel — the backend should detect the closed
 * channel and stop sending.
 */

import { useCallback, useEffect, useRef, useSyncExternalStore } from "react";
import type { GadgetSendMessage } from "../contexts/GadgetContext";

type SendMessage = GadgetSendMessage;

export interface GadgetStreamState<TResult, TSnapshot> {
  /** Whether the invoke Promise has resolved. */
  established: boolean;
  /** The command's return value, available once `established` is true. */
  result: TResult | undefined;
  /** The latest value pushed through the channel. */
  snapshot: TSnapshot | undefined;
  /** The rejection message if the command failed. */
  error: string | undefined;
}

/** Reduce an unknown rejection value to a displayable message. */
function errorMessage(err: unknown): string {
  if (err instanceof Error) return err.message;
  return typeof err === "string" ? err : JSON.stringify(err);
}

export function useGadgetStream<TPayload, TResult, TSnapshot>(
  sendMessage: SendMessage,
  method: string,
  payload: TPayload,
): GadgetStreamState<TResult, TSnapshot> {
  // Mutable state and subscriber list — the external store contract
  // requires synchronous getSnapshot and subscribe.
  const stateRef = useRef<GadgetStreamState<TResult, TSnapshot>>({
    established: false,
    result: undefined,
    snapshot: undefined,
    error: undefined,
  });
  const listenersRef = useRef(new Set<() => void>());

  const subscribe = useCallback((listener: () => void) => {
    listenersRef.current.add(listener);
    return () => listenersRef.current.delete(listener);
  }, []);

  const getSnapshot = useCallback(() => stateRef.current, []);

  // Issue the command whenever the method or payload identity changes.
  useEffect(() => {
    let cancelled = false;

    // Reset state for the new command.
    stateRef.current = {
      established: false,
      result: undefined,
      snapshot: undefined,
      error: undefined,
    };

    const notify = () => {
      for (const listener of listenersRef.current) {
        listener();
      }
    };

    sendMessage<TPayload, TResult, TSnapshot>(method, payload, (msg) => {
      if (cancelled) return;
      stateRef.current = { ...stateRef.current, snapshot: msg };
      notify();
    }).then(
      (result) => {
        if (cancelled) return;
        stateRef.current = { ...stateRef.current, established: true, result };
        notify();
      },
      (err) => {
        if (cancelled) return;
        console.error(`useGadgetStream(${method}) error:`, err);
        stateRef.current = {
          ...stateRef.current,
          established: true,
          error: errorMessage(err),
        };
        notify();
      },
    );

    return () => {
      cancelled = true;
    };
  }, [sendMessage, method, payload]);

  return useSyncExternalStore(subscribe, getSnapshot);
}
