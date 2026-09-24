// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { LauncherPreview } from "./LauncherPreview";

describe("LauncherPreview", () => {
  it("is decoration, hidden from assistive technology", () => {
    const { container } = render(<LauncherPreview />);
    expect(container.firstElementChild).toHaveAttribute("aria-hidden", "true");
  });

  it("shows the query and three results with Ghostty selected", () => {
    const { container } = render(<LauncherPreview />);
    expect(container).toHaveTextContent("gho");
    expect(container.querySelector(".bg-selection")).toHaveTextContent("Ghostty");
    for (const subtitle of [
      "/Applications/Ghostty.app",
      "Switch system appearance between dark and light",
      "/Applications/Google Chrome.app",
    ]) {
      expect(container).toHaveTextContent(subtitle);
    }
  });

  it("marks the matched letters", () => {
    const { container } = render(<LauncherPreview />);
    const matches = Array.from(container.querySelectorAll(".text-accent")).map(
      (m) => m.textContent,
    );
    expect(matches).toContain("Gho");
    expect(matches).toEqual(expect.arrayContaining(["g", "gh", "o", "G", "h"]));
  });
});
