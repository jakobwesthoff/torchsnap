// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { emitTauriEvent, mockCommands } from "../test/tauri";
import { UPDATE_PHASE_CHANGED, type UpdatePhase } from "./types";
import { useUpdatePhase } from "./useUpdatePhase";

describe("useUpdatePhase", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("starts with nothing and then shows the backend's phase", async () => {
    mockCommands({ update_phase: () => ({ phase: "checking" }) });
    const { result } = renderHook(() => useUpdatePhase());

    expect(result.current).toBeNull();
    await waitFor(() => expect(result.current).toEqual({ phase: "checking" }));
  });

  it("follows phase changes", async () => {
    mockCommands({ update_phase: () => ({ phase: "checking" }) });
    const { result } = renderHook(() => useUpdatePhase());
    await waitFor(() => expect(result.current).toEqual({ phase: "checking" }));

    await act(() =>
      emitTauriEvent(UPDATE_PHASE_CHANGED, { phase: "upToDate", installed: "0.12.0" }),
    );
    expect(result.current).toEqual({ phase: "upToDate", installed: "0.12.0" });
  });

  it("does not let a late initial answer overwrite a newer event", async () => {
    let answer: (phase: UpdatePhase) => void = () => {};
    mockCommands({
      update_phase: () => new Promise<UpdatePhase>((resolve) => (answer = resolve)),
    });
    const { result } = renderHook(() => useUpdatePhase());

    await act(() => emitTauriEvent(UPDATE_PHASE_CHANGED, { phase: "failed", message: "offline" }));
    await act(async () => answer({ phase: "checking" }));

    expect(result.current).toEqual({ phase: "failed", message: "offline" });
  });

  it("stays empty when the backend cannot answer", async () => {
    // `update_phase` deliberately fails, so `command()` logs the
    // rejection. That logging is part of the contract under test, so
    // the warning is captured and asserted on instead of left to print.
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    mockCommands({
      update_phase: () => {
        throw new Error("no state");
      },
    });
    const { result } = renderHook(() => useUpdatePhase());
    await act(async () => {});
    expect(result.current).toBeNull();
    expect(warn).toHaveBeenCalledWith('command("update_phase") failed:', expect.anything());
  });

  it("stops listening after unmount", async () => {
    // `@tauri-apps/api`'s mocked event system has a field-name mismatch
    // between `listen()`'s unlisten call (`eventId`) and the mock's
    // listener removal (`args.id`, see `mockIPC` in
    // `@tauri-apps/api/mocks`), so unlisten only removes the callback,
    // never the stale listener entry. The event emitted after unmount
    // below still finds that stale entry and warns that the callback is
    // gone, even though the hook correctly stopped applying updates
    // (`result.current` stays at "idle"). That warning is a known quirk
    // of the mock, not a defect in the hook, so it is captured and
    // asserted on here rather than left to print.
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    mockCommands({ update_phase: () => ({ phase: "idle" }) });
    const { result, unmount } = renderHook(() => useUpdatePhase());
    await waitFor(() => expect(result.current).toEqual({ phase: "idle" }));

    unmount();
    await act(() => emitTauriEvent(UPDATE_PHASE_CHANGED, { phase: "checking" }));
    expect(result.current).toEqual({ phase: "idle" });
    expect(warn).toHaveBeenCalledWith(expect.stringContaining("Couldn't find callback id"));
  });
});
