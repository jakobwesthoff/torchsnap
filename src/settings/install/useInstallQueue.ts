// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Install queue state for the settings window
//
// The backend owns the queue; this hook mirrors it. It pulls the
// queue on mount and again whenever the backend announces a change,
// so requests that arrived while the settings window was closed are
// there the moment it opens.
//
// Results of confirmed installs collect into one batch until the
// user acknowledges them, so several files installed in a row end in
// a single summary with a single restart prompt.
// =========================================================

import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { command, type InstalledGadgetInfo } from "../../lib/command";
import { INSTALL_QUEUE_CHANGED, type InstallRequestView } from "./types";

export interface InstallResult {
  kind: "installed";
  info: InstalledGadgetInfo;
  undone: boolean;
}

export interface InstallQueueState {
  requests: InstallRequestView[];
  /** False until the first pull finished. */
  loaded: boolean;
  results: InstallResult[];
  /** The last failed action, for display next to the queue. */
  error: string | null;
  confirm: (requestId: string) => Promise<void>;
  dismiss: (requestId: string) => Promise<void>;
  undo: (gadgetId: string) => Promise<void>;
  /** Clear the batch results, which also ends the chance to undo them. */
  acknowledge: () => void;
  dismissError: () => void;
}

export function useInstallQueue(): InstallQueueState {
  const [requests, setRequests] = useState<InstallRequestView[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [results, setResults] = useState<InstallResult[]>([]);
  const [error, setError] = useState<string | null>(null);

  // Pull the queue and apply it. `isCurrent` lets the mount effect
  // drop a response that arrives after unmounting.
  const pull = useCallback(
    (isCurrent: () => boolean = () => true) =>
      command("install_queue_snapshot").then(
        (snapshot) => {
          if (isCurrent()) {
            setRequests(snapshot);
            setLoaded(true);
          }
        },
        (e: unknown) => {
          if (isCurrent()) {
            setError(formatError(e));
            setLoaded(true);
          }
        },
      ),
    [],
  );
  const refresh = useCallback(() => pull(), [pull]);

  // `listen` resolves asynchronously. If the component unmounts before
  // it does, the cleanup has nothing to call yet, so the late
  // unlistener is called as soon as it arrives instead of leaking.
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    const isCurrent = () => !cancelled;
    void pull(isCurrent);
    void listen(INSTALL_QUEUE_CHANGED, () => void pull(isCurrent)).then((u) => {
      if (cancelled) {
        u();
      } else {
        unlisten = u;
      }
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [pull]);

  const confirm = useCallback(
    async (requestId: string) => {
      setError(null);
      try {
        const info = await command("install_queue_confirm", { requestId });
        setResults((previous) => [...previous, { kind: "installed", info, undone: false }]);
      } catch (e) {
        setError(formatError(e));
      }
      await refresh();
    },
    [refresh],
  );

  const dismiss = useCallback(
    async (requestId: string) => {
      setError(null);
      try {
        await command("install_queue_dismiss", { requestId });
      } catch (e) {
        setError(formatError(e));
      }
      await refresh();
    },
    [refresh],
  );

  const undo = useCallback(async (gadgetId: string) => {
    setError(null);
    try {
      await command("install_undo", { gadgetId });
      setResults((previous) =>
        previous.map((result) =>
          result.info.id === gadgetId ? { ...result, undone: true } : result,
        ),
      );
    } catch (e) {
      setError(formatError(e));
    }
  }, []);

  const acknowledge = useCallback(() => setResults([]), []);
  const dismissError = useCallback(() => setError(null), []);

  return { requests, loaded, results, error, confirm, dismiss, undo, acknowledge, dismissError };
}

function formatError(error: unknown): string {
  if (typeof error === "string") {
    return error;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return "The install action failed.";
}
