// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { mockCommands } from "../test/tauri";
import { GadgetSettingsWrapper } from "./GadgetSettingsWrapper";

const setting = vi.hoisted(() => ({ enabled: true as boolean | undefined }));
vi.mock("../hooks/useSetting", () => ({
  useSetting: () => [setting.enabled, () => Promise.resolve()],
}));

function renderWrapper() {
  return render(
    <GadgetSettingsWrapper
      gadgetId="awake"
      icon="heroicons:bolt"
      name="Awake"
      description="Keep the system awake"
    >
      <p>Gadget settings</p>
    </GadgetSettingsWrapper>,
  );
}

describe("GadgetSettingsWrapper", () => {
  let asked: boolean;

  beforeEach(() => {
    asked = false;
    mockCommands({
      gadget_enable_failures: () => {
        asked = true;
        return { awake: "no app" };
      },
    });
  });

  it("shows a failed start under the switch while the gadget is turned on", async () => {
    setting.enabled = true;
    renderWrapper();

    expect(await screen.findByRole("alert")).toHaveTextContent(/Failed to start.*\(no app\)/);
    expect(screen.getByText("Gadget settings")).toBeInTheDocument();
  });

  it("shows a failed start before the stored setting is known", async () => {
    setting.enabled = undefined;
    renderWrapper();

    expect(await screen.findByRole("alert")).toHaveTextContent(/Failed to start/);
  });

  it("hides the failure once the gadget is turned off", async () => {
    setting.enabled = true;
    const { rerender } = renderWrapper();
    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(asked).toBe(true);

    setting.enabled = false;
    rerender(
      <GadgetSettingsWrapper
        gadgetId="awake"
        icon="heroicons:bolt"
        name="Awake"
        description="Keep the system awake"
      />,
    );

    expect(screen.queryByRole("alert")).toBeNull();
  });
});
