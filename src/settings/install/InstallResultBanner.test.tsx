// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { InstalledGadgetInfo } from "../../lib/command";
import { InstallResultBanner } from "./InstallResultBanner";
import type { InstallResult } from "./useInstallQueue";

vi.mock("@tauri-apps/plugin-process", () => ({ relaunch: vi.fn() }));

function result(
  info: Partial<InstalledGadgetInfo>,
  undone = false,
  requiresRestart = !undone,
): InstallResult {
  return {
    kind: "installed",
    undone,
    requiresRestart,
    info: {
      id: "weather",
      name: "Weather",
      version: "1.0.0",
      previousVersion: null,
      versionRelation: null,
      requiresRestart: true,
      ...info,
    },
  };
}

describe("InstallResultBanner", () => {
  it("lists every outcome of the batch", () => {
    render(
      <InstallResultBanner
        results={[
          result({}),
          result({ id: "calendar", name: "Calendar", version: "2.0.0", previousVersion: "1.5.0" }),
        ]}
        onUndo={() => {}}
        onDismiss={() => {}}
      />,
    );

    const items = screen.getAllByRole("listitem");
    expect(items[0]).toHaveTextContent("Installed Weather 1.0.0");
    expect(items[1]).toHaveTextContent("Replaced Calendar 1.5.0 with 2.0.0");
  });

  it("offers one restart for the whole batch", () => {
    render(
      <InstallResultBanner
        results={[result({}), result({ id: "calendar", name: "Calendar" })]}
        onUndo={() => {}}
        onDismiss={() => {}}
      />,
    );

    expect(screen.getAllByRole("button", { name: "Restart now" })).toHaveLength(1);
  });

  it("undoes a single install", async () => {
    const onUndo = vi.fn();
    render(<InstallResultBanner results={[result({})]} onUndo={onUndo} onDismiss={() => {}} />);

    await userEvent.click(screen.getByRole("button", { name: "Undo" }));

    expect(onUndo).toHaveBeenCalledWith("weather");
  });

  it("shows an undone install without another undo", () => {
    render(
      <InstallResultBanner results={[result({}, true)]} onUndo={() => {}} onDismiss={() => {}} />,
    );

    const item = screen.getByRole("listitem");
    expect(item).toHaveTextContent("Undid installing Weather 1.0.0");
    expect(within(item).queryByRole("button", { name: "Undo" })).not.toBeInTheDocument();
  });

  it("says that dismissing ends the chance to undo", async () => {
    const onDismiss = vi.fn();
    render(<InstallResultBanner results={[result({})]} onUndo={() => {}} onDismiss={onDismiss} />);

    const dismiss = screen.getByRole("button", { name: "Dismiss" });
    expect(dismiss).toHaveAttribute("title", expect.stringContaining("can no longer be undone"));
    await userEvent.click(dismiss);
    expect(onDismiss).toHaveBeenCalled();
  });

  it("drops the restart prompt once every change is undone", () => {
    render(
      <InstallResultBanner results={[result({}, true)]} onUndo={() => {}} onDismiss={() => {}} />,
    );

    expect(screen.queryByRole("button", { name: "Restart now" })).not.toBeInTheDocument();
    expect(screen.getByText("No restart needed.")).toBeInTheDocument();
  });

  it("keeps the restart prompt while an undone change still needs one", () => {
    render(
      <InstallResultBanner
        results={[result({}, true, true)]}
        onUndo={() => {}}
        onDismiss={() => {}}
      />,
    );

    expect(screen.getByRole("button", { name: "Restart now" })).toBeInTheDocument();
  });
});
