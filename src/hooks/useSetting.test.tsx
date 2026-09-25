// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useSetting } from "./useSetting";

// An in-memory stand-in for the settings store: a cache, listeners, and
// a write that updates the cache before notifying, like the real store.
const store = vi.hoisted(() => {
  const cache = new Map<string, unknown>();
  const listeners = new Set<(key: string, value: unknown) => void>();
  return {
    cache,
    listeners,
    change(key: string, value: unknown) {
      cache.set(key, value);
      for (const listener of listeners) listener(key, value);
    },
    setSetting: vi.fn(async (key: string, value: unknown) => {
      cache.set(key, value);
      for (const listener of listeners) listener(key, value);
    }),
  };
});

vi.mock("../settingsStore", () => ({
  getSettingSync: (key: string) => store.cache.get(key),
  setSetting: store.setSetting,
  subscribe: (listener: (key: string, value: unknown) => void) => {
    store.listeners.add(listener);
    return () => store.listeners.delete(listener);
  },
}));

describe("useSetting", () => {
  beforeEach(() => {
    store.cache.clear();
    store.listeners.clear();
    store.setSetting.mockClear();
    store.cache.set("enabled.clipboard-manager", true);
    store.cache.set("enabled.weather", false);
  });

  it("returns the stored value on the first render", () => {
    const { result } = renderHook(() => useSetting<boolean>("enabled.clipboard-manager"));
    expect(result.current[0]).toBe(true);
  });

  it("follows changes to its key", () => {
    const { result } = renderHook(() => useSetting<boolean>("enabled.clipboard-manager"));
    act(() => store.change("enabled.clipboard-manager", false));
    expect(result.current[0]).toBe(false);
  });

  it("ignores changes to other keys", () => {
    const { result } = renderHook(() => useSetting<boolean>("enabled.clipboard-manager"));
    act(() => store.change("enabled.weather", true));
    expect(result.current[0]).toBe(true);
  });

  it("returns the new key's value as soon as the key changes", () => {
    const { result, rerender } = renderHook(({ key }) => useSetting<boolean>(key), {
      initialProps: { key: "__internal__.no-active-gadget" },
    });
    expect(result.current[0]).toBeUndefined();

    rerender({ key: "enabled.clipboard-manager" });

    expect(result.current[0]).toBe(true);
  });

  it("follows the new key and no longer the old one after a key change", () => {
    const { result, rerender } = renderHook(({ key }) => useSetting<boolean>(key), {
      initialProps: { key: "enabled.clipboard-manager" },
    });
    rerender({ key: "enabled.weather" });

    act(() => store.change("enabled.clipboard-manager", false));
    expect(result.current[0]).toBe(false);
    act(() => store.change("enabled.weather", true));
    expect(result.current[0]).toBe(true);
  });

  it("writes to the current key", async () => {
    const { result, rerender } = renderHook(({ key }) => useSetting<boolean>(key), {
      initialProps: { key: "enabled.clipboard-manager" },
    });
    rerender({ key: "enabled.weather" });

    await act(() => result.current[1](true));

    expect(store.setSetting).toHaveBeenCalledExactlyOnceWith("enabled.weather", true);
  });
});
