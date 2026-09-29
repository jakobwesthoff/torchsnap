// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { fireEvent, render } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { LauncherMascot } from "./LauncherMascot";

describe("LauncherMascot", () => {
  it("positions the centre mascot by top and left, unmirrored, at 192 px", () => {
    const { getByRole } = render(
      <LauncherMascot
        mode="center"
        variant="original"
        top={-152}
        left={241.5}
        onInfoClick={() => {}}
      />,
    );
    const button = getByRole("button");
    expect(button.style.top).toBe("-152px");
    expect(button.style.left).toBe("241.5px");
    expect(button.style.right).toBe("");
    expect(button.className).not.toContain("-scale-x-100");
    expect(button.className).not.toContain("translate-x");
    expect(button.querySelector("img")).toHaveAttribute("width", "192");
  });

  it("positions the sidekick by top and right, mirrored, at 96 px", () => {
    const { getByRole } = render(
      <LauncherMascot
        mode="sidekick"
        variant="original"
        top={-70}
        right={-1.3}
        onInfoClick={() => {}}
      />,
    );
    const button = getByRole("button");
    expect(button.style.top).toBe("-70px");
    expect(button.style.right).toBe("-1.3px");
    expect(button.style.left).toBe("");
    expect(button.className).toContain("-scale-x-100");
    expect(button.querySelector("img")).toHaveAttribute("width", "96");
  });

  it("opens the info overlay on double click only", () => {
    const onInfoClick = vi.fn();
    const { getByRole } = render(
      <LauncherMascot
        mode="center"
        variant="original"
        top={0}
        left={0}
        onInfoClick={onInfoClick}
      />,
    );
    fireEvent.click(getByRole("button"));
    expect(onInfoClick).not.toHaveBeenCalled();
    fireEvent.doubleClick(getByRole("button"));
    expect(onInfoClick).toHaveBeenCalledTimes(1);
  });
});
