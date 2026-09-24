// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Entry } from "./Entry";

describe("Entry", () => {
  it("shows label, description and control in one row", () => {
    render(
      <Entry label="Check for updates automatically" description="Once a day.">
        <button type="button">control</button>
      </Entry>,
    );
    const row = screen.getByText("Check for updates automatically").closest("label");
    expect(row).toHaveTextContent("Once a day.");
    expect(row).toContainElement(screen.getByText("control"));
  });

  it("keeps a gap between a long description and the control", () => {
    render(
      <Entry label="Label" description="A description long enough to reach the control.">
        <button type="button">control</button>
      </Entry>,
    );
    const row = screen.getByText("Label").closest("label");
    expect(row?.className).toMatch(/\bgap-6\b/);
  });
});
