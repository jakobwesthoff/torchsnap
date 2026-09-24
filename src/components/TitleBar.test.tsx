// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { TitleBar } from "./TitleBar";

const win = vi.hoisted(() => ({
  close: vi.fn(() => Promise.resolve()),
  minimize: vi.fn(() => Promise.resolve()),
  toggleMaximize: vi.fn(() => Promise.resolve()),
  isFullscreen: vi.fn(() => Promise.resolve(false)),
  setFullscreen: vi.fn(() => Promise.resolve()),
}));
vi.mock("@tauri-apps/api/webviewWindow", () => ({ getCurrentWebviewWindow: () => win }));

describe("TitleBar", () => {
  beforeEach(() => Object.values(win).forEach((fn) => fn.mockClear()));

  it("closes and minimizes the window", async () => {
    render(<TitleBar />);
    await userEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(win.close).toHaveBeenCalledOnce();
    await userEvent.click(screen.getByRole("button", { name: "Minimize" }));
    expect(win.minimize).toHaveBeenCalledOnce();
  });

  it("toggles fullscreen", async () => {
    render(<TitleBar />);
    await userEvent.click(screen.getByRole("button", { name: "Toggle Fullscreen" }));
    expect(win.setFullscreen).toHaveBeenCalledWith(true);
  });

  it("shows the title in the titlebar variant", () => {
    render(<TitleBar variant="titlebar" title="Software Update" />);
    expect(screen.getByText("Software Update")).toBeInTheDocument();
  });

  it("offers no way to close a window that must stay open", async () => {
    render(<TitleBar closable={false} />);
    const close = screen.getByRole("button", { name: "Close" });
    expect(close).toBeDisabled();
    await userEvent.click(close);
    expect(win.close).not.toHaveBeenCalled();
  });
});
