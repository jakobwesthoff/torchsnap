// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { describe, expect, it } from "vitest";
import { appIconImage } from "./appIconImage";

const LADDER = [
  { url: "/icon-32.webp", width: 32 },
  { url: "/icon-64.webp", width: 64 },
  { url: "/icon-128.webp", width: 128 },
];

describe("appIconImage", () => {
  it("offers every source with its width so the webview picks by density", () => {
    expect(appIconImage(48, LADDER).srcSet).toBe(
      "/icon-32.webp 32w, /icon-64.webp 64w, /icon-128.webp 128w",
    );
  });

  it("tells the webview the CSS size the icon is drawn at", () => {
    expect(appIconImage(48, LADDER).sizes).toBe("48px");
  });

  it("falls back to the smallest source at least as wide as the icon", () => {
    expect(appIconImage(48, LADDER).src).toBe("/icon-64.webp");
  });

  it("falls back to an exact match when there is one", () => {
    expect(appIconImage(64, LADDER).src).toBe("/icon-64.webp");
  });

  it("falls back to the largest source when all are narrower than the icon", () => {
    expect(appIconImage(512, LADDER).src).toBe("/icon-128.webp");
  });

  it("falls back to the smallest source for an icon narrower than all", () => {
    expect(appIconImage(8, LADDER).src).toBe("/icon-32.webp");
  });

  it("has no source while the ladder is not generated", () => {
    expect(appIconImage(48, [])).toEqual({ src: undefined, srcSet: "", sizes: "48px" });
  });
});
