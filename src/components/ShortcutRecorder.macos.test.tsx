// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ShortcutRecorder } from "./ShortcutRecorder";

// jsdom reports a non-Mac platform, so the macOS symbols are only
// reachable by pinning the formatter to its macOS variant.
vi.mock("../keybindings/platform", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../keybindings/platform")>();
  return {
    ...actual,
    formatModifier: (m: Parameters<typeof actual.formatModifier>[0]) =>
      actual.formatModifierForPlatform(m, "macos"),
  };
});

describe("ShortcutRecorder on macOS", () => {
  it("names Cmd, Ctrl and Alt by their symbols in the hint", async () => {
    render(<ShortcutRecorder value="CommandOrControl+Space" onChange={vi.fn()} />);
    await userEvent.click(screen.getByRole("button", { name: "Change" }));
    expect(screen.getByRole("status")).toHaveTextContent("Hold ⌘, ⌃ or ⌥ and press a key.");
  });
});
