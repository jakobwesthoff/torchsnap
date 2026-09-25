// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { render, screen } from "@testing-library/react";
import { Suspense, type ComponentType } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ErrorBoundary } from "../components/ErrorBoundary";
import { launcherComponent } from "./gadgetComponent";

function renderLoaded(Component: ComponentType<{ name: string }>) {
  render(
    <ErrorBoundary fallback={(error) => <p>failed: {error.message}</p>}>
      <Suspense fallback={<p>loading</p>}>
        <Component name="weather" />
      </Suspense>
    </ErrorBoundary>,
  );
}

describe("launcherComponent", () => {
  // React reports every caught render error on the console.
  beforeEach(() => vi.spyOn(console, "error").mockImplementation(() => {}));
  afterEach(() => vi.restoreAllMocks());

  it("renders the loaded component", async () => {
    const View = launcherComponent(() =>
      Promise.resolve({ default: ({ name }: { name: string }) => <p>view for {name}</p> }),
    );
    renderLoaded(View);
    expect(await screen.findByText("view for weather")).toBeInTheDocument();
  });

  it("throws a failed import to the error boundary", async () => {
    const View = launcherComponent<{ name: string }>(() =>
      Promise.reject(new Error("bundle not found")),
    );
    renderLoaded(View);
    expect(await screen.findByText("failed: bundle not found")).toBeInTheDocument();
  });

  it("throws a module without a component instead of loading forever", async () => {
    const View = launcherComponent<{ name: string }>(() =>
      Promise.resolve({ default: undefined as unknown as ComponentType<{ name: string }> }),
    );
    renderLoaded(View);
    expect(await screen.findByText(/^failed: /)).toBeInTheDocument();
    expect(screen.queryByText("loading")).toBeNull();
  });
});
