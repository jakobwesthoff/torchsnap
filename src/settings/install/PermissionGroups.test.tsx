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

  it("shows the folder a command rule may run in below the rule", () => {
    const gitInProjects: PermissionItem = {
      permission: { kind: "command", binary: "git", argv: [], cwd: "${home}/projects", env: [] },
      severity: "warning",
      change: "added",
    };
    render(<PermissionGroups items={[gitInProjects]} platform="macos" />);

    const programs = screen.getByRole("group", { name: "Runs programs" });
    expect(within(programs).getByText("in ~/projects or a folder inside it")).toBeInTheDocument();
  });

  it("says that programs run outside the sandbox and marks free arguments", () => {
    const anyGit: PermissionItem = {
      permission: {
        kind: "command",
        binary: "git",
        argv: [{ kind: "rest", constraint: { kind: "anyString" } }],
        cwd: null,
        env: [],
      },
      severity: "warning",
      change: "unchanged",
    };
    render(<PermissionGroups items={[anyGit]} platform="macos" />);

    const programs = screen.getByRole("group", { name: "Runs programs" });
    expect(programs).toHaveTextContent(
      "Each program runs outside the gadget's sandbox, with your rights.",
    );
    expect(within(programs).getByText("Any arguments")).toHaveAttribute(
      "title",
      "The gadget can pass this program arguments of its choosing, so it can make the program do anything those arguments allow.",
    );
  });

  it("writes a command's binary, arguments and tags as one line, a variable's tag on its line", () => {
    const git: PermissionItem = {
      permission: {
        kind: "command",
        binary: "git",
        argv: [{ kind: "rest", constraint: { kind: "anyString" } }],
        cwd: null,
        env: [{ key: "GIT_COMMIT_MESSAGE", value: { kind: "anyString" } }],
      },
      severity: "warning",
      change: "unchanged",
    };
    render(<PermissionGroups items={[git]} platform="macos" />);

    const header = screen.getByText("git").parentElement;
    expect(header).toHaveTextContent("git with any number of: any textAny arguments");
    expect(header).not.toHaveTextContent("Any value");
    const variable = screen.getByText("sets GIT_COMMIT_MESSAGE to any text").parentElement;
    expect(variable).toHaveTextContent("Any value");
    expect(variable).not.toHaveTextContent("Any arguments");
  });

  it("shows no programs note without command rules", () => {
    render(<PermissionGroups items={[clipboard]} platform="macos" />);

    expect(screen.queryByText(/outside the gadget's sandbox/)).not.toBeInTheDocument();
  });

  it("strikes through the folder of a removed command rule", () => {
    const removedGit: PermissionItem = {
      permission: { kind: "command", binary: "git", argv: [], cwd: "${home}/projects", env: [] },
      severity: "warning",
      change: "removed",
    };
    render(<PermissionGroups items={[]} removed={[removedGit]} platform="macos" showChanges />);

    expect(screen.getByText("in ~/projects or a folder inside it")).toHaveClass("line-through");
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
