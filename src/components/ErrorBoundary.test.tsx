// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ErrorBoundary } from "./ErrorBoundary";

function Throws({ value }: { value: unknown }): never {
  throw value;
}

describe("ErrorBoundary", () => {
  // React reports every caught render error on the console.
  beforeEach(() => vi.spyOn(console, "error").mockImplementation(() => {}));
  afterEach(() => vi.restoreAllMocks());

  it("renders its children while nothing throws", () => {
    render(
      <ErrorBoundary fallback={() => <p>fallback</p>}>
        <p>content</p>
      </ErrorBoundary>,
    );
    expect(screen.getByText("content")).toBeInTheDocument();
    expect(screen.queryByText("fallback")).toBeNull();
  });

  it("renders the fallback with the error a child threw", () => {
    render(
      <ErrorBoundary fallback={(error) => <p>caught: {error.message}</p>}>
        <Throws value={new TypeError("data.items is undefined")} />
      </ErrorBoundary>,
    );
    expect(screen.getByText("caught: data.items is undefined")).toBeInTheDocument();
  });

  it("wraps a thrown non-Error value in an Error", () => {
    render(
      <ErrorBoundary fallback={(error) => <p>caught: {error.message}</p>}>
        <Throws value="plain string" />
      </ErrorBoundary>,
    );
    expect(screen.getByText("caught: plain string")).toBeInTheDocument();
  });

  it("reports the caught error once", () => {
    const onError = vi.fn();
    render(
      <ErrorBoundary fallback={() => null} onError={onError}>
        <Throws value={new Error("boom")} />
      </ErrorBoundary>,
    );
    expect(onError).toHaveBeenCalledOnce();
    expect(onError.mock.calls[0][0]).toEqual(new Error("boom"));
    expect(typeof onError.mock.calls[0][1]).toBe("string");
  });

  it("renders the children again after a remount with a new key", () => {
    const { rerender } = render(
      <ErrorBoundary key="a" fallback={() => <p>fallback</p>}>
        <Throws value={new Error("boom")} />
      </ErrorBoundary>,
    );
    expect(screen.getByText("fallback")).toBeInTheDocument();

    rerender(
      <ErrorBoundary key="b" fallback={() => <p>fallback</p>}>
        <p>content</p>
      </ErrorBoundary>,
    );
    expect(screen.getByText("content")).toBeInTheDocument();
  });
});
