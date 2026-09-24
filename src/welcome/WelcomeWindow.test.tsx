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

const FORWARD = ["Choose your shortcut", "Next: Startup and updates", "Next: Try it"];

async function goToPage(page: number) {
  for (const label of FORWARD.slice(0, page - 1)) {
    await press(label);
  }
}

describe("WelcomeWindow", () => {
  it("starts with the welcome and the launcher preview, without a way back", () => {
    const { container } = render(<WelcomeWindow {...props()} />);
    expect(screen.getByRole("heading", { name: "Welcome to Torchsnap" })).toBeInTheDocument();
    expect(container.querySelector('[aria-hidden="true"]')).toHaveTextContent("Ghostty");
    expect(content().queryByRole("button", { name: "Back" })).toBeNull();
    expect(screen.getByRole("group", { name: "Page 1 of 4" })).toBeInTheDocument();
  });

  it("cannot be closed from its title bar", () => {
    render(<WelcomeWindow {...props()} />);
    expect(screen.getByRole("button", { name: "Close" })).toBeDisabled();
  });

  it("names the next page on its forward button", async () => {
    render(<WelcomeWindow {...props()} />);
    await press("Choose your shortcut");
    expect(screen.getByRole("heading", { name: "Your shortcut" })).toBeInTheDocument();
    await press("Next: Startup and updates");
    expect(screen.getByRole("heading", { name: "Startup and updates" })).toBeInTheDocument();
    await press("Next: Try it");
    expect(screen.getByRole("heading", { name: "You're set" })).toBeInTheDocument();
    expect(screen.getByRole("group", { name: "Page 4 of 4" })).toBeInTheDocument();
  });

  it("goes back with the back button", async () => {
    render(<WelcomeWindow {...props()} />);
    await goToPage(3);
    await press("Back");
    expect(screen.getByRole("heading", { name: "Your shortcut" })).toBeInTheDocument();
    await press("Back");
    expect(screen.getByRole("heading", { name: "Welcome to Torchsnap" })).toBeInTheDocument();
  });

  it("has no How it works page", async () => {
    render(<WelcomeWindow {...props()} />);
    await goToPage(4);
    expect(screen.queryByRole("heading", { name: "How it works" })).toBeNull();
  });

  it("fades each page in", async () => {
    render(<WelcomeWindow {...props()} />);
    const first = screen.getByTestId("welcome-page");
    expect(first.className).toContain("welcome-fade-in");
    await press("Choose your shortcut");
    const second = screen.getByTestId("welcome-page");
    expect(second).not.toBe(first);
    expect(second.className).toContain("welcome-fade-in");
  });

  it("shows the shortcut recorder on the shortcut page", async () => {
    render(<WelcomeWindow {...props()} />);
    await goToPage(2);
    expect(content().getByText("Global Shortcut")).toBeInTheDocument();
  });

  describe("the startup and updates page", () => {
    it("shows launch at login as it is and changes it", async () => {
      const setLaunchAtLogin = vi.fn();
      render(<WelcomeWindow {...props({ launchAtLogin: false, setLaunchAtLogin })} />);
      await goToPage(3);
      const toggle = screen.getByRole("switch", { name: "Launch at login" });
      expect(toggle).toHaveAttribute("aria-checked", "false");
      await userEvent.click(toggle);
      expect(setLaunchAtLogin).toHaveBeenCalledWith(true);
    });

    it("disables launch at login while its state is unknown", async () => {
      render(<WelcomeWindow {...props({ launchAtLogin: null })} />);
      await goToPage(3);
      expect(screen.getByRole("switch", { name: "Launch at login" })).toBeDisabled();
    });

    it("prefills automatic checks with on for someone never asked", async () => {
      render(<WelcomeWindow {...props({ storedAutomaticChecks: null })} />);
      await goToPage(3);
      expect(
        screen.getByRole("switch", { name: "Check for updates automatically" }),
      ).toHaveAttribute("aria-checked", "true");
    });

    it("prefills automatic checks with a stored answer", async () => {
      render(<WelcomeWindow {...props({ storedAutomaticChecks: false })} />);
      await goToPage(3);
      expect(
        screen.getByRole("switch", { name: "Check for updates automatically" }),
      ).toHaveAttribute("aria-checked", "false");
    });

    it("says what the update check sends where", async () => {
      render(<WelcomeWindow {...props()} />);
      await goToPage(3);
      expect(content().getByText(/asks torchsnap\.app/)).toHaveTextContent("IP address");
    });

    it("does not hand over an answer before the last page", async () => {
      const readyToFinish = vi.fn();
      render(<WelcomeWindow {...props({ readyToFinish })} />);
      await goToPage(3);
      expect(readyToFinish).not.toHaveBeenCalled();
    });
  });

  describe("the last page", () => {
    it("hands over the answer when reached", async () => {
      const readyToFinish = vi.fn();
      render(<WelcomeWindow {...props({ readyToFinish })} />);
      await goToPage(3);
      await userEvent.click(
        screen.getByRole("switch", { name: "Check for updates automatically" }),
      );
      await press("Next: Try it");
      expect(readyToFinish).toHaveBeenLastCalledWith(false);
    });

    it("asks to press the shortcut and has no forward button", async () => {
      const { container } = render(<WelcomeWindow {...props()} />);
      await goToPage(4);
      expect(content().getByText("now to open the launcher.")).toBeInTheDocument();
      expect(container.querySelectorAll("main kbd").length).toBe(3);
      for (const label of FORWARD) {
        expect(content().queryByRole("button", { name: label })).toBeNull();
      }
    });

    it("finishes with the fallback link", async () => {
      const finish = vi.fn();
      render(<WelcomeWindow {...props({ finish })} />);
      await goToPage(4);
      await press("Open the launcher");
      expect(finish).toHaveBeenCalledOnce();
    });

    it("withdraws the answer when going back", async () => {
      const notReady = vi.fn();
      render(<WelcomeWindow {...props({ notReady })} />);
      await goToPage(4);
      await press("Back");
      expect(notReady).toHaveBeenCalledOnce();
      expect(screen.getByRole("heading", { name: "Startup and updates" })).toBeInTheDocument();
    });

    it("does not withdraw anything when going back from an earlier page", async () => {
      const notReady = vi.fn();
      render(<WelcomeWindow {...props({ notReady })} />);
      await goToPage(3);
      await press("Back");
      expect(notReady).not.toHaveBeenCalled();
    });
  });
});
