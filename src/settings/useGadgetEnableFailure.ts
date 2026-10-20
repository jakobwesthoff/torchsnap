// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Why a gadget that is turned on failed to start, or `null` while it
 * runs.
 *
 * The backend keeps the error of a failed `enable()` per gadget id and
 * announces every change. The listener is registered before the first
 * read: Settings can open while gadgets are still starting, and a
 * failure recorded between a read and a later registration would stay
 * hidden until the next change.
 */

import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { command } from "../lib/command";

/** Emitted by the backend when a gadget's enable failure changes. */
export const GADGET_ENABLE_FAILURES_CHANGED = "gadget-enable-failures-changed";

export function useGadgetEnableFailure(gadgetId: string): string | null {
  const [failure, setFailure] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;

    // Without an answer nothing is shown: a missing explanation is
    // better than a false one.
    const pull = () =>
      command("gadget_enable_failures").then(
        (failures) => {
          if (!cancelled) {
            setFailure(failures[gadgetId] ?? null);
          }
        },
        () => {},
      );

    listen(GADGET_ENABLE_FAILURES_CHANGED, () => void pull()).then(
      (u) => {
        if (cancelled) {
          u();
          return;
        }
        unlisten = u;
        void pull();
      },
      () => void pull(),
    );
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [gadgetId]);

  return failure;
}
