// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useMascotVariant } from "./useMascotVariant";

// The settings, the window's blur listener and the two draws of a roll
// are stand-ins, so each test decides what the roll returns.
const state = vi.hoisted(() => ({
  settings: new Map<string, unknown>(),
  blurListeners: [] as (() => void)[],
  // What the next roll draws from the full pool and from the SFW pool.
  nextFull: [] as string[],
  nextSfw: [] as string[],
}));

vi.mock("./useSetting", () => ({
  useSetting: (key: string) => [state.settings.get(key), vi.fn()],
}));

vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: () => ({
    listen: (event: string, handler: () => void) => {
      if (event === "tauri://blur") state.blurListeners.push(handler);
      return Promise.resolve(() => {});
    },
  }),
}));

vi.mock("./useRandomMascot", () => ({
  selectMascotVariant: (_sets: unknown, filter?: (variant: string) => boolean) =>
    filter ? state.nextSfw.shift() : state.nextFull.shift(),
}));

vi.mock("../mascotVariants", () => ({
  SnappyHeroSets: [],
  isNsfwVariant: (variant: string) => variant.endsWith("-nsfw"),
}));

/** Queues the draws of the next roll. */
function nextRoll(fromFullPool: string, fromSfwPool: string) {
  state.nextFull.push(fromFullPool);
  state.nextSfw.push(fromSfwPool);
}

beforeEach(() => {
  state.settings.clear();
  state.settings.set("randomMascots", true);
  state.settings.set("showNsfwMascots", true);
  state.blurListeners.length = 0;
  state.nextFull.length = 0;
  state.nextSfw.length = 0;
});

describe("useMascotVariant", () => {
  it("shows the original when random mascots are off", () => {
    state.settings.set("randomMascots", false);
    nextRoll("robot", "robot");
    const { result } = renderHook(() => useMascotVariant());
    expect(result.current.variant).toBe("original");
  });

  it("shows the full pool's draw while NSFW mascots are allowed", () => {
    nextRoll("robot-nsfw", "robot");
    const { result } = renderHook(() => useMascotVariant());
    expect(result.current.variant).toBe("robot-nsfw");
  });

  it("swaps an NSFW draw for the SFW draw while NSFW mascots are off", () => {
    state.settings.set("showNsfwMascots", false);
    nextRoll("robot-nsfw", "ghost");
    const { result } = renderHook(() => useMascotVariant());
    expect(result.current.variant).toBe("ghost");
  });

  it("keeps a safe draw while NSFW mascots are off", () => {
    state.settings.set("showNsfwMascots", false);
    nextRoll("robot", "ghost");
    const { result } = renderHook(() => useMascotVariant());
    expect(result.current.variant).toBe("robot");
  });

  it("follows a settings change without rolling again", () => {
    nextRoll("robot-nsfw", "ghost");
    const { result, rerender } = renderHook(() => useMascotVariant());
    expect(result.current.variant).toBe("robot-nsfw");

    state.settings.set("showNsfwMascots", false);
    rerender();
    expect(result.current.variant).toBe("ghost");

    state.settings.set("showNsfwMascots", true);
    rerender();
    expect(result.current.variant).toBe("robot-nsfw");
  });

  it("rolls again when the launcher window loses focus", () => {
    nextRoll("robot", "robot");
    const { result } = renderHook(() => useMascotVariant());
    expect(result.current.variant).toBe("robot");

    nextRoll("ghost", "ghost");
    act(() => state.blurListeners.forEach((listener) => listener()));
    expect(result.current.variant).toBe("ghost");
  });
});
