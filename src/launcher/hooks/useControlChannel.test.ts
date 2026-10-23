// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { renderHook, waitFor } from "@testing-library/react";
import type { Channel } from "@tauri-apps/api/core";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mockCommands } from "../../test/tauri";
import type { ControlCommand, LauncherSnapshot } from "../../types";
import { useControlChannel } from "./useControlChannel";

function snapshot(query: string): LauncherSnapshot {
  return {
    query,
    searching: false,
    results: [],
    selectedIndex: null,
    inlineView: null,
    inlineViewSelected: false,
    gadgetView: null,
    contentVisible: true,
    inputFocused: true,
  };
}

describe("useControlChannel", () => {
  let channel: Channel<ControlCommand> | null;
  let reports: { requestId: number; snapshot: LauncherSnapshot }[];

  beforeEach(() => {
    channel = null;
    reports = [];
    mockCommands({
      control_subscribe: ({ channel: subscribed }) => {
        channel = subscribed;
      },
      control_report_snapshot: (params) => {
        reports.push(params);
      },
    });
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  /** Deliver a command the way the backend's channel does. */
  async function send(command: ControlCommand) {
    await waitFor(() => expect(channel).not.toBeNull());
    channel!.onmessage(command);
  }

  function render(getSnapshot: () => LauncherSnapshot = () => snapshot("")) {
    const resetState = vi.fn();
    const setQuery = vi.fn();
    const hook = renderHook((props) => useControlChannel(props), {
      initialProps: { resetState, setQuery, getSnapshot },
    });
    return { resetState, setQuery, hook };
  }

  it("resets the launcher on dismiss", async () => {
    const { resetState } = render();
    await send({ type: "dismiss" });
    expect(resetState).toHaveBeenCalledOnce();
  });

  it("sets the query on setQuery", async () => {
    const { setQuery } = render();
    await send({ type: "setQuery", text: "firefox" });
    expect(setQuery).toHaveBeenCalledWith("firefox");
  });

  it("answers reportSnapshot with the launcher's snapshot and the request id", async () => {
    render(() => snapshot("fire"));
    await send({ type: "reportSnapshot", requestId: 7 });
    await waitFor(() => expect(reports).toEqual([{ requestId: 7, snapshot: snapshot("fire") }]));
  });

  it("answers with the latest launcher state after a re-render", async () => {
    const { resetState, setQuery, hook } = render(() => snapshot("old"));
    hook.rerender({ resetState, setQuery, getSnapshot: () => snapshot("new") });
    await send({ type: "reportSnapshot", requestId: 8 });
    await waitFor(() => expect(reports).toEqual([{ requestId: 8, snapshot: snapshot("new") }]));
  });

  it("keeps handling commands after a snapshot fails to build", async () => {
    const error = vi.spyOn(console, "error").mockImplementation(() => {});
    const { resetState } = render(() => {
      throw new Error("view state unavailable");
    });
    await send({ type: "reportSnapshot", requestId: 9 });
    await send({ type: "dismiss" });
    expect(reports).toEqual([]);
    expect(resetState).toHaveBeenCalledOnce();
    expect(error).toHaveBeenCalled();
  });
});
