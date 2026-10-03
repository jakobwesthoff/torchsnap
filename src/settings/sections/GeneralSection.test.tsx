// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { mockCommands } from "../../test/tauri";
import { GeneralSection } from "./GeneralSection";

const openUrl = vi.hoisted(() => vi.fn(() => Promise.resolve()));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl }));

const autostart = vi.hoisted(() => ({
  enable: vi.fn(() => Promise.resolve()),
  disable: vi.fn(() => Promise.resolve()),
  isEnabled: vi.fn(() => Promise.resolve(false)),
}));
vi.mock("@tauri-apps/plugin-autostart", () => autostart);

const logError = vi.hoisted(() => vi.fn());
vi.mock("../../lib/logger", () => ({
  createLogger: () => ({ error: logError, warn: vi.fn(), info: vi.fn(), debug: vi.fn() }),
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
  beforeEach(() => {
    openUrl.mockClear();
    logError.mockClear();
    autostart.enable.mockReset().mockResolvedValue(undefined);
    autostart.disable.mockReset().mockResolvedValue(undefined);
    autostart.isEnabled.mockReset().mockResolvedValue(false);
  });

  function launchAtLogin(): HTMLElement {
    return screen.getByRole("switch", { name: "Launch at login" });
  }

  function mockBuildInfo() {
    mockCommands({
      build_info: () => ({ version: "0.12.0", gitHash: "abc1234" }),
      shortcut_problems: () => ({}),
    });
  }

  it("shows the autostart state and turns it on", async () => {
    mockBuildInfo();
    render(<GeneralSection />);
    await waitFor(() => expect(launchAtLogin()).toBeEnabled());
    expect(launchAtLogin()).toHaveAttribute("aria-checked", "false");

    await userEvent.click(launchAtLogin());

    expect(autostart.enable).toHaveBeenCalledOnce();
    expect(launchAtLogin()).toHaveAttribute("aria-checked", "true");
  });

  it("turns autostart off", async () => {
    autostart.isEnabled.mockResolvedValue(true);
    mockBuildInfo();
    render(<GeneralSection />);
    await waitFor(() => expect(launchAtLogin()).toHaveAttribute("aria-checked", "true"));

    await userEvent.click(launchAtLogin());

    expect(autostart.disable).toHaveBeenCalledOnce();
    expect(launchAtLogin()).toHaveAttribute("aria-checked", "false");
  });

  it("enables the switch and logs when the autostart state cannot be read", async () => {
    autostart.isEnabled.mockRejectedValue(new Error("plugin not registered"));
    mockBuildInfo();
    render(<GeneralSection />);

    await waitFor(() => expect(launchAtLogin()).toBeEnabled());
    expect(launchAtLogin()).toHaveAttribute("aria-checked", "false");
    expect(logError).toHaveBeenCalledWith(expect.stringContaining("plugin not registered"));
  });

  it("reverts the switch and logs when turning autostart on fails", async () => {
    autostart.enable.mockRejectedValue(new Error("denied"));
    mockBuildInfo();
    render(<GeneralSection />);
    await waitFor(() => expect(launchAtLogin()).toBeEnabled());

    await userEvent.click(launchAtLogin());

    await waitFor(() => expect(launchAtLogin()).toHaveAttribute("aria-checked", "false"));
    expect(logError).toHaveBeenCalledWith(expect.stringContaining("denied"));
  });

  it("reverts the switch and logs when turning autostart off fails", async () => {
    autostart.isEnabled.mockResolvedValue(true);
    autostart.disable.mockRejectedValue(new Error("denied"));
    mockBuildInfo();
    render(<GeneralSection />);
    await waitFor(() => expect(launchAtLogin()).toHaveAttribute("aria-checked", "true"));

    await userEvent.click(launchAtLogin());

    await waitFor(() => expect(launchAtLogin()).toHaveAttribute("aria-checked", "true"));
    expect(autostart.disable).toHaveBeenCalledOnce();
    expect(logError).toHaveBeenCalledWith(expect.stringContaining("denied"));
  });

  it("shows no build line and logs when the build info fails", async () => {
    mockCommands({
      build_info: () => Promise.reject("backend gone"),
      shortcut_problems: () => ({}),
    });
    render(<GeneralSection />);

    await waitFor(() =>
      expect(logError).toHaveBeenCalledWith(expect.stringContaining("backend gone")),
    );
    expect(screen.queryByText(/^Build:/)).not.toBeInTheDocument();
  });

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
    expect(link.querySelector("svg")).toHaveAttribute("width", "96");
    expect(link.querySelector("svg")).toHaveAttribute("aria-label", "Open torchsnap.app");
    expect(link).toHaveClass("cursor-pointer", "group/emblem");
    expect(link.querySelector("image")).toHaveClass(
      "motion-safe:group-hover/emblem:translate-y-(--emblem-lift)",
    );
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
