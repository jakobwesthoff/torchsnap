// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { emitTauriEvent, mockCommands } from "../../test/tauri";
import { INSTALL_QUEUE_CHANGED, type InstallRequestView } from "./types";
import { useQueueArrival } from "./useQueueArrival";

function request(id: string): InstallRequestView {
  return {
    id,
    origin: "osOpenFile",
    sourcePath: `/tmp/${id}.torchsnap`,
    state: { kind: "staging" },
  };
}

describe("useQueueArrival", () => {
  it("fires on mount when requests are already waiting", async () => {
    mockCommands({ install_queue_snapshot: () => [request("r1")] });
    const onArrival = vi.fn();

    renderHook(() => useQueueArrival(onArrival));

    await waitFor(() => expect(onArrival).toHaveBeenCalledTimes(1));
  });

  it("does not fire for an empty queue", async () => {
    let pulls = 0;
    mockCommands({
      install_queue_snapshot: () => {
        pulls += 1;
        return [];
      },
    });
    const onArrival = vi.fn();

    renderHook(() => useQueueArrival(onArrival));

    await waitFor(() => expect(pulls).toBe(1));
    expect(onArrival).not.toHaveBeenCalled();
  });

  it("fires once per empty-to-non-empty transition", async () => {
    let queue: InstallRequestView[] = [];
    mockCommands({ install_queue_snapshot: () => queue });
    const onArrival = vi.fn();
    renderHook(() => useQueueArrival(onArrival));

    queue = [request("r1")];
    await act(() => emitTauriEvent(INSTALL_QUEUE_CHANGED));
    await waitFor(() => expect(onArrival).toHaveBeenCalledTimes(1));

    // Still non-empty: the user may have navigated away on purpose.
    queue = [request("r1"), request("r2")];
    await act(() => emitTauriEvent(INSTALL_QUEUE_CHANGED));
    await act(() => emitTauriEvent(INSTALL_QUEUE_CHANGED));
    expect(onArrival).toHaveBeenCalledTimes(1);

    queue = [];
    await act(() => emitTauriEvent(INSTALL_QUEUE_CHANGED));
    queue = [request("r3")];
    await act(() => emitTauriEvent(INSTALL_QUEUE_CHANGED));
    await waitFor(() => expect(onArrival).toHaveBeenCalledTimes(2));
  });
});
