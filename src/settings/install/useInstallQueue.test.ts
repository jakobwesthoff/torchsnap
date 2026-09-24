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

  it("confirms a request", async () => {
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
    expect(result.current.error).toBeNull();
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
});
