// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { afterEach, describe, expect, it, vi } from "vitest";
import {
  detectPlatform,
  formatModifiersForPlatform,
  physicalModifierFor,
  type Platform,
} from "./platform";

describe("detectPlatform", () => {
  afterEach(() => vi.unstubAllGlobals());

  it.each<[string, Platform]>([
    ["MacIntel", "macos"],
    ["Win32", "windows"],
    ["Linux x86_64", "linux"],
    ["", "linux"],
  ])("reads navigator.platform %j as %s", (raw, expected) => {
    vi.stubGlobal("navigator", { platform: raw });
    expect(detectPlatform()).toBe(expected);
  });

  it("prefers navigator.userAgentData.platform", () => {
    vi.stubGlobal("navigator", { userAgentData: { platform: "Windows" }, platform: "MacIntel" });
    expect(detectPlatform()).toBe("windows");
  });
});

describe("physicalModifierFor", () => {
  it("maps Meta to Cmd on macOS and to Ctrl elsewhere", () => {
    expect(physicalModifierFor("Meta", "macos")).toBe("Super");
    expect(physicalModifierFor("Meta", "windows")).toBe("Control");
    expect(physicalModifierFor("Meta", "linux")).toBe("Control");
  });

  it("maps Ctrl, Alt and Shift to themselves", () => {
    expect(physicalModifierFor("Ctrl", "macos")).toBe("Control");
    expect(physicalModifierFor("Alt", "linux")).toBe("Alt");
    expect(physicalModifierFor("Shift", "windows")).toBe("Shift");
  });
});

describe("formatModifiersForPlatform", () => {
  const all = ["Super", "Shift", "Alt", "Control"] as const;

  it("orders modifiers Ctrl, Alt, Shift, Cmd on macOS with its symbols", () => {
    expect(formatModifiersForPlatform([...all], "macos")).toEqual(["⌃", "⌥", "⇧", "⌘"]);
  });

  it("names the Windows key Win on Windows", () => {
    expect(formatModifiersForPlatform([...all], "windows")).toEqual([
      "Ctrl",
      "Alt",
      "Shift",
      "Win",
    ]);
  });

  it("names the Super key Super on Linux", () => {
    expect(formatModifiersForPlatform([...all], "linux")).toEqual([
      "Ctrl",
      "Alt",
      "Shift",
      "Super",
    ]);
  });

  it("shows a modifier listed twice once", () => {
    expect(formatModifiersForPlatform(["Control", "Control"], "linux")).toEqual(["Ctrl"]);
  });
});

// Keybinding modifiers (as launcher footer pills show them) in every
// subset: Meta and Ctrl are one key outside macOS, so they show once.
describe("keybinding modifiers in every combination", () => {
  const MODIFIERS = ["Meta", "Ctrl", "Alt", "Shift"] as const;
  const subsets = Array.from({ length: 16 }, (_, mask) =>
    MODIFIERS.filter((_, bit) => (mask & (1 << bit)) !== 0),
  );

  it("shows Cmd and Ctrl apart on macOS", () => {
    for (const held of subsets) {
      const expected = [
        held.includes("Ctrl") && "⌃",
        held.includes("Alt") && "⌥",
        held.includes("Shift") && "⇧",
        held.includes("Meta") && "⌘",
      ].filter(Boolean);
      const physical = [...held].reverse().map((m) => physicalModifierFor(m, "macos"));
      expect(formatModifiersForPlatform(physical, "macos"), held.join("+")).toEqual(expected);
    }
  });

  it("shows Meta and Ctrl as one Ctrl on Windows and Linux", () => {
    for (const p of ["windows", "linux"] as const) {
      for (const held of subsets) {
        const expected = [
          (held.includes("Ctrl") || held.includes("Meta")) && "Ctrl",
          held.includes("Alt") && "Alt",
          held.includes("Shift") && "Shift",
        ].filter(Boolean);
        const physical = [...held].reverse().map((m) => physicalModifierFor(m, p));
        expect(formatModifiersForPlatform(physical, p), `${p} ${held.join("+")}`).toEqual(expected);
      }
    }
  });
});
