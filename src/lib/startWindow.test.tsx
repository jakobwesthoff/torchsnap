// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { act, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { startWindow } from "./startWindow";

const emit = vi.hoisted(() => vi.fn(() => Promise.resolve()));
vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: () => ({ emit }),
}));

const logError = vi.hoisted(() => vi.fn());
vi.mock("./logger", () => ({
  createLogger: () => ({ error: logError, warn: vi.fn(), info: vi.fn(), debug: vi.fn() }),
}));

vi.mock("../contexts/ThemeProvider", () => ({
  ThemeProvider: ({ children }: { children: React.ReactNode }) => children,
}));

describe("startWindow", () => {
  beforeEach(() => {
    emit.mockClear();
    logError.mockClear();
    document.body.innerHTML = '<div id="root"></div>';
  });

  afterEach(() => {
    document.body.innerHTML = "";
  });

  it("renders the window after its setup and then asks to be shown", async () => {
    const order: string[] = [];
    await act(() =>
      startWindow({
        window: "settings",
        prepare: async () => {
          order.push("prepare");
        },
        app: () => <p>Settings panel</p>,
      }),
    );

    expect(screen.getByText("Settings panel")).toBeInTheDocument();
    expect(order).toEqual(["prepare"]);
    expect(emit).toHaveBeenCalledWith("react-ready");
  });

  it("renders a window without setup", async () => {
    await act(() => startWindow({ window: "update", app: () => <p>Update</p> }));

    expect(screen.getByText("Update")).toBeInTheDocument();
    expect(emit).toHaveBeenCalledWith("react-ready");
  });

  // The window is created hidden and shown on `react-ready`. A failed
  // setup must still show it, with the error, rather than leave it
  // invisible.
  it("shows the error and still asks to be shown when setup fails", async () => {
    const app = vi.fn(() => <p>Settings panel</p>);
    await act(() =>
      startWindow({
        window: "settings",
        prepare: () => Promise.reject(new Error("store unavailable")),
        app,
      }),
    );

    expect(screen.getByRole("alert")).toHaveTextContent("store unavailable");
    expect(app).not.toHaveBeenCalled();
    expect(emit).toHaveBeenCalledWith("react-ready");
    expect(logError).toHaveBeenCalledWith(
      expect.stringContaining("store unavailable"),
      expect.arrayContaining([["window", "settings"]]),
    );
  });

  it("shows a rejected string as the error", async () => {
    await act(() =>
      startWindow({
        window: "devtools",
        prepare: () => Promise.reject("ipc closed"),
        app: () => <p>DevTools</p>,
      }),
    );

    expect(screen.getByRole("alert")).toHaveTextContent("ipc closed");
  });
});
