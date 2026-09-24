// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { PermissionGroups } from "./PermissionGroups";
import type { PermissionItem } from "./types";

const anyWebsite: PermissionItem = {
  permission: { kind: "httpOrigin", origin: "*" },
  severity: "warning",
  change: "added",
};
const clipboard: PermissionItem = {
  permission: { kind: "clipboard" },
  severity: "notice",
  change: "unchanged",
};
const macToken: PermissionItem = {
  permission: { kind: "filesystemRead", pattern: "/Library/ZeroTier/authtoken.secret" },
  severity: "notice",
  change: "unchanged",
};
const windowsToken: PermissionItem = {
  permission: { kind: "filesystemRead", pattern: "C:\\ProgramData\\ZeroTier\\authtoken.secret" },
  severity: "notice",
  change: "unchanged",
};

describe("PermissionGroups", () => {
  it("renders nothing without permissions", () => {
    const { container } = render(<PermissionGroups items={[]} platform="macos" />);

    expect(container).toBeEmptyDOMElement();
  });

  it("shows each group with its entries", () => {
    render(<PermissionGroups items={[clipboard, anyWebsite]} platform="macos" />);

    const network = screen.getByRole("group", { name: "Network" });
    expect(network).toHaveTextContent("any website");
    const clipboardGroup = screen.getByRole("group", { name: "Clipboard" });
    expect(clipboardGroup).toHaveTextContent("copy text to the clipboard");
  });

  it("labels broad groups and explains the label", () => {
    render(<PermissionGroups items={[clipboard, anyWebsite]} platform="macos" />);

    const broad = within(screen.getByRole("group", { name: "Network" })).getByText("Broad access");
    expect(broad).toHaveAttribute("title", expect.stringContaining("beyond"));
    expect(
      within(screen.getByRole("group", { name: "Clipboard" })).queryByText("Broad access"),
    ).not.toBeInTheDocument();
  });

  it("keeps paths for other systems behind a toggle", async () => {
    render(<PermissionGroups items={[macToken, windowsToken]} platform="macos" />);
    const files = screen.getByRole("group", { name: "Files" });

    expect(files).toHaveTextContent("/Library/ZeroTier/authtoken.secret");
    expect(files).not.toHaveTextContent("ProgramData");

    await userEvent.click(within(files).getByRole("button", { name: "+1 for other systems" }));

    expect(files).toHaveTextContent("ProgramData");
  });

  it("marks what a new version adds and drops, with a summary", () => {
    render(
      <PermissionGroups
        items={[anyWebsite]}
        removed={[{ ...clipboard, change: "removed" }]}
        platform="macos"
        showChanges
      />,
    );

    expect(
      screen.getByText("Adds: any website · Drops: copy text to the clipboard"),
    ).toBeInTheDocument();
    expect(
      within(screen.getByRole("group", { name: "Network" })).getByText("New"),
    ).toBeInTheDocument();
    expect(
      within(screen.getByRole("group", { name: "Clipboard" })).getByText("Removed"),
    ).toBeInTheDocument();
  });

  it("shows no change markers when changes are not asked for", () => {
    render(<PermissionGroups items={[anyWebsite]} platform="macos" />);

    expect(screen.queryByText("New")).not.toBeInTheDocument();
    expect(screen.queryByText(/^Adds:/)).not.toBeInTheDocument();
  });
});
