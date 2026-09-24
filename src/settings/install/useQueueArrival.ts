// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Queue arrival
//
// Calls `onArrival` when install requests appear: on mount if
// some are already waiting, and whenever the queue goes from
// empty to non-empty. It stays quiet while the queue remains
// non-empty, so a user who navigates away from the review is not
// pulled back by every later change.
// =========================================================

import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { command } from "../../lib/command";
import { INSTALL_QUEUE_CHANGED } from "./types";

export function useQueueArrival(onArrival: () => void): void {
  // The latest callback, read when the queue changes, so the effect
  // does not re-subscribe on every render.
  const onArrivalRef = useRef(onArrival);
  useEffect(() => {
    onArrivalRef.current = onArrival;
  }, [onArrival]);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    let wasEmpty = true;

    const pull = () =>
      command("install_queue_snapshot").then(
        (snapshot) => {
          if (cancelled) {
            return;
          }
          const isEmpty = snapshot.length === 0;
          if (wasEmpty && !isEmpty) {
            onArrivalRef.current();
          }
          wasEmpty = isEmpty;
        },
        () => {},
      );

    void pull();
    void listen(INSTALL_QUEUE_CHANGED, () => void pull()).then((u) => {
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
  }, []);
}
