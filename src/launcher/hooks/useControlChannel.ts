// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Subscribes to the backend control channel and dispatches
 * incoming commands to the appropriate launcher state setters.
 *
 * The control channel is a Tauri Channel<ControlCommand> —
 * typed, targeted to this webview, and lifecycle-aware. When
 * the webview is destroyed the channel is cleaned up, and the
 * Rust side detects the stale reference.
 *
 * New control commands are added to the ControlCommand union
 * and the switch statement below. The Launcher component
 * itself never changes.
 */

import { useEffect } from "react";
import { Channel, invoke } from "@tauri-apps/api/core";

interface UseControlChannelParams {
  resetState: () => void;
  setQuery: (text: string) => void;
}

type ControlCommand =
  | { type: "dismiss" }
  | { type: "setQuery"; text: string };

export function useControlChannel({
  resetState,
  setQuery,
}: UseControlChannelParams) {
  useEffect(() => {
    const channel = new Channel<ControlCommand>();

    channel.onmessage = (command) => {
      switch (command.type) {
        case "dismiss":
          resetState();
          break;
        case "setQuery":
          setQuery(command.text);
          break;
      }
    };

    invoke("control_subscribe", { channel });
  }, [resetState, setQuery]);
}
