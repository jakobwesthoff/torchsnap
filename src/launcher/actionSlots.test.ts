// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { describe, expect, it } from "vitest";
import type { Action } from "../types";
import { SLOT_KEYS, actionLabel, actionsToFooterState, hasSlot, slotBindings } from "./actionSlots";

describe("SLOT_KEYS", () => {
  it("binds every slot to its fixed key", () => {
    expect(SLOT_KEYS).toEqual({
      primary: { modifiers: [], key: "Enter" },
      secondary: { modifiers: ["Meta"], key: "Enter" },
      copy: { modifiers: ["Meta"], key: "c" },
      reveal: { modifiers: ["Meta", "Shift"], key: "r" },
      delete: { modifiers: ["Meta"], key: "Backspace" },
      openSettings: { modifiers: ["Meta"], key: "," },
    });
  });
});

describe("actionLabel", () => {
  it("prefers the gadget's own label", () => {
    expect(actionLabel({ slot: "copy", label: "Copy network id" }, "Home")).toBe("Copy network id");
  });

  it("falls back to the slot's default label", () => {
    expect(actionLabel({ slot: "copy", label: null }, "Home")).toBe("Copy");
    expect(actionLabel({ slot: "reveal", label: null }, "Home")).toBe("Reveal in Finder");
    expect(actionLabel({ slot: "delete", label: null }, "Home")).toBe("Delete");
    expect(actionLabel({ slot: "openSettings", label: null }, "Home")).toBe("Open settings");
  });

  it("shows the entry title for an unlabeled primary or secondary action", () => {
    expect(actionLabel({ slot: "primary", label: null }, "Safari")).toBe("Safari");
    expect(actionLabel({ slot: "secondary", label: null }, "Safari")).toBe("Safari");
  });
});

describe("hasSlot", () => {
  const actions: Action[] = [{ slot: "copy", label: null }];

  it("finds a filled slot", () => {
    expect(hasSlot(actions, "copy")).toBe(true);
  });

  it("reports an empty slot", () => {
    expect(hasSlot(actions, "primary")).toBe(false);
    expect(hasSlot([], "primary")).toBe(false);
  });
});

describe("actionsToFooterState", () => {
  it("shows the primary action on Enter and every other slot as a hint", () => {
    const footer = actionsToFooterState(
      [
        { slot: "primary", label: "Join" },
        { slot: "copy", label: "Copy network id" },
        { slot: "delete", label: null },
      ],
      "Home",
    );

    expect(footer).toEqual({
      primary: { combo: { modifiers: [], key: "Enter" }, label: "Join" },
      hints: [
        { combo: { modifiers: ["Meta"], key: "c" }, label: "Copy network id" },
        { combo: { modifiers: ["Meta"], key: "Backspace" }, label: "Delete" },
      ],
    });
  });

  it("has no primary hint for an entry without a primary action", () => {
    const footer = actionsToFooterState([{ slot: "copy", label: null }], "🦀");

    expect(footer.primary).toBeUndefined();
    expect(footer.hints).toEqual([{ combo: { modifiers: ["Meta"], key: "c" }, label: "Copy" }]);
  });

  it("is empty without actions", () => {
    expect(actionsToFooterState([], "Nothing")).toEqual({ primary: undefined, hints: [] });
  });
});

describe("slotBindings", () => {
  it("binds every slot except primary, which Enter covers", () => {
    expect(
      slotBindings([
        { slot: "primary", label: "Open" },
        { slot: "secondary", label: "Reveal in Finder" },
        { slot: "openSettings", label: null },
      ]),
    ).toEqual([
      { slot: "secondary", combo: { modifiers: ["Meta"], key: "Enter" } },
      { slot: "openSettings", combo: { modifiers: ["Meta"], key: "," } },
    ]);
  });

  it("binds nothing without actions", () => {
    expect(slotBindings([])).toEqual([]);
  });
});
