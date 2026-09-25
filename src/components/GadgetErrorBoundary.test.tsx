// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { GadgetErrorBoundary } from "./GadgetErrorBoundary";

const logged = vi.hoisted(() => ({ error: vi.fn(), sources: [] as string[] }));
vi.mock("../lib/logger", () => ({
  createLogger: (source: string) => {
    logged.sources.push(source);
    return { error: logged.error };
  },
}));

function BrokenView(): never {
  throw new TypeError("data.items is undefined");
}

describe("GadgetErrorBoundary", () => {
  beforeEach(() => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    logged.error.mockClear();
    logged.sources.length = 0;
  });
  afterEach(() => vi.restoreAllMocks());

  it("renders the gadget's view while it works", () => {
    render(
      <GadgetErrorBoundary gadgetId="weather">
        <p>forecast</p>
      </GadgetErrorBoundary>,
    );
    expect(screen.getByText("forecast")).toBeInTheDocument();
  });

  it("replaces a throwing view with an error card naming the gadget", () => {
    render(
      <GadgetErrorBoundary gadgetId="weather">
        <BrokenView />
      </GadgetErrorBoundary>,
    );
    expect(screen.getByRole("alert")).toHaveTextContent("weather");
    expect(screen.getByRole("alert")).toHaveTextContent("data.items is undefined");
  });

  it("logs the error under the gadget's id", () => {
    render(
      <GadgetErrorBoundary gadgetId="weather">
        <BrokenView />
      </GadgetErrorBoundary>,
    );
    expect(logged.sources).toContain("weather");
    expect(logged.error).toHaveBeenCalledOnce();
    expect(logged.error.mock.calls[0][0]).toContain("data.items is undefined");
  });

  it("offers Go back when the caller can leave the view", async () => {
    const onGoBack = vi.fn();
    render(
      <GadgetErrorBoundary gadgetId="weather" onGoBack={onGoBack}>
        <BrokenView />
      </GadgetErrorBoundary>,
    );
    await userEvent.click(screen.getByRole("button", { name: "Go back" }));
    expect(onGoBack).toHaveBeenCalledOnce();
  });

  it("offers no Go back without a way to leave the view", () => {
    render(
      <GadgetErrorBoundary gadgetId="weather">
        <BrokenView />
      </GadgetErrorBoundary>,
    );
    expect(screen.queryByRole("button", { name: "Go back" })).toBeNull();
  });
});
