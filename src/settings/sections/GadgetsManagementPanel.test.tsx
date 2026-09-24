// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { mockWindows } from "@tauri-apps/api/mocks";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { mockCommands } from "../../test/tauri";
import { GadgetsManagementPanel } from "./GadgetsManagementPanel";

vi.mock("../../gadgets/registry", () => ({
  getGadgetsWithSettings: () => [
    { id: "weather", label: "Weather", description: "Forecasts" },
    { id: "clipboard-manager", label: "Clipboard", description: "History" },
  ],
}));

vi.mock("../../hooks/useSetting", () => ({
  useSetting: () => [true, () => {}],
}));

const dialog = vi.hoisted(() => ({ selection: null as string | null }));
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: () => Promise.resolve(dialog.selection),
}));

// The drop zone's listener is captured so tests can deliver drag
// events and control when registration resolves.
type DragHandler = (event: { payload: { type: string; paths?: string[] } }) => void;
const webview = vi.hoisted(() => ({
  handler: undefined as DragHandler | undefined,
  register: (unlisten: () => void) => Promise.resolve(unlisten),
  unlisten: () => {},
}));
vi.mock("@tauri-apps/api/webview", () => ({
  getCurrentWebview: () => ({
    onDragDropEvent: (handler: DragHandler) => {
      webview.handler = handler;
      return webview.register(webview.unlisten);
    },
  }),
}));

describe("GadgetsManagementPanel", () => {
  beforeEach(() => {
    mockWindows("settings");
    dialog.selection = null;
    webview.handler = undefined;
    webview.register = (unlisten) => Promise.resolve(unlisten);
    webview.unlisten = () => {};
  });

  function mockPanelCommands(submitted: unknown[]) {
    mockCommands({
      gadget_sources: () => ({}),
      gadget_permissions: () => ({}),
      install_queue_snapshot: () => [],
      install_queue_submit: (params) => {
        submitted.push(params);
      },
    });
  }

  it("submits the picked file to the install queue", async () => {
    const submitted: unknown[] = [];
    mockPanelCommands(submitted);
    dialog.selection = "/Downloads/weather.torchsnap";
    render(<GadgetsManagementPanel />);

    await userEvent.click(screen.getByRole("button", { name: "Choose a file…" }));

    await waitFor(() =>
      expect(submitted).toEqual([
        { paths: ["/Downloads/weather.torchsnap"], origin: "settingsPicker" },
      ]),
    );
  });

  it("submits every dropped archive to the install queue", async () => {
    const submitted: unknown[] = [];
    mockPanelCommands(submitted);
    render(<GadgetsManagementPanel />);
    await waitFor(() => expect(webview.handler).toBeDefined());

    webview.handler?.({
      payload: { type: "drop", paths: ["/tmp/a.torchsnap", "/tmp/b.torchsnap"] },
    });

    await waitFor(() =>
      expect(submitted).toEqual([
        { paths: ["/tmp/a.torchsnap", "/tmp/b.torchsnap"], origin: "settingsDrop" },
      ]),
    );
  });

  it("rejects a drop that mixes in other files", async () => {
    const submitted: unknown[] = [];
    mockPanelCommands(submitted);
    render(<GadgetsManagementPanel />);
    await waitFor(() => expect(webview.handler).toBeDefined());

    webview.handler?.({ payload: { type: "drop", paths: ["/tmp/a.torchsnap", "/tmp/notes.txt"] } });

    expect(
      await screen.findByText("Only .torchsnap files can be installed as gadgets."),
    ).toBeInTheDocument();
    expect(submitted).toEqual([]);
  });

  it("removes the drop listener even when unmounted before it was registered", async () => {
    mockPanelCommands([]);
    let resolveRegistration: (unlisten: () => void) => void = () => {};
    webview.register = () =>
      new Promise((resolve) => {
        resolveRegistration = resolve;
      });
    const unlisten = vi.fn();

    const { unmount } = render(<GadgetsManagementPanel />);
    unmount();
    resolveRegistration(unlisten);
    await Promise.resolve();

    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  it("shows each gadget's permissions on its card", async () => {
    mockCommands({
      gadget_sources: () => ({ weather: "user", "clipboard-manager": "builtin" }),
      gadget_permissions: () => ({
        weather: [
          {
            permission: { kind: "httpOrigin", origin: "https://api.weather.example" },
            severity: "notice",
            change: "unchanged",
          },
        ],
      }),
      install_queue_snapshot: () => [],
    });

    render(<GadgetsManagementPanel />);

    const card = (await screen.findByText("Weather")).closest("[data-gadget]");
    expect(card).not.toBeNull();
    expect(
      within(card as HTMLElement).getByText("Connect to https://api.weather.example"),
    ).toBeInTheDocument();

    const builtin = screen.getByText("Clipboard").closest("[data-gadget]") as HTMLElement;
    expect(within(builtin).queryByRole("list", { name: "Permissions" })).not.toBeInTheDocument();
  });

  it("reviews the first queued request and reports the install", async () => {
    let queue = [
      {
        id: "r1",
        origin: "osOpenFile" as const,
        sourcePath: "/Downloads/weather.torchsnap",
        state: {
          kind: "ready" as const,
          review: {
            gadget: { id: "weather", name: "Weather", description: "Forecasts", version: "1.0.0" },
            sourcePath: "/Downloads/weather.torchsnap",
            provenance: null,
            action: { kind: "install" as const },
            permissions: [],
            removedPermissions: [],
          },
        },
      },
    ];
    const confirmed: string[] = [];
    mockCommands({
      gadget_sources: () => ({}),
      gadget_permissions: () => ({}),
      install_queue_snapshot: () => queue,
      install_queue_confirm: ({ requestId }) => {
        confirmed.push(requestId);
        queue = [];
        return {
          id: "weather",
          name: "Weather",
          version: "1.0.0",
          previousVersion: null,
          versionRelation: null,
          requiresRestart: true,
        };
      },
    });

    render(<GadgetsManagementPanel />);

    const dialog = await screen.findByRole("dialog", { name: "Install Weather?" });
    await userEvent.click(within(dialog).getByRole("button", { name: "Install" }));

    expect(confirmed).toEqual(["r1"]);
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(screen.getByText("Installed Weather 1.0.0.")).toBeInTheDocument();
  });
});
