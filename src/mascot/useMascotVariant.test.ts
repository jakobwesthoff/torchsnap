// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { StrictMode, createElement, type ReactNode } from "react";
import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useMascotVariant } from "./useMascotVariant";
import type { DrawInput } from "./selection";

// The settings, the window's blur listener, the pools and the conditions
// are stand-ins; the draw itself is the real one, watched by a spy so the
// tests can see what each draw was given.
const state = vi.hoisted(() => ({
  settings: new Map<string, unknown>(),
  blurListeners: [] as (() => void)[],
  unlistened: 0,
  draws: [] as DrawInput[],
}));

vi.mock("../hooks/useSetting", () => ({
  useSetting: (key: string) => [state.settings.get(key), vi.fn()],
}));

vi.mock("../settingsStore", () => ({
  getSettingSync: (key: string) => state.settings.get(key),
}));

vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: () => ({
    listen: (event: string, handler: () => void) => {
      if (event === "tauri://blur") state.blurListeners.push(handler);
      return Promise.resolve(() => {
        state.unlistened++;
      });
    },
  }),
}));

vi.mock("./conditions", () => ({ CONDITIONS: {} }));

vi.mock("./selection", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./selection")>();
  return {
    ...actual,
    drawMascot: (input: DrawInput) => {
      state.draws.push(input);
      return actual.drawMascot(input);
    },
  };
});

// Characters in the draw's order: claw (NSFW only), ghost, original,
// robot (an SFW/NSFW pair).
vi.mock("./variants", () => {
  const mascotFacts: Record<string, { character: string; nsfw: boolean }> = {
    "claw-nsfw": { character: "claw", nsfw: true },
    ghost: { character: "ghost", nsfw: false },
    original: { character: "original", nsfw: false },
    robot: { character: "robot", nsfw: false },
    "robot-nsfw": { character: "robot", nsfw: true },
  };
  return {
    ORIGINAL_MASCOT: "original",
    mascotFacts,
    mascotPools: { allYear: Object.keys(mascotFacts), occasions: [] },
    isNsfwVariant: (variant: string) => mascotFacts[variant]?.nsfw === true,
  };
});

/** Makes every random number `value`: 0 picks the first character and
 *  variant, 0.99 the last. */
function randomAlways(value: number) {
  vi.spyOn(Math, "random").mockReturnValue(value);
}

/** The input of the most recent draw. */
function lastDraw(): DrawInput {
  return state.draws[state.draws.length - 1];
}

function blur() {
  act(() => state.blurListeners.forEach((listener) => listener()));
}

function strict({ children }: { children: ReactNode }) {
  return createElement(StrictMode, null, children);
}

beforeEach(() => {
  state.settings.clear();
  state.settings.set("randomMascots", true);
  state.settings.set("showNsfwMascots", true);
  state.settings.set("mascotMode", "center");
  state.blurListeners.length = 0;
  state.unlistened = 0;
  state.draws.length = 0;
});

afterEach(() => {
  vi.restoreAllMocks();
});

// =========================================================
// Settings
// =========================================================

describe("useMascotVariant: settings", () => {
  it("shows the original and no draw while random mascots are off", () => {
    state.settings.set("randomMascots", false);
    randomAlways(0.99);
    const { result } = renderHook(() => useMascotVariant());
    expect(result.current).toEqual({ variant: "original", draw: null });
  });

  it("draws nothing on blur while random mascots are off", () => {
    state.settings.set("randomMascots", false);
    renderHook(() => useMascotVariant());
    const drawsAtMount = state.draws.length;
    blur();
    expect(state.draws).toHaveLength(drawsAtMount);
  });

  it("shows the draw held from before once random mascots are turned on", () => {
    state.settings.set("randomMascots", false);
    randomAlways(0.99);
    const { result, rerender } = renderHook(() => useMascotVariant());
    state.settings.set("randomMascots", true);
    rerender();
    expect(result.current.variant).toBe("robot-nsfw");
    expect(result.current.draw?.variant).toBe("robot-nsfw");
  });

  it("returns the draw it shows", () => {
    randomAlways(0);
    const { result } = renderHook(() => useMascotVariant());
    expect(result.current.variant).toBe("claw-nsfw");
    expect(result.current.draw).toMatchObject({ variant: "claw-nsfw", character: "claw" });
  });
});

// =========================================================
// The recent list
// =========================================================

describe("useMascotVariant: recently shown", () => {
  it("remembers exactly one character after mount and one blur under StrictMode", () => {
    randomAlways(0.5);
    const { result } = renderHook(() => useMascotVariant(), { wrapper: strict });
    const shown = result.current.draw!.character;
    blur();
    expect(lastDraw().recent).toEqual([shown]);
    expect(result.current.draw!.remembered).toBe(1);
  });

  it("remembers two characters after two blurs, most recent first", () => {
    randomAlways(0);
    const { result } = renderHook(() => useMascotVariant(), { wrapper: strict });
    const first = result.current.draw!.character;
    blur();
    const second = result.current.draw!.character;
    blur();
    expect(lastDraw().recent).toEqual([second, first]);
    expect(first).not.toBe(second);
  });

  it("remembers nothing while the mascot is hidden", () => {
    state.settings.set("mascotMode", "off");
    const { result } = renderHook(() => useMascotVariant());
    blur();
    expect(lastDraw().recent).toEqual([]);
    expect(result.current.draw!.remembered).toBe(0);
  });

  it("stops listening when unmounted", async () => {
    const { unmount } = renderHook(() => useMascotVariant());
    unmount();
    await Promise.resolve();
    expect(state.unlistened).toBe(1);
  });
});

// =========================================================
// NSFW
// =========================================================

describe("useMascotVariant: NSFW", () => {
  it("draws without NSFW variants while they are off, and with them after they are turned on", () => {
    state.settings.set("showNsfwMascots", false);
    randomAlways(0);
    const { result, rerender } = renderHook(() => useMascotVariant());
    expect(result.current.variant).toBe("ghost");
    expect(lastDraw().allowNsfw).toBe(false);

    state.settings.set("showNsfwMascots", true);
    rerender();
    expect(result.current.variant).toBe("ghost");

    blur();
    expect(lastDraw().allowNsfw).toBe(true);
    expect(result.current.variant).toBe("claw-nsfw");
  });

  it("swaps an NSFW variant for its SFW twin without a new draw when NSFW is turned off", () => {
    randomAlways(0.99);
    const rendered: string[] = [];
    const { result, rerender } = renderHook(() => {
      const mascot = useMascotVariant();
      rendered.push(mascot.variant);
      return mascot;
    });
    expect(result.current.variant).toBe("robot-nsfw");
    const drawsBefore = state.draws.length;

    rendered.length = 0;
    state.settings.set("showNsfwMascots", false);
    rerender();
    expect(result.current.variant).toBe("robot");
    expect(result.current.draw).toMatchObject({ variant: "robot", character: "robot" });
    expect(state.draws).toHaveLength(drawsBefore);
    expect(rendered).not.toContain("robot-nsfw");
  });

  it("draws again when the character on screen has no SFW variant", () => {
    randomAlways(0);
    const rendered: string[] = [];
    const { result, rerender } = renderHook(() => {
      const mascot = useMascotVariant();
      rendered.push(mascot.variant);
      return mascot;
    });
    expect(result.current.variant).toBe("claw-nsfw");

    rendered.length = 0;
    state.settings.set("showNsfwMascots", false);
    rerender();
    expect(result.current.variant).toBe("ghost");
    expect(lastDraw().allowNsfw).toBe(false);
    expect(rendered.length).toBeGreaterThan(0);
    expect(rendered.every((variant) => variant === "ghost")).toBe(true);
  });

  it("does not bring the NSFW variant back when NSFW is turned on again", () => {
    randomAlways(0.99);
    const { result, rerender } = renderHook(() => useMascotVariant());
    state.settings.set("showNsfwMascots", false);
    rerender();
    state.settings.set("showNsfwMascots", true);
    rerender();
    expect(result.current.variant).toBe("robot");
  });

  it("keeps an SFW variant on screen when NSFW is turned off", () => {
    randomAlways(0.5);
    const { result, rerender } = renderHook(() => useMascotVariant());
    const before = result.current.variant;
    const drawsBefore = state.draws.length;
    state.settings.set("showNsfwMascots", false);
    rerender();
    expect(result.current.variant).toBe(before);
    expect(state.draws).toHaveLength(drawsBefore);
  });

  it("returns no NSFW variant under StrictMode while NSFW is off", () => {
    state.settings.set("showNsfwMascots", false);
    const rendered: string[] = [];
    renderHook(
      () => {
        const mascot = useMascotVariant();
        rendered.push(mascot.variant);
        return mascot;
      },
      { wrapper: strict },
    );
    for (let i = 0; i < 20; i++) blur();
    expect(rendered.filter((variant) => variant.endsWith("-nsfw"))).toEqual([]);
  });
});
