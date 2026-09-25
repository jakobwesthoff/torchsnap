// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { render, screen } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { mockCommands } from "../../test/tauri";
import { GeneralSection } from "./GeneralSection";

vi.mock("@tauri-apps/plugin-autostart", () => ({
  enable: vi.fn(),
  disable: vi.fn(),
  isEnabled: vi.fn(() => Promise.resolve(false)),
}));

const SETTINGS: Record<string, unknown> = {
  globalShortcut: "CmdOrCtrl+Shift+Space",
  "controlChannel.enabled": false,
};
vi.mock("../../hooks/useSetting", () => ({
  useSetting: (key: string) => {
    const [value, setValue] = useState(() => SETTINGS[key]);
    return [value, async (next: unknown) => setValue(next)];
  },
}));

describe("GeneralSection", () => {
  it("has the updates section between startup and advanced", async () => {
    mockCommands({
      build_info: () => ({ version: "0.12.0", gitHash: "abc1234" }),
      shortcut_problems: () => ({}),
    });
    render(<GeneralSection />);

    const titles = screen.getAllByRole("heading", { level: 3 }).map((h) => h.textContent);
    expect(titles).toEqual(["Startup", "Updates", "Advanced"]);
    expect(await screen.findByText("Build: v0.12.0 (abc1234)")).toBeInTheDocument();
  });
});
