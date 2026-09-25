// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { describe, expect, it } from "vitest";
import { matchesCombo, type KeyCombo, type ModifierKey } from "./matching";

interface Held {
  meta?: boolean;
  ctrl?: boolean;
  alt?: boolean;
  shift?: boolean;
}

function keydown(
  key: string,
  { meta = false, ctrl = false, alt = false, shift = false }: Held = {},
) {
  return new KeyboardEvent("keydown", {
    key,
    metaKey: meta,
    ctrlKey: ctrl,
    altKey: alt,
    shiftKey: shift,
  });
}

function combo(key: string, ...modifiers: ModifierKey[]): KeyCombo {
  return { key, modifiers };
}

const MAC = true;
const OTHER = false;

describe("matchesCombo", () => {
  describe("letter keys", () => {
    it("matches a lowercase letter binding", () => {
      expect(matchesCombo(keydown("d", { ctrl: true }), combo("d", "Ctrl"), MAC)).toBe(true);
    });

    it("keeps matching while CapsLock turns the produced letter uppercase", () => {
      expect(matchesCombo(keydown("D", { ctrl: true }), combo("d", "Ctrl"), MAC)).toBe(true);
      expect(matchesCombo(keydown("U", { meta: true }), combo("u", "Meta"), MAC)).toBe(true);
    });

    it("matches Shift+letter, whose event carries the uppercase letter", () => {
      expect(matchesCombo(keydown("K", { shift: true }), combo("k", "Shift"), MAC)).toBe(true);
    });

    // The host's default keybindings for Reveal and OpenWith have this shape.
    it("matches the host default Meta+Shift+r", () => {
      const event = keydown("R", { meta: true, shift: true });
      expect(matchesCombo(event, combo("r", "Meta", "Shift"), MAC)).toBe(true);
    });

    it("keeps Shift strict: a Shift-less binding does not fire with Shift held", () => {
      expect(matchesCombo(keydown("K", { meta: true, shift: true }), combo("k", "Meta"), MAC)).toBe(
        false,
      );
    });

    it("keeps Shift strict: a Shift binding does not fire without Shift", () => {
      expect(matchesCombo(keydown("k", { meta: true }), combo("k", "Meta", "Shift"), MAC)).toBe(
        false,
      );
    });

    it("treats an uppercase letter in the binding like its lowercase form", () => {
      expect(matchesCombo(keydown("k", { meta: true }), combo("K", "Meta"), MAC)).toBe(true);
      expect(matchesCombo(keydown("K", { meta: true, shift: true }), combo("K", "Meta"), MAC)).toBe(
        false,
      );
      expect(
        matchesCombo(keydown("K", { meta: true, shift: true }), combo("K", "Meta", "Shift"), MAC),
      ).toBe(true);
    });

    it("does not match a different letter", () => {
      expect(matchesCombo(keydown("j", { meta: true }), combo("k", "Meta"), MAC)).toBe(false);
    });
  });

  describe("non-letter keys", () => {
    it("allows Shift for characters that need it without listing it", () => {
      expect(matchesCombo(keydown("?", { shift: true }), combo("?"), MAC)).toBe(true);
    });

    it("requires Shift when the binding lists it", () => {
      expect(matchesCombo(keydown("Enter"), combo("Enter", "Shift"), MAC)).toBe(false);
      expect(matchesCombo(keydown("Enter", { shift: true }), combo("Enter", "Shift"), MAC)).toBe(
        true,
      );
    });

    it("compares named keys case-sensitively", () => {
      expect(matchesCombo(keydown("enter"), combo("Enter"), MAC)).toBe(false);
    });
  });

  describe("Meta and Ctrl mapping", () => {
    it("maps Meta to Cmd on macOS", () => {
      expect(matchesCombo(keydown("c", { meta: true }), combo("c", "Meta"), MAC)).toBe(true);
      expect(matchesCombo(keydown("c", { ctrl: true }), combo("c", "Meta"), MAC)).toBe(false);
    });

    it("maps both Meta and Ctrl to the Ctrl key elsewhere", () => {
      const ctrlC = keydown("c", { ctrl: true });
      expect(matchesCombo(ctrlC, combo("c", "Meta"), OTHER)).toBe(true);
      expect(matchesCombo(ctrlC, combo("c", "Ctrl"), OTHER)).toBe(true);
      expect(matchesCombo(ctrlC, combo("c", "Meta", "Ctrl"), OTHER)).toBe(true);
    });

    it("keeps Ctrl letter bindings working under CapsLock elsewhere", () => {
      expect(matchesCombo(keydown("U", { ctrl: true }), combo("u", "Ctrl"), OTHER)).toBe(true);
    });

    it("does not match a Meta or Ctrl binding without Ctrl elsewhere", () => {
      expect(matchesCombo(keydown("c"), combo("c", "Meta"), OTHER)).toBe(false);
      expect(matchesCombo(keydown("c"), combo("c", "Ctrl"), OTHER)).toBe(false);
    });

    it("does not match an unmodified binding with Ctrl held elsewhere", () => {
      expect(matchesCombo(keydown("c", { ctrl: true }), combo("c"), OTHER)).toBe(false);
    });

    it("rejects a stray Ctrl on macOS", () => {
      expect(matchesCombo(keydown("x", { ctrl: true }), combo("x"), MAC)).toBe(false);
    });

    it("rejects Ctrl+Cmd on macOS unless the binding wants both", () => {
      const event = keydown("x", { meta: true, ctrl: true });
      expect(matchesCombo(event, combo("x", "Meta"), MAC)).toBe(false);
      expect(matchesCombo(event, combo("x", "Meta", "Ctrl"), MAC)).toBe(true);
    });

    it("rejects the Windows/Super key as an extra modifier elsewhere", () => {
      expect(
        matchesCombo(keydown("x", { ctrl: true, meta: true }), combo("x", "Meta"), OTHER),
      ).toBe(false);
    });
  });

  it("requires Alt to match exactly", () => {
    expect(matchesCombo(keydown("ArrowUp", { alt: true }), combo("ArrowUp"), MAC)).toBe(false);
    expect(matchesCombo(keydown("ArrowUp"), combo("ArrowUp", "Alt"), MAC)).toBe(false);
    expect(matchesCombo(keydown("ArrowUp", { alt: true }), combo("ArrowUp", "Alt"), MAC)).toBe(
      true,
    );
  });
});
