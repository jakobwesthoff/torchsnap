// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { render, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { PermissionSummary } from "./PermissionSummary";
import type { PermissionItem } from "./types";

const clipboard: PermissionItem = {
  permission: { kind: "clipboard" },
  severity: "notice",
  change: "unchanged",
};
const settings: PermissionItem = {
  permission: { kind: "settings" },
  severity: "info",
  change: "unchanged",
};
const anyWebsite: PermissionItem = {
  permission: { kind: "httpOrigin", origin: "*" },
  severity: "warning",
  change: "added",
};

describe("PermissionSummary", () => {
  it("renders nothing without permissions", () => {
    const { container } = render(<PermissionSummary items={[]} />);

    expect(container).toBeEmptyDOMElement();
  });

  it("lists warnings first, then notices, then info", () => {
    render(<PermissionSummary items={[settings, clipboard, anyWebsite]} />);

    const titles = screen.getAllByRole("listitem").map((item) => item.dataset.severity);
    expect(titles).toEqual(["warning", "notice", "info"]);
  });

  it("marks each item with its severity and shows its wording", () => {
    render(<PermissionSummary items={[anyWebsite]} />);

    const item = screen.getByRole("listitem");
    expect(item).toHaveAttribute("data-severity", "warning");
    expect(item).toHaveTextContent("Connect to any website");
  });

  it("marks added permissions only when changes are shown", () => {
    const { rerender } = render(<PermissionSummary items={[anyWebsite, clipboard]} />);
    expect(screen.queryByText("New")).not.toBeInTheDocument();

    rerender(<PermissionSummary items={[anyWebsite, clipboard]} showChanges />);

    const [added, unchanged] = screen.getAllByRole("listitem");
    expect(within(added).getByText("New")).toBeInTheDocument();
    expect(within(unchanged).queryByText("New")).not.toBeInTheDocument();
  });

  it("lists permissions the new version no longer asks for", () => {
    render(
      <PermissionSummary
        items={[settings]}
        removed={[{ ...clipboard, change: "removed" }]}
        showChanges
      />,
    );

    const removedList = screen.getByRole("list", { name: "No longer requested" });
    expect(removedList).toHaveTextContent("Copy text to the clipboard");
  });
});
