// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { AvailableUpdate, UpdatePhase } from "./types";
import { formatMegabytes } from "./format";
import { UpdateWindow, type UpdateActions } from "./UpdateWindow";

vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn(() => Promise.resolve()) }));

function actions(): UpdateActions {
  return {
    check: vi.fn(),
    install: vi.fn(),
    skip: vi.fn(),
    close: vi.fn(),
    openDownload: vi.fn(),
  };
}

function available(overrides: Partial<AvailableUpdate> = {}): UpdatePhase {
  return {
    phase: "available",
    installed: "0.11.1",
    version: "0.12.0",
    releases: [{ version: "0.12.0", date: "2026-10-02", notes: "- Updates" }],
    pendingGadgetChanges: 0,
    locationProblem: null,
    downloadUrl: "https://example.org/Torchsnap.dmg",
    ...overrides,
  };
}

function renderWindow(phase: UpdatePhase | null) {
  const a = actions();
  render(<UpdateWindow phase={phase} actions={a} />);
  return a;
}

/** Click a button of the window content, not of the title bar. */
async function click(name: string) {
  await userEvent.click(within(screen.getByRole("main")).getByRole("button", { name }));
}

describe("UpdateWindow", () => {
  it("shows no content before the backend answered", () => {
    renderWindow(null);
    expect(screen.queryByRole("heading", { level: 2 })).toBeNull();
  });

  it("offers a check while idle", async () => {
    const a = renderWindow({ phase: "idle" });
    await click("Check Now");
    expect(a.check).toHaveBeenCalledOnce();
    await click("Close");
    expect(a.close).toHaveBeenCalledOnce();
  });

  it("says it is checking and can be closed", async () => {
    const a = renderWindow({ phase: "checking" });
    expect(screen.getByRole("heading", { name: "Checking for updates…" })).toBeInTheDocument();
    await click("Close");
    expect(a.close).toHaveBeenCalledOnce();
  });

  it("confirms an up-to-date version", async () => {
    const a = renderWindow({ phase: "upToDate", installed: "0.12.0" });
    expect(screen.getByText("Torchsnap 0.12.0 is the newest version.")).toBeInTheDocument();
    await click("OK");
    expect(a.close).toHaveBeenCalledOnce();
  });

  it("shows a failure with a retry", async () => {
    const a = renderWindow({
      phase: "failed",
      message: "Torchsnap could not get update information.",
    });
    expect(screen.getByText("Torchsnap could not get update information.")).toBeInTheDocument();
    await click("Try Again");
    expect(a.check).toHaveBeenCalledOnce();
    await click("Close");
    expect(a.close).toHaveBeenCalledOnce();
  });

  describe("an available update", () => {
    it("names both versions and shows the notes", () => {
      renderWindow(available());
      expect(
        screen.getByText("Torchsnap 0.12.0 is available. You have 0.11.1."),
      ).toBeInTheDocument();
      expect(screen.getByRole("region", { name: "Release notes" })).toHaveTextContent("Updates");
    });

    it("installs, postpones or skips", async () => {
      const a = renderWindow(available());
      await click("Install and Restart");
      expect(a.install).toHaveBeenCalledOnce();
      await click("Later");
      expect(a.close).toHaveBeenCalledOnce();
      await click("Skip This Version");
      expect(a.skip).toHaveBeenCalledOnce();
    });

    it("says nothing about gadgets when none are pending", () => {
      renderWindow(available());
      expect(screen.queryByText(/pending gadget/)).toBeNull();
    });

    it("says the restart also applies one pending gadget change", () => {
      renderWindow(available({ pendingGadgetChanges: 1 }));
      expect(
        screen.getByText("Restarting also applies 1 pending gadget change."),
      ).toBeInTheDocument();
    });

    it("counts several pending gadget changes", () => {
      renderWindow(available({ pendingGadgetChanges: 3 }));
      expect(
        screen.getByText("Restarting also applies 3 pending gadget changes."),
      ).toBeInTheDocument();
    });

    it("offers the download instead of installing from the disk image", async () => {
      const a = renderWindow(available({ locationProblem: "diskImage", pendingGadgetChanges: 2 }));
      expect(screen.getByText(/running straight from the disk image/)).toBeInTheDocument();
      expect(screen.queryByRole("button", { name: "Install and Restart" })).toBeNull();
      expect(screen.queryByText(/pending gadget/)).toBeNull();
      await click("Download");
      expect(a.openDownload).toHaveBeenCalledWith("https://example.org/Torchsnap.dmg");
    });

    it("explains a translocated copy", () => {
      renderWindow(available({ locationProblem: "translocated" }));
      expect(screen.getByText(/from a temporary location/)).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Download" })).toBeInTheDocument();
    });
  });

  it("shows download progress in percent", () => {
    renderWindow({ phase: "downloading", version: "0.12.0", downloaded: 25, total: 100 });
    expect(
      screen.getByRole("heading", { name: "Downloading Torchsnap 0.12.0…" }),
    ).toBeInTheDocument();
    expect(screen.getByText("25 %")).toBeInTheDocument();
    expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "25");
  });

  it("shows megabytes when the size is unknown", () => {
    renderWindow({
      phase: "downloading",
      version: "0.12.0",
      downloaded: 3 * 1024 * 1024,
      total: null,
    });
    expect(screen.getByText("3.0 MB")).toBeInTheDocument();
    expect(screen.getByRole("progressbar")).not.toHaveAttribute("aria-valuenow");
  });

  it("offers no buttons while installing", () => {
    renderWindow({ phase: "installing", version: "0.12.0" });
    expect(
      screen.getByRole("heading", { name: "Installing Torchsnap 0.12.0…" }),
    ).toBeInTheDocument();
    expect(screen.queryAllByRole("button", { name: /Install|Later|Skip|OK/ })).toEqual([]);
  });
});

describe("formatMegabytes", () => {
  it("rounds to one decimal", () => {
    expect(formatMegabytes(0)).toBe("0.0 MB");
    expect(formatMegabytes(1.25 * 1024 * 1024)).toBe("1.3 MB");
  });
});
