// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { preloadMascotImage, preloadMascotVariant, preloadMascotVariants } from "./preloadMascot";

// Records every image the preloader creates instead of fetching it.
const created: { src: string }[] = [];

class RecordingImage {
  src = "";
  onload: (() => void) | null = null;
  onerror: (() => void) | null = null;
  constructor() {
    created.push(this);
  }
}

const base = `${import.meta.env.BASE_URL}images/mascot/snappy`;

beforeEach(() => {
  created.length = 0;
  vi.stubGlobal("Image", RecordingImage);
});

afterEach(() => {
  vi.unstubAllGlobals();
});

function setPixelRatio(ratio: number) {
  vi.stubGlobal("devicePixelRatio", ratio);
}

describe("preloadMascotImage", () => {
  it("fetches the 1x file on a standard display", () => {
    setPixelRatio(1);
    preloadMascotImage("robot", 192);
    expect(created.map((img) => img.src)).toEqual([`${base}-robot-192.webp`]);
  });

  it("fetches only the 2x file on a retina display", () => {
    setPixelRatio(2);
    preloadMascotImage("robot", 192);
    expect(created.map((img) => img.src)).toEqual([`${base}-robot-384.webp`]);
  });
});

describe("preloadMascotVariant", () => {
  it("fetches one variant at every size", () => {
    setPixelRatio(1);
    preloadMascotVariant("robot", [96, 192]);
    expect(created.map((img) => img.src)).toEqual([
      `${base}-robot-96.webp`,
      `${base}-robot-192.webp`,
    ]);
  });
});

describe("preloadMascotVariants", () => {
  it("fetches every variant at one size", () => {
    setPixelRatio(1);
    preloadMascotVariants(["robot", "ghost"], 96);
    expect(created.map((img) => img.src)).toEqual([
      `${base}-robot-96.webp`,
      `${base}-ghost-96.webp`,
    ]);
  });
});
