// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mockCommands } from "../test/tauri";
import type { UpdatePhase } from "./types";
import { UpdateApp } from "./UpdateApp";

const openUrl = vi.hoisted(() => vi.fn(() => Promise.resolve()));
const closeWindow = vi.hoisted(() => vi.fn(() => Promise.resolve()));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl }));
vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: () => ({ close: closeWindow }),
}));

const AVAILABLE: UpdatePhase = {
  phase: "available",
  installed: "0.11.1",
  version: "0.12.0",
  releases: [],
  pendingGadgetChanges: 0,
  locationProblem: null,
  downloadUrl: "https://example.org/Torchsnap.dmg",
};

describe("UpdateApp", () => {
  beforeEach(() => {
    openUrl.mockClear();
    closeWindow.mockClear();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("sends Install, Skip and Check to the backend", async () => {
    const calls: string[] = [];
    mockCommands({
      update_phase: () => AVAILABLE,
      update_install: () => void calls.push("install"),
      update_skip: () => void calls.push("skip"),
    });
    render(<UpdateApp />);

    await userEvent.click(await screen.findByRole("button", { name: "Install and Restart" }));
    await userEvent.click(screen.getByRole("button", { name: "Skip This Version" }));
    expect(calls).toEqual(["install", "skip"]);
  });

  it("closes the window for Later", async () => {
    mockCommands({ update_phase: () => AVAILABLE });
    render(<UpdateApp />);
    await userEvent.click(await screen.findByRole("button", { name: "Later" }));
    expect(closeWindow).toHaveBeenCalledOnce();
  });

  it("checks again from a failure", async () => {
    const check = vi.fn();
    mockCommands({
      update_phase: () => ({ phase: "failed", message: "offline" }),
      update_check: check,
    });
    render(<UpdateApp />);
    await userEvent.click(await screen.findByRole("button", { name: "Try Again" }));
    expect(check).toHaveBeenCalledOnce();
  });

  it("opens the download in the browser", async () => {
    mockCommands({ update_phase: () => ({ ...AVAILABLE, locationProblem: "diskImage" }) });
    render(<UpdateApp />);
    await userEvent.click(await screen.findByRole("button", { name: "Download" }));
    expect(openUrl).toHaveBeenCalledWith("https://example.org/Torchsnap.dmg");
  });

  it("keeps the window usable when a command fails", async () => {
    // `update_install` deliberately fails, so `command()` logs the
    // rejection. That logging is part of the contract under test, so
    // the warning is captured and asserted on instead of left to print.
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    mockCommands({
      update_phase: () => AVAILABLE,
      update_install: () => {
        throw new Error("There is no update to install.");
      },
    });
    render(<UpdateApp />);
    await userEvent.click(await screen.findByRole("button", { name: "Install and Restart" }));
    expect(screen.getByRole("button", { name: "Later" })).toBeInTheDocument();
    expect(warn).toHaveBeenCalledWith('command("update_install") failed:', expect.anything());
  });
});
