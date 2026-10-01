// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { mockCommands } from "../../test/tauri";
import { GeneralSection } from "./GeneralSection";

const openUrl = vi.hoisted(() => vi.fn(() => Promise.resolve()));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl }));

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
  beforeEach(() => openUrl.mockClear());

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

  it("shows the app icon above the build info", async () => {
    mockCommands({
      build_info: () => ({ version: "0.12.0", gitHash: "abc1234" }),
      shortcut_problems: () => ({}),
    });
    render(<GeneralSection />);

    const buildInfo = await screen.findByText("Build: v0.12.0 (abc1234)");
    const link = screen.getByRole("link", { name: "Open torchsnap.app" });
    expect(link.querySelector("img")).toHaveAttribute("sizes", "96px");
    expect(link.querySelector("img")).toHaveAttribute("alt", "Open torchsnap.app");
    expect(link).toHaveClass("cursor-pointer");
    expect(link.compareDocumentPosition(buildInfo) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("shows the app icon while the build info is still loading", () => {
    mockCommands({
      build_info: () => new Promise(() => {}),
      shortcut_problems: () => ({}),
    });
    render(<GeneralSection />);

    expect(screen.getByRole("link", { name: "Open torchsnap.app" })).toBeInTheDocument();
    expect(screen.queryByText(/^Build:/)).not.toBeInTheDocument();
  });

  it("opens the website in the browser when the icon is clicked", async () => {
    mockCommands({
      build_info: () => ({ version: "0.12.0", gitHash: "abc1234" }),
      shortcut_problems: () => ({}),
    });
    render(<GeneralSection />);

    await userEvent.click(screen.getByRole("link", { name: "Open torchsnap.app" }));
    expect(openUrl).toHaveBeenCalledWith("https://torchsnap.app/");
  });

  it("keeps the settings window on its page when the icon is clicked", () => {
    mockCommands({
      build_info: () => ({ version: "0.12.0", gitHash: "abc1234" }),
      shortcut_problems: () => ({}),
    });
    render(<GeneralSection />);

    const notPrevented = fireEvent.click(screen.getByRole("link", { name: "Open torchsnap.app" }));
    expect(notPrevented).toBe(false);
  });

  it("swallows a failure of the browser to open", async () => {
    openUrl.mockRejectedValueOnce(new Error("no browser"));
    mockCommands({
      build_info: () => ({ version: "0.12.0", gitHash: "abc1234" }),
      shortcut_problems: () => ({}),
    });
    render(<GeneralSection />);

    await userEvent.click(screen.getByRole("link", { name: "Open torchsnap.app" }));
    expect(openUrl).toHaveBeenCalledOnce();
  });
});
