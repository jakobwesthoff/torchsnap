// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Tauri IPC test doubles
//
// Frontend code talks to the backend only through `command()` and
// Tauri events. These helpers replace the IPC layer underneath both,
// so tests exercise the real `command()` wrapper and real `listen()`
// calls against handlers typed by the same `CommandMap` the app uses.
// =========================================================

import { emit } from "@tauri-apps/api/event";
import { mockIPC } from "@tauri-apps/api/mocks";
import type { CommandMap, CommandName } from "../lib/command";

type CommandHandlers = {
  [C in CommandName]?: (
    params: CommandMap[C]["params"],
  ) => CommandMap[C]["result"] | Promise<CommandMap[C]["result"]>;
};

/**
 * Answer backend commands with the given handlers. A command without a
 * handler rejects, so a test fails loudly when the code under test
 * calls something the test did not expect.
 */
export function mockCommands(handlers: CommandHandlers): void {
  mockIPC(
    (cmd, payload) => {
      const handler = handlers[cmd as CommandName] as ((params: unknown) => unknown) | undefined;
      if (!handler) {
        throw new Error(`unexpected command \`${cmd}\` in test`);
      }
      return handler(payload);
    },
    { shouldMockEvents: true },
  );
}

/**
 * Deliver a Tauri event to listeners registered through `listen()`.
 * Requires `mockCommands` to have been called in the same test, since
 * that is what switches the event system to its mocked form.
 */
export async function emitTauriEvent(event: string, payload?: unknown): Promise<void> {
  await emit(event, payload);
}
