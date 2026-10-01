// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { AppIcon } from "./AppIcon";

describe("AppIcon", () => {
  it("renders at the given size", () => {
    render(<AppIcon size={48} />);
    const icon = screen.getByRole("img", { name: "Torchsnap" });
    expect(icon).toHaveAttribute("width", "48");
    expect(icon).toHaveAttribute("height", "48");
    expect(icon).toHaveAttribute("sizes", "48px");
  });

  it("offers the generated ladder in ascending widths", () => {
    render(<AppIcon size={48} />);
    const srcSet = screen.getByRole("img").getAttribute("srcset") ?? "";
    const widths = srcSet
      .split(", ")
      .map((candidate) => Number(candidate.split(" ")[1].slice(0, -1)));
    expect(widths).toEqual([...widths].sort((a, b) => a - b));
    expect(widths).toContain(48);
    expect(widths).toContain(96);
    expect(srcSet).toMatch(/app-icon-1024\.webp 1024w$/);
  });

  it("takes an alt text", () => {
    render(<AppIcon size={48} alt="Torchsnap app icon" />);
    expect(screen.getByRole("img", { name: "Torchsnap app icon" })).toBeInTheDocument();
  });

  it("is decorative with an empty alt text", () => {
    render(<AppIcon size={48} alt="" />);
    expect(screen.queryByRole("img")).not.toBeInTheDocument();
    expect(screen.getByRole("presentation")).toHaveAttribute("alt", "");
  });

  it("passes classes through", () => {
    render(<AppIcon size={48} className="shrink-0" />);
    expect(screen.getByRole("img")).toHaveClass("shrink-0");
  });
});
