// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { describe, expect, it } from "vitest";
import { namedExport } from "./wasmPluginLoader";

const BUNDLE = "torchsnap-gadget://localhost/weather/frontend/dist/launcher.js";

describe("namedExport", () => {
  it("returns the named export as the default component", () => {
    const Forecast = () => null;
    expect(namedExport({ Forecast }, "Forecast", "weather", BUNDLE)).toEqual({
      default: Forecast,
    });
  });

  it("names the gadget, bundle and export when the export is missing", () => {
    expect(() => namedExport({ Forecast: () => null }, "Forcast", "weather", BUNDLE)).toThrow(
      `gadget weather: bundle ${BUNDLE} has no export "Forcast"`,
    );
  });

  it("treats an export set to null as missing", () => {
    expect(() => namedExport({ Forecast: null }, "Forecast", "weather", BUNDLE)).toThrow(
      /has no export "Forecast"/,
    );
  });
});
