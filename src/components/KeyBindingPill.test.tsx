// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { KeyBindingPill } from "./KeyBindingPill";

function keycaps(container: HTMLElement) {
  return Array.from(container.querySelectorAll("kbd")).map((k) => k.textContent);
}

// jsdom reports no Mac and no Windows, so Meta is the Ctrl key and the
// Linux labels apply.
describe("KeyBindingPill", () => {
  it("shows modifiers in the shared order, then the key", () => {
    const { container } = render(<KeyBindingPill modifiers={["Shift", "Meta"]} keyName="r" />);
    expect(keycaps(container)).toEqual(["Ctrl", "Shift", "R"]);
  });

  it("shows a key without modifiers", () => {
    const { container } = render(<KeyBindingPill keyName="Enter" />);
    expect(keycaps(container)).toEqual(["↵"]);
  });
});
