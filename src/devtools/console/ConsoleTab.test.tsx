// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Platform } from "../../keybindings/platform";
import { ConsoleTab } from "./ConsoleTab";

const state = vi.hoisted(() => ({
  platform: "macos" as Platform,
  clear: vi.fn(),
}));

vi.mock("../../keybindings/platform", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../keybindings/platform")>()),
  get platform() {
    return state.platform;
  },
}));

vi.mock("./useLogStream", () => ({
  useLogStream: () => ({
    items: [],
    droppedCount: 0,
    clear: state.clear,
    spanDepthMap: new Map(),
    completedSpanIds: new Set(),
  }),
}));

function searchInput(): HTMLElement {
  return screen.getByPlaceholderText("Filter...");
}

describe("ConsoleTab shortcuts", () => {
  beforeEach(() => state.clear.mockClear());

  describe("on macOS", () => {
    beforeEach(() => {
      state.platform = "macos";
    });

    it("clears the log with Cmd+K and focuses search with Cmd+F", () => {
      render(<ConsoleTab />);
      fireEvent.keyDown(window, { key: "k", metaKey: true });
      expect(state.clear).toHaveBeenCalledOnce();

      fireEvent.keyDown(window, { key: "f", metaKey: true });
      expect(searchInput()).toHaveFocus();
    });

    it("ignores extra modifiers", () => {
      render(<ConsoleTab />);
      fireEvent.keyDown(window, { key: "K", metaKey: true, shiftKey: true });
      fireEvent.keyDown(window, { key: "k", metaKey: true, altKey: true });
      expect(state.clear).not.toHaveBeenCalled();
    });

    it("matches under CapsLock", () => {
      render(<ConsoleTab />);
      fireEvent.keyDown(window, { key: "K", metaKey: true });
      expect(state.clear).toHaveBeenCalledOnce();
    });

    it("advertises Cmd+K on the clear button", () => {
      render(<ConsoleTab />);
      expect(screen.getByTitle("Clear log (⌘K)")).toBeInTheDocument();
    });
  });

  describe("on Linux", () => {
    beforeEach(() => {
      state.platform = "linux";
    });

    it("clears the log with Ctrl+K and focuses search with Ctrl+F", () => {
      render(<ConsoleTab />);
      fireEvent.keyDown(window, { key: "k", ctrlKey: true });
      expect(state.clear).toHaveBeenCalledOnce();

      fireEvent.keyDown(window, { key: "f", ctrlKey: true });
      expect(searchInput()).toHaveFocus();
    });

    it("ignores the Super key", () => {
      render(<ConsoleTab />);
      fireEvent.keyDown(window, { key: "k", metaKey: true });
      expect(state.clear).not.toHaveBeenCalled();
    });

    it("advertises Ctrl+K on the clear button", () => {
      render(<ConsoleTab />);
      expect(screen.getByTitle("Clear log (Ctrl+K)")).toBeInTheDocument();
    });
  });
});
