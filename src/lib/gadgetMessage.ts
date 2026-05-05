// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Send a message to a plugin's backend `handle_message` handler.
 *
 * Wraps the `plugin_message` Tauri command with automatic Channel
 * creation. Callers that don't need streaming can omit `onMessage`
 * — a no-op channel is created internally so the backend command
 * receives its required parameter.
 */

import { Channel } from "@tauri-apps/api/core";
import { command } from "./command";

export function sendGadgetMessage<TPayload = unknown, TResult = unknown, TStream = never>(
  source: string,
  method: string,
  payload: TPayload,
  onMessage?: (msg: TStream) => void,
): Promise<TResult> {
  const channel = new Channel<TStream>();
  if (onMessage) {
    channel.onmessage = onMessage;
  }

  // The command registry types `plugin_message` as returning `unknown`
  // because the actual return type varies per plugin. The caller's
  // generic `TResult` narrows it at each call site.
  return command("plugin_message", {
    source,
    method,
    payload,
    channel: channel as Channel<unknown>,
  }) as Promise<TResult>;
}
