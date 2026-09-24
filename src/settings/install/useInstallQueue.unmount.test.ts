// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// Kept in its own file because it replaces `listen` for the whole
// module, which the other hook tests need for real.

import { renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { mockCommands } from "../../test/tauri";

let resolveListen: ((unlisten: () => void) => void) | undefined;

vi.mock("@tauri-apps/api/event", () => ({
  listen: () =>
    new Promise<() => void>((resolve) => {
      resolveListen = resolve;
    }),
}));

const { useInstallQueue } = await import("./useInstallQueue");

describe("useInstallQueue listener cleanup", () => {
  it("removes the listener even when unmounting before it was registered", async () => {
    mockCommands({ install_queue_snapshot: () => [] });
    const unlisten = vi.fn();

    const { unmount } = renderHook(() => useInstallQueue());
    unmount();
    resolveListen?.(unlisten);
    await Promise.resolve();

    expect(unlisten).toHaveBeenCalledTimes(1);
  });
});
