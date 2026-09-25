// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { render, screen } from "@testing-library/react";
import { beforeAll, describe, expect, it } from "vitest";
import {
  MockGadgetContextProvider,
  setupSdkGlobalsForTesting,
} from "@torchsnap/gadget-sdk/testing";
import { CalculatorInline } from "./CalculatorInline";

beforeAll(() => setupSdkGlobalsForTesting());

const RESULT = { expression: "2+2", result: "4", resultType: "number" };

describe("CalculatorInline", () => {
  it("shows the result and the expression it came from", () => {
    render(
      <MockGadgetContextProvider launcher={{}}>
        <CalculatorInline data={RESULT} query="2+2" matchedPrefix="" selected={false} />
      </MockGadgetContextProvider>,
    );

    expect(screen.getByText("4")).toBeInTheDocument();
    expect(screen.getByText("2+2")).toBeInTheDocument();
  });

  it("renders nothing without a result", () => {
    const { container } = render(
      <MockGadgetContextProvider launcher={{}}>
        <CalculatorInline data={undefined} query="" matchedPrefix="" selected={false} />
      </MockGadgetContextProvider>,
    );

    expect(container).toBeEmptyDOMElement();
  });
});
