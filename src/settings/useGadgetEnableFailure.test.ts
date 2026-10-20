// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { renderHook, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { emitTauriEvent, mockCommands } from "../test/tauri";
import { GADGET_ENABLE_FAILURES_CHANGED, useGadgetEnableFailure } from "./useGadgetEnableFailure";

// `listen` passes through to the real (mocked-IPC) implementation, but a
// test can hold its registration back to see what happens meanwhile.
const registration = vi.hoisted(() => ({ held: null as Promise<void> | null }));
vi.mock("@tauri-apps/api/event", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@tauri-apps/api/event")>();
  return {
    ...actual,
    listen: (async (...args: Parameters<typeof actual.listen>) => {
      if (registration.held) {
        await registration.held;
      }
      return actual.listen(...args);
    }) as typeof actual.listen,
  };
});

describe("useGadgetEnableFailure", () => {
  afterEach(() => {
    registration.held = null;
    vi.restoreAllMocks();
  });

  it("returns the reason the backend keeps for the gadget", async () => {
    mockCommands({ gadget_enable_failures: () => ({ weather: "guest enable() failed: no key" }) });

    const { result } = renderHook(() => useGadgetEnableFailure("weather"));

    await waitFor(() => expect(result.current).toBe("guest enable() failed: no key"));
  });

  it("returns null when only other gadgets failed", async () => {
    let asked = false;
    mockCommands({
      gadget_enable_failures: () => {
        asked = true;
        return { calculator: "broken" };
      },
    });

    const { result } = renderHook(() => useGadgetEnableFailure("weather"));

    await waitFor(() => expect(asked).toBe(true));
    expect(result.current).toBeNull();
  });

  it("follows the failure as it changes", async () => {
    let failures: Record<string, string> = { weather: "no key" };
    mockCommands({ gadget_enable_failures: () => failures });
    const { result } = renderHook(() => useGadgetEnableFailure("weather"));
    await waitFor(() => expect(result.current).toBe("no key"));

    failures = {};
    await emitTauriEvent(GADGET_ENABLE_FAILURES_CHANGED);

    await waitFor(() => expect(result.current).toBeNull());
  });

  it("returns null when the failures cannot be read", async () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    mockCommands({});

    const { result } = renderHook(() => useGadgetEnableFailure("weather"));

    await waitFor(() =>
      expect(warn).toHaveBeenCalledWith(
        'command("gadget_enable_failures") failed:',
        expect.anything(),
      ),
    );
    expect(result.current).toBeNull();
  });

  it("reads the failures only once the listener is registered", async () => {
    let reads = 0;
    mockCommands({
      gadget_enable_failures: () => {
        reads += 1;
        return { weather: "no key" };
      },
    });
    let register!: () => void;
    registration.held = new Promise((resolve) => (register = resolve));

    const { result } = renderHook(() => useGadgetEnableFailure("weather"));
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(reads).toBe(0);

    register();

    await waitFor(() => expect(result.current).toBe("no key"));
    expect(reads).toBe(1);
  });

  it("still reads the failures when the listener cannot be registered", async () => {
    mockCommands({ gadget_enable_failures: () => ({ weather: "no key" }) });
    registration.held = Promise.reject(new Error("no event system"));

    const { result } = renderHook(() => useGadgetEnableFailure("weather"));

    await waitFor(() => expect(result.current).toBe("no key"));
  });

  it("stops listening once unmounted", async () => {
    let reads = 0;
    mockCommands({
      gadget_enable_failures: () => {
        reads += 1;
        return {};
      },
    });
    const { unmount } = renderHook(() => useGadgetEnableFailure("weather"));
    await waitFor(() => expect(reads).toBe(1));

    unmount();
    await emitTauriEvent(GADGET_ENABLE_FAILURES_CHANGED);

    expect(reads).toBe(1);
  });

  it("drops a registration that completes after unmount", async () => {
    let reads = 0;
    mockCommands({
      gadget_enable_failures: () => {
        reads += 1;
        return {};
      },
    });
    let register!: () => void;
    registration.held = new Promise((resolve) => (register = resolve));
    const { unmount } = renderHook(() => useGadgetEnableFailure("weather"));

    unmount();
    register();
    await new Promise((resolve) => setTimeout(resolve, 0));
    await emitTauriEvent(GADGET_ENABLE_FAILURES_CHANGED);

    expect(reads).toBe(0);
  });
});
