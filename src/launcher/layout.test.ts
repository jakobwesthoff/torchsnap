// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { describe, expect, it } from "vitest";
import mascotData from "../derived/mascots.json";
import { CARD_SHADOW, CARD_TOP_OFFSET, launcherWindowLayout } from "./layout";
import { placeMascot, type MascotAnchor } from "./placement";

const mascots = mascotData as Record<string, { groundAnchor: MascotAnchor; boxLeft: number }>;

// Rounding slack for `CARD_TOP_OFFSET + top`, which sums fractional
// constants; a real overlap is at least a sizeable part of a pixel.
const EPSILON = 1e-9;

describe("launcherWindowLayout", () => {
  it("sizes the window for the largest card", () => {
    expect(launcherWindowLayout(680, 550)).toEqual({
      windowWidth: 808,
      windowHeight: expect.closeTo(795.9, 10),
      cardTopOffset: expect.closeTo(181.9, 10),
    });
  });

  it("follows the card's height", () => {
    expect(launcherWindowLayout(680, 61)).toEqual({
      windowWidth: 808,
      windowHeight: expect.closeTo(306.9, 10),
      cardTopOffset: expect.closeTo(181.9, 10),
    });
  });
});

// =========================================================
// Room above the card
//
// Everything drawn above the card must fit between the window's top
// edge and the card, `CARD_TOP_OFFSET` below it; the window clips
// whatever reaches further.
// =========================================================

describe("room above the card", () => {
  it("holds every mascot's image in both modes", () => {
    for (const [variant, { groundAnchor, boxLeft }] of Object.entries(mascots)) {
      for (const mode of ["center", "sidekick"] as const) {
        const imageTop = CARD_TOP_OFFSET + placeMascot(mode, groundAnchor, boxLeft).top;
        expect(imageTop, `${variant} (${mode})`).toBeGreaterThanOrEqual(-EPSILON);
      }
    }
  });

  it("holds an image anchored on its bottom edge, the highest a mascot can reach", () => {
    const { top } = placeMascot("center", { x: 0.5, y: 1 }, 0);
    expect(CARD_TOP_OFFSET + top).toBeGreaterThanOrEqual(0);
  });

  // A blur radius B is a Gaussian with a standard deviation of B / 2
  // (CSS Backgrounds), whose tail ends three deviations, 1.5 * B,
  // past the shadow's edge. The y offset moves that edge down and the
  // spread moves it out.
  it("holds every layer of the card's shadow", () => {
    const outerLayers = CARD_SHADOW.filter((layer) => !layer.startsWith("inset"));
    const blurred = outerLayers.filter((layer) => {
      // The lengths come first (`x y [blur [spread]]`), the colour last.
      const lengths: number[] = [];
      for (const token of layer.split(" ")) {
        const length = /^(-?\d+(?:\.\d+)?)(?:px)?$/.exec(token);
        if (!length) break;
        lengths.push(Number(length[1]));
      }
      const [, y, blur = 0, spread = 0] = lengths;
      expect(1.5 * blur + spread - y, layer).toBeLessThanOrEqual(CARD_TOP_OFFSET);
      return blur > 0;
    });
    // Guards the parsing: the card has two blurred layers, so a format
    // the pattern misreads cannot pass with nothing checked.
    expect(blurred).toHaveLength(2);
  });
});
