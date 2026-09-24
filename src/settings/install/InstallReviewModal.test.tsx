// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { InstallReviewModal } from "./InstallReviewModal";
import type { InstallRequestView, InstallReview, ReviewAction } from "./types";

function review(overrides: Partial<InstallReview> = {}): InstallReview {
  return {
    gadget: { id: "weather", name: "Weather", description: "Forecasts", version: "2.0.0" },
    sourcePath: "/Users/me/Downloads/weather.torchsnap",
    provenance: null,
    action: { kind: "install" },
    permissions: [],
    removedPermissions: [],
    ...overrides,
  };
}

function ready(overrides: Partial<InstallReview> = {}): InstallRequestView {
  return {
    id: "r1",
    origin: "osOpenFile",
    sourcePath: "/Users/me/Downloads/weather.torchsnap",
    state: { kind: "ready", review: review(overrides) },
  };
}

function replace(relation: "upgrade" | "same" | "downgrade" | "unknown"): ReviewAction {
  return { kind: "replace", previousVersion: "1.0.0", relation };
}

function renderModal(request: InstallRequestView) {
  const onInstall = vi.fn();
  const onCancel = vi.fn();
  render(<InstallReviewModal request={request} onInstall={onInstall} onCancel={onCancel} />);
  return { onInstall, onCancel };
}

describe("InstallReviewModal", () => {
  it("shows the gadget, its version, id and where the file is", () => {
    renderModal(ready());

    const dialog = screen.getByRole("dialog", { name: "Install Weather?" });
    expect(dialog).toHaveTextContent("Weather 2.0.0");
    expect(dialog).toHaveTextContent("weather");
    expect(dialog).toHaveTextContent("Forecasts");
    expect(dialog).toHaveTextContent("/Users/me/Downloads/weather.torchsnap");
  });

  it("shows where a downloaded file came from", () => {
    renderModal(
      ready({
        provenance: {
          downloadUrl: "https://github.com/acme/weather/releases/download/v2/weather.torchsnap",
          referrerUrl: null,
          downloadedBy: "Safari",
        },
      }),
    );

    expect(screen.getByText("Downloaded from github.com with Safari")).toBeInTheDocument();
  });

  it("describes a replace by its version relation", () => {
    const cases: [ReviewAction, string][] = [
      [replace("upgrade"), "Updates version 1.0.0 to 2.0.0"],
      [replace("same"), "Reinstalls version 2.0.0"],
      [replace("unknown"), "Replaces version 1.0.0 with 2.0.0"],
    ];
    for (const [action, text] of cases) {
      const { unmount } = render(
        <InstallReviewModal request={ready({ action })} onInstall={() => {}} onCancel={() => {}} />,
      );
      expect(screen.getByRole("dialog", { name: "Replace Weather?" })).toHaveTextContent(text);
      unmount();
    }
  });

  it("warns about a downgrade", () => {
    renderModal(ready({ action: replace("downgrade") }));

    expect(screen.getByRole("alert")).toHaveTextContent(
      "This is an older version than the installed 1.0.0",
    );
  });

  it("marks permissions the new version adds and lists the ones it drops", () => {
    renderModal(
      ready({
        action: replace("upgrade"),
        permissions: [
          { permission: { kind: "httpOrigin", origin: "*" }, severity: "warning", change: "added" },
        ],
        removedPermissions: [
          { permission: { kind: "clipboard" }, severity: "notice", change: "removed" },
        ],
      }),
    );

    expect(screen.getByText("New")).toBeInTheDocument();
    expect(screen.getByRole("list", { name: "No longer requested" })).toBeInTheDocument();
  });

  it("offers only dismissal for a rejected gadget", async () => {
    const { onCancel } = renderModal(
      ready({ action: { kind: "reject", reason: "Built-in gadgets cannot be replaced." } }),
    );

    expect(screen.getByRole("dialog")).toHaveTextContent("Built-in gadgets cannot be replaced.");
    expect(screen.queryByRole("button", { name: /install/i })).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Dismiss" }));
    expect(onCancel).toHaveBeenCalledWith("r1");
  });

  it("installs on confirm and cancels on Cancel or Escape", async () => {
    const { onInstall, onCancel } = renderModal(ready());

    await userEvent.click(screen.getByRole("button", { name: "Install" }));
    expect(onInstall).toHaveBeenCalledWith("r1");

    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await userEvent.keyboard("{Escape}");
    expect(onCancel).toHaveBeenCalledTimes(2);
  });

  it("keeps keyboard focus inside the dialog", async () => {
    renderModal(ready());
    const install = screen.getByRole("button", { name: "Install" });
    const cancel = screen.getByRole("button", { name: "Cancel" });

    expect(install).toHaveFocus();
    await userEvent.tab();
    expect(cancel).toHaveFocus();
    await userEvent.tab();
    expect(install).toHaveFocus();
    await userEvent.tab({ shift: true });
    expect(cancel).toHaveFocus();
  });

  it("shows progress while the file is being read", () => {
    renderModal({ ...ready(), state: { kind: "staging" } });

    expect(screen.getByRole("dialog")).toHaveTextContent("Reading weather.torchsnap");
    expect(screen.getByRole("button", { name: "Cancel" })).toBeInTheDocument();
  });

  it("shows why a request failed", async () => {
    const { onCancel } = renderModal({
      ...ready(),
      state: { kind: "failed", message: "the gadget archive is larger than the 16 MiB limit" },
    });

    expect(screen.getByRole("dialog")).toHaveTextContent("larger than the 16 MiB limit");
    await userEvent.click(screen.getByRole("button", { name: "Dismiss" }));
    expect(onCancel).toHaveBeenCalledWith("r1");
  });
});
