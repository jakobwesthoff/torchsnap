// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Pending gadget changes
//
// Installs, replacements and uninstalls take effect on restart.
// The backend keeps track of them; this hook mirrors that record,
// so the gadget list shows the same state after Settings was
// closed and opened again. It pulls on mount and whenever the
// install queue announces a change (a confirmed install is one),
// and after its own undo.
// =========================================================

import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { command } from "../../lib/command";
import { INSTALL_QUEUE_CHANGED, type PendingGadget } from "./types";

export interface PendingChangesState {
  changes: Record<string, PendingGadget>;
  loaded: boolean;
  error: string | null;
  refresh: () => Promise<void>;
  undo: (gadgetId: string) => Promise<void>;
  dismissError: () => void;
}

export function usePendingChanges(): PendingChangesState {
  const [changes, setChanges] = useState<Record<string, PendingGadget>>({});
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // `isCurrent` lets the mount effect drop a response that arrives
  // after unmounting.
  const pull = useCallback(
    (isCurrent: () => boolean = () => true) =>
      command("pending_gadget_changes").then(
        (next) => {
          if (isCurrent()) {
            setChanges(next);
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

  const refresh = useCallback(() => pull(), [pull]);

  const undo = useCallback(
    async (gadgetId: string) => {
      setError(null);
      try {
        await command("install_undo", { gadgetId });
      } catch (e) {
        setError(formatError(e));
      }
      await pull();
    },
    [pull],
  );

  const dismissError = useCallback(() => setError(null), []);

  return { changes, loaded, error, refresh, undo, dismissError };
}

function formatError(error: unknown): string {
  if (typeof error === "string") {
    return error;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return "The action failed.";
}
