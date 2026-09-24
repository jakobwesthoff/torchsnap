// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Update phase for the update window
//
// The backend owns the phase; this hook mirrors it. It pulls the
// phase once on mount, because the window can open in the middle of a
// check, and then follows the change events.
// =========================================================

import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { command } from "../lib/command";
import { UPDATE_PHASE_CHANGED, type UpdatePhase } from "./types";

/** `null` until the first answer from the backend. */
export function useUpdatePhase(): UpdatePhase | null {
  const [phase, setPhase] = useState<UpdatePhase | null>(null);

  useEffect(() => {
    let cancelled = false;
    // An event that arrives before the initial pull answers is newer
    // than that answer, so the pull must not overwrite it.
    let heardEvent = false;
    let unlisten: (() => void) | undefined;

    void listen<UpdatePhase>(UPDATE_PHASE_CHANGED, (event) => {
      heardEvent = true;
      if (!cancelled) {
        setPhase(event.payload);
      }
    }).then((u) => {
      if (cancelled) {
        u();
      } else {
        unlisten = u;
      }
    });

    command("update_phase").then(
      (current) => {
        if (!cancelled && !heardEvent) {
          setPhase(current);
        }
      },
      () => {},
    );

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  return phase;
}
