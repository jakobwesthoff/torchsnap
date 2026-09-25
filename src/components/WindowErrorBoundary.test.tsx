// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { WindowErrorBoundary } from "./WindowErrorBoundary";

const logged = vi.hoisted(() => ({ error: vi.fn(), sources: [] as string[] }));
vi.mock("../lib/logger", () => ({
  createLogger: (source: string) => {
    logged.sources.push(source);
    return { error: logged.error };
  },
}));

function BrokenWindow(): never {
  throw new Error("settings store missing");
}

describe("WindowErrorBoundary", () => {
  beforeEach(() => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    logged.error.mockClear();
    logged.sources.length = 0;
  });
  afterEach(() => vi.restoreAllMocks());

  it("renders the window while it works", () => {
    render(
      <WindowErrorBoundary window="settings">
        <p>settings panel</p>
      </WindowErrorBoundary>,
    );
    expect(screen.getByText("settings panel")).toBeInTheDocument();
  });

  it("shows the error and logs it as a host error naming the window", () => {
    render(
      <WindowErrorBoundary window="settings">
        <BrokenWindow />
      </WindowErrorBoundary>,
    );
    expect(screen.getByRole("alert")).toHaveTextContent("settings store missing");
    expect(logged.sources).toContain("host");
    expect(logged.error).toHaveBeenCalledOnce();
    expect(logged.error.mock.calls[0][1]).toContainEqual(["window", "settings"]);
  });

  it("reloads the window from the fallback", async () => {
    const reload = vi.fn();
    vi.spyOn(window, "location", "get").mockReturnValue({ ...window.location, reload });
    render(
      <WindowErrorBoundary window="settings">
        <BrokenWindow />
      </WindowErrorBoundary>,
    );
    await userEvent.click(screen.getByRole("button", { name: "Reload window" }));
    expect(reload).toHaveBeenCalledOnce();
  });
});
