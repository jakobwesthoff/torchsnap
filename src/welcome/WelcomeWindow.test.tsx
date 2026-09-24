// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { WelcomeWindow, type WelcomeProps } from "./WelcomeWindow";

vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: () => ({ close: vi.fn(), minimize: vi.fn() }),
}));

function props(overrides: Partial<WelcomeProps> = {}): WelcomeProps {
  return {
    globalShortcut: "CmdOrCtrl+Shift+Space",
    setGlobalShortcut: vi.fn(() => Promise.resolve()),
    launchAtLogin: false,
    setLaunchAtLogin: vi.fn(),
    storedAutomaticChecks: null,
    readyToFinish: vi.fn(),
    notReady: vi.fn(),
    finish: vi.fn(),
    ...overrides,
  };
}

function content() {
  return within(screen.getByRole("main"));
}

async function press(name: string) {
  await userEvent.click(content().getByRole("button", { name }));
}

async function goToStep(step: number) {
  for (let i = 1; i < step; i++) {
    await press("Continue");
  }
}

describe("WelcomeWindow", () => {
  it("starts with the welcome and offers no Back", () => {
    render(<WelcomeWindow {...props()} />);
    expect(screen.getByRole("heading", { name: "Welcome to Torchsnap" })).toBeInTheDocument();
    expect(content().queryByRole("button", { name: "Back" })).toBeNull();
    expect(screen.getByRole("group", { name: "Step 1 of 5" })).toBeInTheDocument();
  });

  it("cannot be closed from its title bar", () => {
    render(<WelcomeWindow {...props()} />);
    expect(screen.getByRole("button", { name: "Close" })).toBeDisabled();
  });

  it("walks forward and back through the steps", async () => {
    render(<WelcomeWindow {...props()} />);
    await goToStep(3);
    expect(screen.getByRole("heading", { name: "Your shortcut" })).toBeInTheDocument();
    await press("Back");
    expect(screen.getByRole("heading", { name: "How it works" })).toBeInTheDocument();
  });

  it("explains the launcher with the user's shortcut", async () => {
    const { container } = render(<WelcomeWindow {...props({ globalShortcut: "Alt+Space" })} />);
    await goToStep(2);
    expect(content().getByText(/in any app to open the launcher/)).toBeInTheDocument();
    expect(container.querySelectorAll("kbd").length).toBe(2);
  });

  it("shows the shortcut recorder on step 3", async () => {
    render(<WelcomeWindow {...props()} />);
    await goToStep(3);
    expect(content().getByText("Global Shortcut")).toBeInTheDocument();
  });

  describe("step 4", () => {
    it("shows launch at login as it is and changes it", async () => {
      const setLaunchAtLogin = vi.fn();
      render(<WelcomeWindow {...props({ launchAtLogin: false, setLaunchAtLogin })} />);
      await goToStep(4);
      const toggle = screen.getByRole("switch", { name: "Launch at login" });
      expect(toggle).toHaveAttribute("aria-checked", "false");
      await userEvent.click(toggle);
      expect(setLaunchAtLogin).toHaveBeenCalledWith(true);
    });

    it("disables launch at login while its state is unknown", async () => {
      render(<WelcomeWindow {...props({ launchAtLogin: null })} />);
      await goToStep(4);
      expect(screen.getByRole("switch", { name: "Launch at login" })).toBeDisabled();
    });

    it("prefills automatic checks with on for someone never asked", async () => {
      render(<WelcomeWindow {...props({ storedAutomaticChecks: null })} />);
      await goToStep(4);
      expect(
        screen.getByRole("switch", { name: "Check for updates automatically" }),
      ).toHaveAttribute("aria-checked", "true");
    });

    it("prefills automatic checks with a stored answer", async () => {
      render(<WelcomeWindow {...props({ storedAutomaticChecks: false })} />);
      await goToStep(4);
      expect(
        screen.getByRole("switch", { name: "Check for updates automatically" }),
      ).toHaveAttribute("aria-checked", "false");
    });

    it("says what the update check sends where", async () => {
      render(<WelcomeWindow {...props()} />);
      await goToStep(4);
      expect(content().getByText(/asks torchsnap\.app/)).toHaveTextContent("IP address");
    });

    it("does not hand over an answer before the last step", async () => {
      const readyToFinish = vi.fn();
      render(<WelcomeWindow {...props({ readyToFinish })} />);
      await goToStep(4);
      expect(readyToFinish).not.toHaveBeenCalled();
    });
  });

  describe("the last step", () => {
    it("hands over the answer when reached", async () => {
      const readyToFinish = vi.fn();
      render(<WelcomeWindow {...props({ readyToFinish })} />);
      await goToStep(4);
      await userEvent.click(
        screen.getByRole("switch", { name: "Check for updates automatically" }),
      );
      await press("Continue");
      expect(readyToFinish).toHaveBeenLastCalledWith(false);
    });

    it("asks to press the shortcut and has no Continue", async () => {
      render(<WelcomeWindow {...props()} />);
      await goToStep(5);
      expect(screen.getByRole("heading", { name: "You're set" })).toBeInTheDocument();
      expect(content().getByText("now to open the launcher.")).toBeInTheDocument();
      expect(content().queryByRole("button", { name: "Continue" })).toBeNull();
    });

    it("finishes with the fallback button", async () => {
      const finish = vi.fn();
      render(<WelcomeWindow {...props({ finish })} />);
      await goToStep(5);
      await press("Open the launcher");
      expect(finish).toHaveBeenCalledOnce();
    });

    it("withdraws the answer when going back", async () => {
      const notReady = vi.fn();
      render(<WelcomeWindow {...props({ notReady })} />);
      await goToStep(5);
      await press("Back");
      expect(notReady).toHaveBeenCalledOnce();
      expect(screen.getByRole("heading", { name: "Two choices" })).toBeInTheDocument();
    });

    it("does not withdraw anything when going back from an earlier step", async () => {
      const notReady = vi.fn();
      render(<WelcomeWindow {...props({ notReady })} />);
      await goToStep(3);
      await press("Back");
      expect(notReady).not.toHaveBeenCalled();
    });
  });
});
