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
      ["Cmd+Shift+letter", { code: "KeyV", meta: true, shift: true }, "CommandOrControl+Shift+V"],
      ["Ctrl+letter", { code: "KeyA", ctrl: true }, "CommandOrControl+A"],
      ["Alt+Space", { code: "Space", alt: true }, "Alt+Space"],
      ["Cmd+digit", { code: "Digit1", meta: true }, "CommandOrControl+1"],
      ["Cmd+Alt+letter", { code: "KeyK", meta: true, alt: true }, "CommandOrControl+Alt+K"],
      [
        "Cmd+Alt+Shift+letter",
        { code: "KeyK", meta: true, alt: true, shift: true },
        "CommandOrControl+Alt+Shift+K",
      ],
      ["Ctrl+Alt+F-key", { code: "F12", ctrl: true, alt: true }, "CommandOrControl+Alt+F12"],
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
    expect(onChange.mock.calls).toEqual([["CommandOrControl+Shift+K"], ["CommandOrControl+K"]]);
  });

  it("commits the whole chord when modifiers go down one after another", async () => {
    const onChange = await renderRecording();
    fireEvent.keyDown(window, { code: "MetaLeft", metaKey: true });
    fireEvent.keyDown(window, { code: "AltLeft", metaKey: true, altKey: true });
    fireEvent.keyDown(window, { code: "KeyK", metaKey: true, altKey: true });
    fireEvent.keyUp(window, { code: "KeyK", metaKey: true, altKey: true });
    expect(onChange).toHaveBeenCalledExactlyOnceWith("CommandOrControl+Alt+K");
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
    // jsdom is not a Mac, where Meta and Ctrl both display as "Ctrl".
    expect(screen.getByRole("status")).toHaveTextContent("Hold Ctrl or Alt and press a key.");
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
});
