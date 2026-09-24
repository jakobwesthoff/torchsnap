// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ReleaseNotesView } from "./ReleaseNotesView";

const openUrl = vi.hoisted(() => vi.fn(() => Promise.resolve()));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl }));

function renderNotes(notes: string, date: string | null = "2026-10-02") {
  return render(<ReleaseNotesView releases={[{ version: "0.12.0", date, notes }]} />);
}

describe("ReleaseNotesView", () => {
  beforeEach(() => openUrl.mockClear());

  it("shows every release with its version and date, in order", () => {
    render(
      <ReleaseNotesView
        releases={[
          { version: "0.12.1", date: "2026-10-10", notes: "- Second" },
          { version: "0.12.0", date: null, notes: "- First" },
        ]}
      />,
    );
    const sections = screen.getAllByRole("region");
    expect(sections.map((s) => s.getAttribute("aria-label"))).toEqual([
      "Torchsnap 0.12.1",
      "Torchsnap 0.12.0",
    ]);
    expect(sections[0]).toHaveTextContent("2026-10-10");
    expect(sections[1]).toHaveTextContent("First");
  });

  it("renders the Markdown of the notes", () => {
    renderNotes("### Added\n\n- **Updates** from `torchsnap.app`.");
    expect(screen.getByRole("heading", { name: "Added" })).toBeInTheDocument();
    expect(screen.getByRole("listitem")).toHaveTextContent("Updates from torchsnap.app.");
    expect(screen.getByText("Updates").tagName).toBe("STRONG");
  });

  it("drops raw HTML instead of rendering it", () => {
    const { container } = renderNotes(
      'Before <img src=x onerror="alert(1)"><script>alert(2)</script> after',
    );
    expect(container.querySelector("img")).toBeNull();
    expect(container.querySelector("script")).toBeNull();
    expect(container).not.toHaveTextContent("<script>");
  });

  it("does not load Markdown images", () => {
    const { container } = renderNotes("![tracker](https://example.org/pixel.png)");
    expect(container.querySelector("img")).toBeNull();
  });

  it("opens https links in the browser instead of the window", async () => {
    renderNotes("See [the docs](https://docs.torchsnap.app/).");
    await userEvent.click(screen.getByRole("link", { name: "the docs" }));
    expect(openUrl).toHaveBeenCalledWith("https://docs.torchsnap.app/");
  });

  it("does not open javascript links", async () => {
    renderNotes("[click](javascript:alert(1))");
    await userEvent.click(screen.getByText("click"));
    expect(openUrl).not.toHaveBeenCalled();
  });

  it("does not open relative links", async () => {
    renderNotes("[local](/etc/passwd)");
    await userEvent.click(screen.getByText("local"));
    expect(openUrl).not.toHaveBeenCalled();
  });
});
