// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { afterEach, describe, expect, it, vi } from "vitest";
import { mockCommands } from "../test/tauri";
import { openGadgetSettings } from "./openGadgetSettings";

describe("openGadgetSettings", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("opens the settings on the gadget's section, then closes the launcher", async () => {
    const order: string[] = [];
    mockCommands({
      gadget_open_settings: (params) => {
        order.push(`open ${params.gadgetId}`);
      },
    });

    await openGadgetSettings("zerotier", () => order.push("dismiss"));

    expect(order).toEqual(["open zerotier", "dismiss"]);
  });

  it("keeps the launcher open when the settings cannot be opened", async () => {
    vi.spyOn(console, "warn").mockImplementation(() => {});
    mockCommands({
      gadget_open_settings: () => {
        throw new Error("no settings window");
      },
    });
    const dismiss = vi.fn();

    await expect(openGadgetSettings("zerotier", dismiss)).rejects.toThrow("no settings window");
    expect(dismiss).not.toHaveBeenCalled();
  });
});
