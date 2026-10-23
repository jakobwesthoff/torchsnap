// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// The module reads the platform and the current window when it loads.
vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: () => ({ listen: vi.fn() }),
}));

const { isContentVisible } = await import("./visibility");

describe("isContentVisible", () => {
  beforeEach(() => {
    const root = document.createElement("div");
    root.id = "root";
    document.body.appendChild(root);
  });

  afterEach(() => {
    document.getElementById("root")?.remove();
  });

  it("is true while the root is shown", () => {
    expect(isContentVisible()).toBe(true);
  });

  it("is false while the root is blanked", () => {
    document.getElementById("root")!.style.visibility = "hidden";
    expect(isContentVisible()).toBe(false);
  });

  it("is true again once the root is restored", () => {
    const root = document.getElementById("root")!;
    root.style.visibility = "hidden";
    root.style.visibility = "visible";
    expect(isContentVisible()).toBe(true);
  });

  it("is true before the root exists", () => {
    document.getElementById("root")?.remove();
    expect(isContentVisible()).toBe(true);
  });
});
