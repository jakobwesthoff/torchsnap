// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mockCommands } from "../test/tauri";
import { WelcomeApp } from "./WelcomeApp";

const autostart = vi.hoisted(() => ({
  enabled: false,
  isEnabled: vi.fn(),
  enable: vi.fn(() => Promise.resolve()),
  disable: vi.fn(() => Promise.resolve()),
}));
vi.mock("@tauri-apps/plugin-autostart", () => ({
  isEnabled: () => autostart.isEnabled(),
  enable: () => autostart.enable(),
  disable: () => autostart.disable(),
}));
vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: () => ({ close: vi.fn(), minimize: vi.fn() }),
}));

const settings = vi.hoisted(() => new Map<string, unknown>());
vi.mock("../hooks/useSetting", () => ({
  useSetting: (key: string) => {
    const [value, setValue] = useState(() => settings.get(key));
    return [
      value,
      async (next: unknown) => {
        settings.set(key, next);
        setValue(next);
      },
    ];
  },
}));

// The shortcut page renders `ShortcutSection`, which asks the backend
// for shortcut problems on mount. None of these tests care about that
// note, so every `mockCommands` call below answers it with none.
const NO_SHORTCUT_PROBLEMS = { shortcut_problems: () => ({}) };

const FORWARD = ["Choose your shortcut", "Next: Startup and updates", "Next: Try it"];

async function goToPage(page: number) {
  const main = within(screen.getByRole("main"));
  for (const label of FORWARD.slice(0, page - 1)) {
    await userEvent.click(main.getByRole("button", { name: label }));
  }
}

describe("WelcomeApp", () => {
  beforeEach(() => {
    settings.clear();
    settings.set("globalShortcut", "CmdOrCtrl+Shift+Space");
    autostart.isEnabled.mockReset().mockResolvedValue(false);
    autostart.enable.mockClear();
    autostart.disable.mockClear();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("reads launch at login and turns it on", async () => {
    mockCommands(NO_SHORTCUT_PROBLEMS);
    render(<WelcomeApp />);
    await goToPage(3);
    const toggle = screen.getByRole("switch", { name: "Launch at login" });
    await waitFor(() => expect(toggle).toBeEnabled());
    await userEvent.click(toggle);
    expect(autostart.enable).toHaveBeenCalledOnce();
    expect(toggle).toHaveAttribute("aria-checked", "true");
  });

  it("turns launch at login off", async () => {
    autostart.isEnabled.mockResolvedValue(true);
    mockCommands(NO_SHORTCUT_PROBLEMS);
    render(<WelcomeApp />);
    await goToPage(3);
    const toggle = screen.getByRole("switch", { name: "Launch at login" });
    await waitFor(() => expect(toggle).toHaveAttribute("aria-checked", "true"));
    await userEvent.click(toggle);
    expect(autostart.disable).toHaveBeenCalledOnce();
  });

  it("treats an unreadable autostart state as off", async () => {
    autostart.isEnabled.mockRejectedValue(new Error("no launch agent"));
    mockCommands(NO_SHORTCUT_PROBLEMS);
    render(<WelcomeApp />);
    await goToPage(3);
    const toggle = screen.getByRole("switch", { name: "Launch at login" });
    await waitFor(() => expect(toggle).toBeEnabled());
    expect(toggle).toHaveAttribute("aria-checked", "false");
  });

  it("prefills the stored update answer", async () => {
    settings.set("updates.automaticChecks", false);
    mockCommands(NO_SHORTCUT_PROBLEMS);
    render(<WelcomeApp />);
    await goToPage(3);
    expect(screen.getByRole("switch", { name: "Check for updates automatically" })).toHaveAttribute(
      "aria-checked",
      "false",
    );
  });

  it("tells the backend about the last step and finishes there", async () => {
    const calls: unknown[] = [];
    mockCommands({
      ...NO_SHORTCUT_PROBLEMS,
      welcome_ready_to_finish: (params) => void calls.push(["ready", params]),
      welcome_not_ready: () => void calls.push(["notReady"]),
      welcome_finish: () => void calls.push(["finish"]),
    });
    render(<WelcomeApp />);
    await goToPage(4);
    const main = within(screen.getByRole("main"));
    await userEvent.click(main.getByRole("button", { name: "Back" }));
    await userEvent.click(main.getByRole("button", { name: "Next: Try it" }));
    await userEvent.click(main.getByRole("button", { name: "Open the launcher" }));

    await waitFor(() => expect(calls).toHaveLength(4));
    expect(calls).toEqual([
      ["ready", { automaticChecks: true }],
      ["notReady"],
      ["ready", { automaticChecks: true }],
      ["finish"],
    ]);
  });

  it("stays usable when the backend refuses to finish", async () => {
    // `welcome_finish` deliberately fails, so `command()` logs the
    // rejection. That logging is part of the contract under test, so
    // the warning is captured and asserted on instead of left to print.
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    mockCommands({
      ...NO_SHORTCUT_PROBLEMS,
      welcome_ready_to_finish: () => {},
      welcome_finish: () => {
        throw new Error("The welcome is not on its last step.");
      },
    });
    render(<WelcomeApp />);
    await goToPage(4);
    const main = within(screen.getByRole("main"));
    await userEvent.click(main.getByRole("button", { name: "Open the launcher" }));
    expect(main.getByRole("button", { name: "Open the launcher" })).toBeInTheDocument();
    expect(warn).toHaveBeenCalledWith('command("welcome_finish") failed:', expect.anything());
  });
});
