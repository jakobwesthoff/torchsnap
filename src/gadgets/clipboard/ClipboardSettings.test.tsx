// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { mockCommands } from "../../test/tauri";
import ClipboardSettings from "./ClipboardSettings";

interface Stats {
  totalEntries: number;
  entriesByFormat: Record<string, number>;
  totalSizeBytes: number;
}

const gadget = vi.hoisted(() => ({
  enabled: true,
  stats: { totalEntries: 3, entriesByFormat: { text: 3 }, totalSizeBytes: 2048 } as Stats,
  sendMessage: vi.fn(),
  logger: { error: vi.fn() },
}));

vi.mock("../../contexts/useGadgetInfo", () => ({
  useGadgetInfo: () => ({ id: "clipboard-manager", enabled: gadget.enabled }),
}));
vi.mock("../../contexts/useGadgetRuntime", () => ({
  useGadgetRuntime: () => ({ sendMessage: gadget.sendMessage, logger: gadget.logger }),
}));
vi.mock("../../contexts/useGadgetSetting", () => ({
  useGadgetSetting: (key: string) =>
    ({
      retentionDays: [30, vi.fn()],
      bringToFrontOnPaste: [true, vi.fn()],
      "shortcut.open-clipboard": ["CommandOrControl+Shift+V", vi.fn()],
    })[key],
}));

function clearAll() {
  return screen.getByRole("button", { name: "Clear All" });
}

describe("ClipboardSettings", () => {
  beforeEach(() => {
    gadget.enabled = true;
    gadget.stats = { totalEntries: 3, entriesByFormat: { text: 3 }, totalSizeBytes: 2048 };
    gadget.sendMessage.mockReset();
    gadget.sendMessage.mockImplementation((method: string) =>
      method === "stats" ? Promise.resolve(gadget.stats) : Promise.resolve(null),
    );
    // The settings row renders `ShortcutProblemNote`, which asks the
    // backend for shortcut problems on mount. Tests that care about
    // that note install their own `mockCommands` call, which replaces
    // this default.
    mockCommands({ shortcut_problems: () => ({}) });
  });

  it("shows statistics and offers Clear All while enabled with entries", async () => {
    render(<ClipboardSettings />);

    expect(await screen.findByText("Total entries")).toBeInTheDocument();
    expect(clearAll()).toBeEnabled();
  });

  it("offers no Clear All while enabled with an empty history", async () => {
    gadget.stats = { totalEntries: 0, entriesByFormat: {}, totalSizeBytes: 0 };
    render(<ClipboardSettings />);

    expect(await screen.findByText("Total entries")).toBeInTheDocument();
    expect(clearAll()).toBeDisabled();
  });

  it("asks the disabled gadget for nothing and offers no Clear All", () => {
    gadget.enabled = false;
    render(<ClipboardSettings />);

    expect(gadget.sendMessage).not.toHaveBeenCalled();
    expect(screen.getByText(/Turn on the gadget/)).toBeInTheDocument();
    expect(clearAll()).toBeDisabled();
    expect(screen.getByRole("button", { name: "Refresh" })).toBeDisabled();
  });

  it("loads statistics once the gadget is turned on", async () => {
    gadget.enabled = false;
    const { rerender } = render(<ClipboardSettings />);
    expect(gadget.sendMessage).not.toHaveBeenCalled();

    gadget.enabled = true;
    rerender(<ClipboardSettings />);

    await waitFor(() => expect(gadget.sendMessage).toHaveBeenCalledWith("stats", {}));
    expect(await screen.findByText("Total entries")).toBeInTheDocument();
  });

  it("explains why the clipboard shortcut is not registered", async () => {
    mockCommands({
      shortcut_problems: () => ({
        "gadgets.clipboard-manager.shortcut.open-clipboard": {
          kind: "takenBy",
          label: "Global Shortcut",
        },
      }),
    });

    render(<ClipboardSettings />);

    expect(await screen.findByRole("alert")).toHaveTextContent(/Already used by "Global Shortcut"/);
  });
});
