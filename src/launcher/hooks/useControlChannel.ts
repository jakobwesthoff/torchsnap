// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Subscribes to the backend control channel and dispatches
 * incoming commands to the callbacks the Launcher passes in.
 * `reportSnapshot` is answered with `getSnapshot()` through
 * `control_report_snapshot`, under the request's id.
 *
 * The control channel is a Tauri Channel<ControlCommand> —
 * typed, targeted to this webview, and lifecycle-aware. When
 * the webview is destroyed the channel is cleaned up, and the
 * Rust side detects the stale reference.
 *
 * New control commands are added to the ControlCommand union
 * and the switch statement below.
 */

import { useEffect, useRef } from "react";
import { Channel } from "@tauri-apps/api/core";
import { command } from "../../lib/command";

interface UseControlChannelParams {
  resetState: () => void;
  setQuery: (text: string) => void;
  /** What the launcher shows, for the control API's `snapshot`. */
  getSnapshot: () => LauncherSnapshot;
}

import type { ControlCommand, LauncherSnapshot } from "../../types";

export function useControlChannel({ resetState, setQuery, getSnapshot }: UseControlChannelParams) {
  // Keep stable refs to the callbacks so the effect can run exactly
  // once while always dispatching through the latest functions.
  //
  // ESLINT: Both refs are only read inside the channel's onmessage
  // callback, which cannot fire during render. Direct assignment
  // during render is preferable to a useEffect wrapper because it
  // guarantees the ref is current before any post-commit event —
  // a useEffect would leave a gap where an incoming message could
  // dispatch through a stale callback.
  const resetStateRef = useRef(resetState);
  // eslint-disable-next-line react-hooks/refs
  resetStateRef.current = resetState;
  const setQueryRef = useRef(setQuery);
  // eslint-disable-next-line react-hooks/refs
  setQueryRef.current = setQuery;
  const getSnapshotRef = useRef(getSnapshot);
  // eslint-disable-next-line react-hooks/refs
  getSnapshotRef.current = getSnapshot;

  useEffect(() => {
    // Without an answer the backend's request times out, so a failure
    // here is only logged; the channel keeps handling commands.
    // `command()` already logs a rejected report.
    const reportSnapshot = (requestId: number) => {
      let snapshot: LauncherSnapshot;
      try {
        snapshot = getSnapshotRef.current();
      } catch (error) {
        console.error("building the launcher snapshot failed:", error);
        return;
      }
      command("control_report_snapshot", { requestId, snapshot }).catch(() => {});
    };

    const channel = new Channel<ControlCommand>();

    channel.onmessage = (command) => {
      switch (command.type) {
        case "dismiss":
          resetStateRef.current();
          break;
        case "setQuery":
          setQueryRef.current(command.text);
          break;
        case "reportSnapshot":
          reportSnapshot(command.requestId);
          break;
      }
    };

    command("control_subscribe", { channel });

    // The backend replaces the old channel on re-subscribe, so there
    // is no explicit unsubscribe needed. Silencing the handler is
    // enough to prevent leaking closures if this ever re-runs.
    return () => {
      channel.onmessage = () => {};
    };
  }, []);
}
