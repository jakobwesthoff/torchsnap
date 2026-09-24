// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { InstalledGadgetInfo } from "../../lib/command";
import { emitTauriEvent, mockCommands } from "../../test/tauri";
import { INSTALL_QUEUE_CHANGED, type InstallRequestView } from "./types";
import { useInstallQueue } from "./useInstallQueue";

function readyRequest(id: string, gadgetId = "weather"): InstallRequestView {
  return {
    id,
    origin: "settingsPicker",
    sourcePath: `/Downloads/${gadgetId}.torchsnap`,
    state: {
      kind: "ready",
      review: {
        gadget: { id: gadgetId, name: "Weather", description: "Forecasts", version: "1.0.0" },
        sourcePath: `/Downloads/${gadgetId}.torchsnap`,
        provenance: null,
        action: { kind: "install" },
        permissions: [],
        removedPermissions: [],
      },
    },
  };
}

function installed(gadgetId = "weather"): InstalledGadgetInfo {
  return {
    id: gadgetId,
    name: "Weather",
    version: "1.0.0",
    previousVersion: null,
    versionRelation: null,
    requiresRestart: true,
  };
}

describe("useInstallQueue", () => {
  it("pulls the queue on mount", async () => {
    mockCommands({ install_queue_snapshot: () => [readyRequest("r1")] });

    const { result } = renderHook(() => useInstallQueue());

    await waitFor(() => expect(result.current.requests).toHaveLength(1));
    expect(result.current.requests[0].id).toBe("r1");
  });

  it("pulls again when the backend announces a change", async () => {
    let queue: InstallRequestView[] = [];
    mockCommands({ install_queue_snapshot: () => queue });
    const { result } = renderHook(() => useInstallQueue());
    await waitFor(() => expect(result.current.loaded).toBe(true));

    queue = [readyRequest("r1")];
    await act(() => emitTauriEvent(INSTALL_QUEUE_CHANGED));

    await waitFor(() => expect(result.current.requests).toHaveLength(1));
  });

  it("confirms a request and records the outcome", async () => {
    const confirmed: string[] = [];
    mockCommands({
      install_queue_snapshot: () => [],
      install_queue_confirm: ({ requestId }) => {
        confirmed.push(requestId);
        return installed();
      },
    });
    const { result } = renderHook(() => useInstallQueue());

    await act(() => result.current.confirm("r1"));

    expect(confirmed).toEqual(["r1"]);
    expect(result.current.results).toEqual([
      { kind: "installed", info: installed(), undone: false, requiresRestart: true },
    ]);
  });

  it("records a failed confirm without throwing", async () => {
    mockCommands({
      install_queue_snapshot: () => [],
      install_queue_confirm: () => {
        throw new Error("changed since this review");
      },
    });
    const { result } = renderHook(() => useInstallQueue());

    await act(() => result.current.confirm("r1"));

    expect(result.current.results).toEqual([]);
    expect(result.current.error).toContain("changed since this review");
  });

  it("dismisses a request", async () => {
    const dismissed: string[] = [];
    mockCommands({
      install_queue_snapshot: () => [],
      install_queue_dismiss: ({ requestId }) => {
        dismissed.push(requestId);
      },
    });
    const { result } = renderHook(() => useInstallQueue());

    await act(() => result.current.dismiss("r1"));

    expect(dismissed).toEqual(["r1"]);
  });

  it("undoes an install and marks its result", async () => {
    const undone: string[] = [];
    mockCommands({
      install_queue_snapshot: () => [],
      install_queue_confirm: () => installed(),
      install_undo: ({ gadgetId }) => {
        undone.push(gadgetId);
        return { restoredVersion: null, requiresRestart: false };
      },
    });
    const { result } = renderHook(() => useInstallQueue());
    await act(() => result.current.confirm("r1"));

    await act(() => result.current.undo("weather"));

    expect(undone).toEqual(["weather"]);
    expect(result.current.results[0].undone).toBe(true);
    // The backend says whether anything is still waiting for a restart.
    expect(result.current.results[0].requiresRestart).toBe(false);
  });

  it("keeps results across several confirms until acknowledged", async () => {
    let next = 0;
    const ids = ["weather", "calendar"];
    mockCommands({
      install_queue_snapshot: () => [],
      install_queue_confirm: () => installed(ids[next++]),
    });
    const { result } = renderHook(() => useInstallQueue());

    await act(() => result.current.confirm("r1"));
    await act(() => result.current.confirm("r2"));
    expect(result.current.results.map((r) => r.info.id)).toEqual(["weather", "calendar"]);

    act(() => result.current.acknowledge());
    expect(result.current.results).toEqual([]);
  });
});
