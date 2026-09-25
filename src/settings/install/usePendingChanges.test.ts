// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { emitTauriEvent, mockCommands } from "../../test/tauri";
import { INSTALL_QUEUE_CHANGED, type PendingGadget } from "./types";
import { usePendingChanges } from "./usePendingChanges";

const replaced: PendingGadget = {
  kind: "replaced",
  name: "Weather",
  description: "Forecasts",
  version: "2.0.0",
  previousVersion: "1.0.0",
};

describe("usePendingChanges", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("pulls the pending changes on mount", async () => {
    mockCommands({ pending_gadget_changes: () => ({ weather: replaced }) });

    const { result } = renderHook(() => usePendingChanges());

    await waitFor(() => expect(result.current.changes).toEqual({ weather: replaced }));
  });

  it("pulls again when the install queue changes", async () => {
    let changes: Record<string, PendingGadget> = {};
    mockCommands({ pending_gadget_changes: () => changes });
    const { result } = renderHook(() => usePendingChanges());
    await waitFor(() => expect(result.current.loaded).toBe(true));

    changes = { weather: replaced };
    await act(() => emitTauriEvent(INSTALL_QUEUE_CHANGED));

    await waitFor(() => expect(result.current.changes).toEqual({ weather: replaced }));
  });

  it("undoes a change and pulls the result", async () => {
    let changes: Record<string, PendingGadget> = { weather: replaced };
    const undone: string[] = [];
    mockCommands({
      pending_gadget_changes: () => changes,
      install_undo: ({ gadgetId }) => {
        undone.push(gadgetId);
        changes = {};
        return { restoredVersion: "1.0.0", requiresRestart: false };
      },
    });
    const { result } = renderHook(() => usePendingChanges());
    await waitFor(() => expect(result.current.loaded).toBe(true));

    await act(() => result.current.undo("weather"));

    expect(undone).toEqual(["weather"]);
    expect(result.current.changes).toEqual({});
  });

  it("reports a failed undo", async () => {
    // `install_undo` deliberately fails, so `command()` logs the
    // rejection. That logging is part of the contract under test, so
    // the warning is captured and asserted on instead of left to print.
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    mockCommands({
      pending_gadget_changes: () => ({ weather: replaced }),
      install_undo: () => {
        throw new Error("nothing to undo for `weather` in this session");
      },
    });
    const { result } = renderHook(() => usePendingChanges());
    await waitFor(() => expect(result.current.loaded).toBe(true));

    await act(() => result.current.undo("weather"));

    expect(result.current.error).toContain("nothing to undo");
    expect(warn).toHaveBeenCalledWith('command("install_undo") failed:', expect.anything());
  });
});
