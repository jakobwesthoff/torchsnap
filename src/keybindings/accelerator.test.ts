// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { describe, expect, it } from "vitest";
import { accelToDisplayParts, accelToDisplayPartsForPlatform } from "./accelerator";
import { formatKey, type Platform } from "./platform";

describe("accelToDisplayPartsForPlatform", () => {
  it("shows CommandOrControl as Cmd on macOS and as Ctrl elsewhere", () => {
    for (const token of ["CommandOrControl", "CmdOrCtrl", "CommandOrCtrl", "CmdOrControl"]) {
      expect(accelToDisplayPartsForPlatform(`${token}+K`, "macos")).toEqual(["⌘", "K"]);
      expect(accelToDisplayPartsForPlatform(`${token}+K`, "windows")).toEqual(["Ctrl", "K"]);
    }
  });

  it("shows Control and Super as separate keys", () => {
    expect(accelToDisplayPartsForPlatform("Super+Control+K", "macos")).toEqual(["⌃", "⌘", "K"]);
    expect(accelToDisplayPartsForPlatform("Super+Control+K", "windows")).toEqual([
      "Ctrl",
      "Win",
      "K",
    ]);
    expect(accelToDisplayPartsForPlatform("Super+Control+K", "linux")).toEqual([
      "Ctrl",
      "Super",
      "K",
    ]);
  });

  it("accepts every spelling the shortcut parser accepts, in any case", () => {
    expect(accelToDisplayPartsForPlatform("cmd+ctrl+option+shift+K", "macos")).toEqual([
      "⌃",
      "⌥",
      "⇧",
      "⌘",
      "K",
    ]);
    expect(accelToDisplayPartsForPlatform("Command+Control+Alt+K", "macos")).toEqual([
      "⌃",
      "⌥",
      "⌘",
      "K",
    ]);
  });

  it("puts modifiers in one order however the accelerator lists them", () => {
    expect(accelToDisplayPartsForPlatform("CmdOrCtrl+Shift+Space", "macos")).toEqual([
      "⇧",
      "⌘",
      "Space",
    ]);
    expect(accelToDisplayPartsForPlatform("Shift+CmdOrCtrl+Space", "windows")).toEqual([
      "Ctrl",
      "Shift",
      "Space",
    ]);
  });

  it("formats a lone key", () => {
    expect(accelToDisplayPartsForPlatform("F5", "macos")).toEqual(["F5"]);
  });
});

describe("accelToDisplayParts", () => {
  // jsdom reports no Mac and no Windows, so the Linux labels apply.
  it("uses the detected platform", () => {
    expect(accelToDisplayParts("Super+Control+Space")).toEqual([
      "Ctrl",
      "Super",
      formatKey("Space"),
    ]);
  });
});

// Every modifier subset, written in reverse display order, on every
// platform: the keycaps always come out Ctrl, Alt, Shift, Cmd/Win/Super.
describe("every modifier combination", () => {
  const TOKENS = ["Control", "Alt", "Shift", "Super"] as const;
  const LABELS: Record<Platform, Record<(typeof TOKENS)[number], string>> = {
    macos: { Control: "⌃", Alt: "⌥", Shift: "⇧", Super: "⌘" },
    windows: { Control: "Ctrl", Alt: "Alt", Shift: "Shift", Super: "Win" },
    linux: { Control: "Ctrl", Alt: "Alt", Shift: "Shift", Super: "Super" },
  };
  const subsets = Array.from({ length: 16 }, (_, mask) =>
    TOKENS.filter((_, bit) => (mask & (1 << bit)) !== 0),
  );

  for (const p of ["macos", "windows", "linux"] as const) {
    it(`shows every subset in display order on ${p}`, () => {
      for (const held of subsets) {
        const accelerator = [...[...held].reverse(), "K"].join("+");
        expect(accelToDisplayPartsForPlatform(accelerator, p), accelerator).toEqual([
          ...held.map((t) => LABELS[p][t]),
          "K",
        ]);
      }
    });

    it(`resolves CommandOrControl in every subset on ${p}`, () => {
      const primary = p === "macos" ? "Super" : "Control";
      for (const held of subsets.filter((h) => !h.includes(primary))) {
        const accelerator = ["CommandOrControl", ...held, "K"].join("+");
        const expected = TOKENS.filter((t) => t === primary || held.includes(t));
        expect(accelToDisplayPartsForPlatform(accelerator, p), accelerator).toEqual([
          ...expected.map((t) => LABELS[p][t]),
          "K",
        ]);
      }
    });
  }
});
