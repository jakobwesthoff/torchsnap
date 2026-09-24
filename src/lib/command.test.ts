// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { afterEach, describe, expect, it, vi } from "vitest";
import { command } from "./command";
import { mockCommands } from "../test/tauri";

describe("command", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("passes the parameters to the backend command and returns its result", async () => {
    const calls: unknown[] = [];
    mockCommands({
      uninstall_user_gadget: (params) => {
        calls.push(params);
        return { requiresRestart: true };
      },
    });

    const result = await command("uninstall_user_gadget", { gadgetId: "weather" });

    expect(calls).toEqual([{ gadgetId: "weather" }]);
    expect(result).toEqual({ requiresRestart: true });
  });

  it("calls parameterless commands without arguments", async () => {
    mockCommands({ gadget_sources: () => ({ weather: "user" }) });

    await expect(command("gadget_sources")).resolves.toEqual({ weather: "user" });
  });

  it("logs a rejection and re-throws it to the caller", async () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    mockCommands({
      uninstall_user_gadget: () => {
        throw new Error("unknown gadget id `weather`");
      },
    });

    await expect(command("uninstall_user_gadget", { gadgetId: "weather" })).rejects.toThrow(
      "unknown gadget id `weather`",
    );
    expect(warn).toHaveBeenCalledWith(
      'command("uninstall_user_gadget") failed:',
      expect.any(Error),
    );
  });
});
