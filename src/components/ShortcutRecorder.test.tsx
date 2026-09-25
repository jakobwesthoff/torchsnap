// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ShortcutRecorder } from "./ShortcutRecorder";

interface Chord {
  code: string;
  meta?: boolean;
  ctrl?: boolean;
  alt?: boolean;
  shift?: boolean;
}

/** Press and release one chord while the recorder listens on `window`. */
function press({ code, meta = false, ctrl = false, alt = false, shift = false }: Chord) {
  const init = { code, metaKey: meta, ctrlKey: ctrl, altKey: alt, shiftKey: shift };
  fireEvent.keyDown(window, init);
  fireEvent.keyUp(window, init);
}

async function renderRecording(value = "CommandOrControl+Space") {
  const onChange = vi.fn();
  render(<ShortcutRecorder value={value} onChange={onChange} />);
  await userEvent.click(screen.getByRole("button", { name: "Change" }));
  return onChange;
}

function isRecording() {
  return screen.queryByRole("button", { name: "Cancel shortcut recording" }) !== null;
}

describe("ShortcutRecorder", () => {
  describe("accepted combos", () => {
    it.each<[string, Chord, string]>([
      ["Cmd+Shift+letter", { code: "KeyV", meta: true, shift: true }, "Shift+Super+V"],
      ["Ctrl+letter", { code: "KeyA", ctrl: true }, "Control+A"],
      ["Alt+Space", { code: "Space", alt: true }, "Alt+Space"],
      ["Cmd+digit", { code: "Digit1", meta: true }, "Super+1"],
      ["Cmd+Alt+letter", { code: "KeyK", meta: true, alt: true }, "Alt+Super+K"],
      [
        "Cmd+Alt+Shift+letter",
        { code: "KeyK", meta: true, alt: true, shift: true },
        "Alt+Shift+Super+K",
      ],
      ["Ctrl+Alt+F-key", { code: "F12", ctrl: true, alt: true }, "Control+Alt+F12"],
      ["Cmd+Ctrl+letter", { code: "KeyK", meta: true, ctrl: true }, "Control+Super+K"],
      // The same flag is the Windows or Super key outside macOS.
      ["Win+letter", { code: "KeyK", meta: true }, "Super+K"],
      ["a bare F-key", { code: "F5" }, "F5"],
      ["the highest F-key", { code: "F24" }, "F24"],
      ["Shift+F-key", { code: "F5", shift: true }, "Shift+F5"],
    ])("commits %s", async (_name, chord, accelerator) => {
      const onChange = await renderRecording();
      press(chord);
      expect(onChange).toHaveBeenCalledExactlyOnceWith(accelerator);
      expect(isRecording()).toBe(false);
    });
  });

  it("records Cmd+Shift+K and Cmd+K as different shortcuts", async () => {
    const onChange = await renderRecording();
    press({ code: "KeyK", meta: true, shift: true });
    await userEvent.click(screen.getByRole("button", { name: "Change" }));
    press({ code: "KeyK", meta: true });
    expect(onChange.mock.calls).toEqual([["Shift+Super+K"], ["Super+K"]]);
  });

  it("records Ctrl+K and Cmd+K as different shortcuts", async () => {
    const onChange = await renderRecording();
    press({ code: "KeyK", ctrl: true });
    await userEvent.click(screen.getByRole("button", { name: "Change" }));
    press({ code: "KeyK", meta: true });
    expect(onChange.mock.calls).toEqual([["Control+K"], ["Super+K"]]);
  });

  it("commits the whole chord when modifiers go down one after another", async () => {
    const onChange = await renderRecording();
    fireEvent.keyDown(window, { code: "MetaLeft", metaKey: true });
    fireEvent.keyDown(window, { code: "AltLeft", metaKey: true, altKey: true });
    fireEvent.keyDown(window, { code: "KeyK", metaKey: true, altKey: true });
    fireEvent.keyUp(window, { code: "KeyK", metaKey: true, altKey: true });
    expect(onChange).toHaveBeenCalledExactlyOnceWith("Alt+Super+K");
  });

  describe("rejected combos", () => {
    it.each<[string, Chord]>([
      ["a bare letter", { code: "KeyA" }],
      ["a bare Enter", { code: "Enter" }],
      ["a bare Space", { code: "Space" }],
      ["Shift+letter", { code: "KeyA", shift: true }],
      ["a key named like an F-key but out of range", { code: "F25" }],
    ])("does not commit %s and keeps recording", async (_name, chord) => {
      const onChange = await renderRecording();
      press(chord);
      expect(onChange).not.toHaveBeenCalled();
      expect(isRecording()).toBe(true);
    });

    it("explains the rule after a rejected combo", async () => {
      await renderRecording();
      expect(screen.getByRole("status")).not.toHaveTextContent(/every other app/);
      press({ code: "KeyA" });
      expect(screen.getByRole("status")).toHaveTextContent(/every other app/);
    });

    it("still commits a valid combo after a rejected one", async () => {
      const onChange = await renderRecording();
      press({ code: "KeyA" });
      press({ code: "KeyA", alt: true });
      expect(onChange).toHaveBeenCalledExactlyOnceWith("Alt+A");
    });
  });

  it("keeps recording when only modifiers are pressed and released", async () => {
    const onChange = await renderRecording();
    press({ code: "MetaLeft", meta: true });
    press({ code: "ShiftLeft", shift: true });
    expect(onChange).not.toHaveBeenCalled();
    expect(isRecording()).toBe(true);
  });

  it("shows which modifiers are needed while recording, and nothing when idle", async () => {
    const onChange = vi.fn();
    render(<ShortcutRecorder value="CommandOrControl+Space" onChange={onChange} />);
    expect(screen.queryByRole("status")).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Change" }));
    // jsdom reports neither macOS nor Windows, so the Linux labels apply.
    expect(screen.getByRole("status")).toHaveTextContent(
      "Hold Ctrl, Alt or Super and press a key.",
    );
    expect(screen.getByRole("status")).toHaveTextContent(/F-keys/);
  });

  it("cancels on Escape without committing", async () => {
    const onChange = await renderRecording();
    press({ code: "Escape" });
    expect(onChange).not.toHaveBeenCalled();
    expect(isRecording()).toBe(false);
  });

  it("cancels from the cancel button without committing", async () => {
    const onChange = await renderRecording();
    fireEvent.mouseDown(screen.getByRole("button", { name: "Cancel shortcut recording" }));
    expect(onChange).not.toHaveBeenCalled();
    expect(isRecording()).toBe(false);
  });

  it("does not start recording while disabled", async () => {
    render(<ShortcutRecorder value="CommandOrControl+Space" onChange={vi.fn()} disabled />);
    const change = screen.getByRole("button", { name: "Change" });
    expect(change).toBeDisabled();
    await userEvent.click(change);
    expect(isRecording()).toBe(false);
  });

  // Every modifier subset against a broad set of keys, as `e.code`
  // reports them. A combo is accepted with Ctrl, Alt or Cmd/Win held,
  // or when the key is an F-key; the accelerator lists the held
  // modifiers in display order, then the key.
  describe("every modifier combination", () => {
    const CODES = [
      "KeyA",
      "KeyK",
      "KeyZ",
      "Digit0",
      "Digit9",
      "Space",
      "Enter",
      "Tab",
      "Backspace",
      "Delete",
      "ArrowUp",
      "ArrowLeft",
      "Home",
      "PageDown",
      "Minus",
      "Equal",
      "BracketLeft",
      "Backslash",
      "Semicolon",
      "Quote",
      "Backquote",
      "Comma",
      "Period",
      "Slash",
      "F1",
      "F12",
      "F13",
      "F24",
    ];
    const MODIFIERS = [
      { flag: "ctrl", token: "Control" },
      { flag: "alt", token: "Alt" },
      { flag: "shift", token: "Shift" },
      { flag: "meta", token: "Super" },
    ] as const;

    const cases = Array.from({ length: 16 }, (_, mask) =>
      MODIFIERS.filter((_, bit) => (mask & (1 << bit)) !== 0),
    ).flatMap((held) =>
      CODES.map((code) => {
        const chord: Chord = { code };
        for (const m of held) {
          chord[m.flag] = true;
        }
        const key = code.replace(/^Key/, "").replace(/^Digit/, "");
        const accepted =
          held.some((m) => m.flag !== "shift") || /^F([1-9]|1[0-9]|2[0-4])$/.test(code);
        return { chord, accepted, accelerator: [...held.map((m) => m.token), key].join("+") };
      }),
    );

    it("commits exactly the accepted combos, each as its own accelerator", async () => {
      const onChange = vi.fn();
      render(<ShortcutRecorder value="Control+Space" onChange={onChange} />);

      for (const { chord, accepted, accelerator } of cases) {
        if (!isRecording()) {
          fireEvent.click(screen.getByRole("button", { name: "Change" }));
        }
        onChange.mockClear();
        press(chord);
        if (accepted) {
          expect(onChange, JSON.stringify(chord)).toHaveBeenCalledExactlyOnceWith(accelerator);
          expect(isRecording()).toBe(false);
        } else {
          expect(onChange, JSON.stringify(chord)).not.toHaveBeenCalled();
          expect(isRecording()).toBe(true);
        }
      }
    });

    it("never gives two different combos the same accelerator", () => {
      const accelerators = cases.filter((c) => c.accepted).map((c) => c.accelerator);
      expect(new Set(accelerators).size).toBe(accelerators.length);
    });
  });
});
