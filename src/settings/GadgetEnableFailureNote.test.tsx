// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { mockCommands } from "../test/tauri";
import { GadgetEnableFailureNote } from "./GadgetEnableFailureNote";

describe("GadgetEnableFailureNote", () => {
  it("says that the gadget failed to start, how to retry and why", async () => {
    mockCommands({
      gadget_enable_failures: () => ({ awake: "gadget enable() returned error: no app" }),
    });

    render(<GadgetEnableFailureNote gadgetId="awake" />);

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Failed to start, so it does not work. Turn it off and on again to retry. (gadget enable() returned error: no app)",
    );
  });

  it("shows nothing while the gadget runs", async () => {
    let asked = false;
    mockCommands({
      gadget_enable_failures: () => {
        asked = true;
        return { calculator: "broken" };
      },
    });

    const { container } = render(<GadgetEnableFailureNote gadgetId="awake" />);

    await waitFor(() => expect(asked).toBe(true));
    expect(container).toBeEmptyDOMElement();
  });
});
