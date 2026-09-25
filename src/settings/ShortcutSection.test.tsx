// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { mockCommands } from "../test/tauri";
import { ShortcutSection } from "./ShortcutSection";

describe("ShortcutSection", () => {
  it("explains why the launcher shortcut is not registered", async () => {
    mockCommands({
      shortcut_problems: () => ({
        globalShortcut: { kind: "rejected", reason: "HotKey already registered" },
      }),
    });

    render(<ShortcutSection globalShortcut="Control+Space" setGlobalShortcut={vi.fn()} />);

    expect(await screen.findByRole("alert")).toHaveTextContent(/did not accept this shortcut/);
  });
});
