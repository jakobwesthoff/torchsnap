// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mockCommands } from "../test/tauri";
import { UpdatesSection } from "./UpdatesSection";

// The settings store is replaced by a plain map; what matters here is
// which key the section reads and what it writes.
const settings = vi.hoisted(() => new Map<string, unknown>());
vi.mock("../hooks/useSetting", () => ({
  useSetting: (key: string) => {
    const [value, setValue] = useState(() => settings.get(key));
    return [
      value,
      async (next: unknown) => {
        settings.set(key, next);
        setValue(next);
      },
    ];
  },
}));

describe("UpdatesSection", () => {
  beforeEach(() => settings.clear());

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("shows automatic checks off while the user has not answered", () => {
    render(<UpdatesSection />);
    expect(screen.getByRole("switch")).toHaveAttribute("aria-checked", "false");
  });

  it("shows automatic checks on after the user agreed", () => {
    settings.set("updates.automaticChecks", true);
    render(<UpdatesSection />);
    expect(screen.getByRole("switch")).toHaveAttribute("aria-checked", "true");
  });

  it("stores the choice", async () => {
    render(<UpdatesSection />);
    await userEvent.click(screen.getByRole("switch"));
    expect(settings.get("updates.automaticChecks")).toBe(true);

    await userEvent.click(screen.getByRole("switch"));
    expect(settings.get("updates.automaticChecks")).toBe(false);
  });

  it("says what the check sends where", () => {
    render(<UpdatesSection />);
    expect(screen.getByText(/asks torchsnap\.app/)).toHaveTextContent("IP address");
  });

  it("starts a manual check", async () => {
    const check = vi.fn();
    mockCommands({ update_check: check });
    render(<UpdatesSection />);
    await userEvent.click(screen.getByRole("button", { name: "Check for Updates" }));
    expect(check).toHaveBeenCalledOnce();
  });

  it("opens the welcome window again", async () => {
    const show = vi.fn();
    mockCommands({ welcome_show: show });
    render(<UpdatesSection />);
    await userEvent.click(screen.getByRole("button", { name: "Show Welcome" }));
    expect(show).toHaveBeenCalledOnce();
  });

  it("stays usable when the check cannot start", async () => {
    // `update_check` deliberately fails, so `command()` logs the
    // rejection. That logging is part of the contract under test, so
    // the warning is captured and asserted on instead of left to print.
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    mockCommands({
      update_check: () => {
        throw new Error("no backend");
      },
    });
    render(<UpdatesSection />);
    await userEvent.click(screen.getByRole("button", { name: "Check for Updates" }));
    expect(screen.getByRole("button", { name: "Check for Updates" })).toBeEnabled();
    expect(warn).toHaveBeenCalledWith('command("update_check") failed:', expect.anything());
  });
});
