// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { mockCommands } from "../test/tauri";
import { SettingsPanel } from "./SettingsPanel";

// The panel's own job here is routing; each section is replaced by a
// marker so the test sees which one is active.
vi.mock("../components/TitleBar", () => ({ TitleBar: () => null }));
vi.mock("../gadgets/registry", () => ({
  getGadgetsWithSettings: () => [],
  getGadgetSettingsComponent: () => undefined,
}));
vi.mock("./sections/GeneralSection", () => ({ GeneralSection: () => <div>general section</div> }));
vi.mock("./sections/GadgetsManagementPanel", () => ({
  GadgetsManagementPanel: () => <div>gadgets section</div>,
}));

describe("SettingsPanel", () => {
  it("opens the Gadgets section when install requests are waiting", async () => {
    mockCommands({
      take_settings_start_section: () => null,
      install_queue_snapshot: () => [
        {
          id: "r1",
          origin: "osOpenFile",
          sourcePath: "/Downloads/weather.torchsnap",
          state: { kind: "staging" },
        },
      ],
    });

    render(<SettingsPanel />);

    expect(await screen.findByText("gadgets section")).toBeInTheDocument();
  });

  it("opens the section the backend asks to start on", async () => {
    mockCommands({
      install_queue_snapshot: () => [],
      take_settings_start_section: () => "gadgets",
    });

    render(<SettingsPanel />);

    expect(await screen.findByText("gadgets section")).toBeInTheDocument();
  });

  it("starts on General without install requests", async () => {
    mockCommands({ install_queue_snapshot: () => [], take_settings_start_section: () => null });

    render(<SettingsPanel />);

    expect(await screen.findByText("general section")).toBeInTheDocument();
  });
});
