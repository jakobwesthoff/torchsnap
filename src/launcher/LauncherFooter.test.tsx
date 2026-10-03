// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { FooterState } from "../types";
import { LauncherFooter } from "./LauncherFooter";

const FOOTER: FooterState = {
  primary: { combo: { modifiers: [], key: "Enter" }, label: "Run" },
  hints: [{ combo: { modifiers: ["Meta"], key: "c" }, label: "Copy" }],
};

describe("LauncherFooter", () => {
  it("shows the primary action and the hints", () => {
    render(<LauncherFooter footer={FOOTER} />);
    expect(screen.getByText("Run")).toBeInTheDocument();
    expect(screen.getByText("Copy")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("renders nothing without actions or an error", () => {
    const { container } = render(<LauncherFooter footer={{ hints: [] }} />);
    expect(container).toBeEmptyDOMElement();
  });

  it("shows an error in place of the hints, with the full text as tooltip", () => {
    const message = "Eject Disc failed: eject disc via drutil failed (exit status: 1): no drive";
    render(<LauncherFooter footer={FOOTER} error={message} />);

    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent(message);
    expect(alert).toHaveAttribute("title", message);
    expect(screen.queryByText("Run")).toBeNull();
    expect(screen.queryByText("Copy")).toBeNull();
  });

  it("shows an error even when there are no actions", () => {
    render(<LauncherFooter footer={{ hints: [] }} error="Clipboard failed: busy" />);
    expect(screen.getByRole("alert")).toHaveTextContent("Clipboard failed: busy");
  });
});
