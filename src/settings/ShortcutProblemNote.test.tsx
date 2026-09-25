// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { ShortcutProblem } from "../lib/command";
import { emitTauriEvent, mockCommands } from "../test/tauri";
import { SHORTCUT_PROBLEMS_CHANGED, ShortcutProblemNote } from "./ShortcutProblemNote";

const KEY = "gadgets.clipboard-manager.shortcut.open-clipboard";

function withProblems(problems: Record<string, ShortcutProblem>) {
  mockCommands({ shortcut_problems: () => problems });
}

describe("ShortcutProblemNote", () => {
  it("shows nothing while the shortcut is registered", async () => {
    let asked = false;
    mockCommands({
      shortcut_problems: () => {
        asked = true;
        return {};
      },
    });
    const { container } = render(<ShortcutProblemNote settingsKey={KEY} />);
    await waitFor(() => expect(asked).toBe(true));
    expect(container).toBeEmptyDOMElement();
  });

  it("ignores problems of other shortcuts", async () => {
    withProblems({ globalShortcut: { kind: "missing" } });
    const { container } = render(<ShortcutProblemNote settingsKey={KEY} />);
    await waitFor(() => expect(container).toBeEmptyDOMElement());
  });

  it.each<[string, ShortcutProblem, RegExp]>([
    ["no stored shortcut", { kind: "missing" }, /No shortcut is set/],
    [
      "an invalid shortcut",
      { kind: "invalid", combo: "Shift+NotAKey" },
      /"Shift\+NotAKey" is not a valid shortcut/,
    ],
    [
      "a shortcut another one already uses",
      { kind: "takenBy", label: "Global Shortcut" },
      /Already used by "Global Shortcut"/,
    ],
    [
      "a shortcut the system refused",
      { kind: "rejected", reason: "HotKey already registered" },
      /system did not accept this shortcut.*HotKey already registered/,
    ],
  ])("explains %s", async (_name, problem, text) => {
    withProblems({ [KEY]: problem });
    render(<ShortcutProblemNote settingsKey={KEY} />);
    expect(await screen.findByRole("alert")).toHaveTextContent(text);
  });

  it("updates when the shortcuts were registered again", async () => {
    let problems: Record<string, ShortcutProblem> = { [KEY]: { kind: "missing" } };
    mockCommands({ shortcut_problems: () => problems });
    render(<ShortcutProblemNote settingsKey={KEY} />);
    expect(await screen.findByRole("alert")).toBeInTheDocument();

    problems = {};
    await emitTauriEvent(SHORTCUT_PROBLEMS_CHANGED);

    await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
  });

  it("shows nothing when the problems cannot be read", async () => {
    mockCommands({});
    const { container } = render(<ShortcutProblemNote settingsKey={KEY} />);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(container).toBeEmptyDOMElement();
  });
});
