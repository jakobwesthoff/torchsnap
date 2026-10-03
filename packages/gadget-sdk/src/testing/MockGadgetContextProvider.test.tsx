// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { renderHook } from "@testing-library/react";
import type { ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";
import { useLauncher } from "../shims/hooks";
import { MockGadgetContextProvider, type MockLauncherActions } from "./MockGadgetContextProvider";
import { setupSdkGlobalsForTesting } from "./setup";

setupSdkGlobalsForTesting();

function renderLauncher(launcher: MockLauncherActions) {
  const wrapper = ({ children }: { children: ReactNode }) => (
    <MockGadgetContextProvider launcher={launcher}>{children}</MockGadgetContextProvider>
  );
  return renderHook(() => useLauncher(), { wrapper }).result.current;
}

describe("MockGadgetContextProvider launcher slice", () => {
  it("defaults onExecute to a resolving promise and showError to a no-op", async () => {
    const launcher = renderLauncher({});
    await expect(launcher.onExecute("entry-1", "primary")).resolves.toBeUndefined();
    expect(() => launcher.showError("copy failed")).not.toThrow();
  });

  it("wraps an onExecute override that returns nothing in a promise", async () => {
    const onExecute = vi.fn(() => {});
    const launcher = renderLauncher({ onExecute });

    await expect(launcher.onExecute("entry-1", "copy")).resolves.toBeUndefined();
    expect(onExecute).toHaveBeenCalledWith("entry-1", "copy");
  });

  it("passes on a rejecting onExecute override", async () => {
    const launcher = renderLauncher({ onExecute: () => Promise.reject("entry is gone") });
    await expect(launcher.onExecute("entry-1", "copy")).rejects.toBe("entry is gone");
  });

  it("uses a showError override", () => {
    const showError = vi.fn();
    renderLauncher({ showError }).showError("copy failed");
    expect(showError).toHaveBeenCalledWith("copy failed");
  });
});
