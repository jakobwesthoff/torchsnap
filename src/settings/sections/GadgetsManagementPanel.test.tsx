// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { render, screen, within } from "@testing-library/react";
import { mockWindows } from "@tauri-apps/api/mocks";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { mockCommands } from "../../test/tauri";
import { GadgetsManagementPanel } from "./GadgetsManagementPanel";

vi.mock("../../gadgets/registry", () => ({
  getGadgetsWithSettings: () => [
    { id: "weather", label: "Weather", description: "Forecasts" },
    { id: "clipboard-manager", label: "Clipboard", description: "History" },
  ],
}));

vi.mock("../../hooks/useSetting", () => ({
  useSetting: () => [true, () => {}],
}));

describe("GadgetsManagementPanel", () => {
  beforeEach(() => {
    mockWindows("settings");
  });

  it("shows each gadget's permissions on its card", async () => {
    mockCommands({
      gadget_sources: () => ({ weather: "user", "clipboard-manager": "builtin" }),
      gadget_permissions: () => ({
        weather: [
          {
            permission: { kind: "httpOrigin", origin: "https://api.weather.example" },
            severity: "notice",
            change: "unchanged",
          },
        ],
      }),
      install_queue_snapshot: () => [],
    });

    render(<GadgetsManagementPanel />);

    const card = (await screen.findByText("Weather")).closest("[data-gadget]");
    expect(card).not.toBeNull();
    expect(
      within(card as HTMLElement).getByText("Connect to https://api.weather.example"),
    ).toBeInTheDocument();

    const builtin = screen.getByText("Clipboard").closest("[data-gadget]") as HTMLElement;
    expect(within(builtin).queryByRole("list", { name: "Permissions" })).not.toBeInTheDocument();
  });
});
