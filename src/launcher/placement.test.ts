// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { describe, expect, it } from "vitest";
import {
  CARD_WIDTH,
  CENTER_FEET_Y,
  MASCOT_SIZE,
  SIDEKICK_ANCHOR_INSET,
  SIDEKICK_FEET_Y,
} from "./layout";
import { placeMascot } from "./placement";

// The plain Snappy's anchor and visible left edge.
const ORIGINAL = { groundAnchor: { x: 0.5131, y: 0.8438 }, boxLeft: 0.1768 };

describe("placeMascot in centre mode", () => {
  it("puts the anchor's y on the feet line", () => {
    const { top } = placeMascot("center", ORIGINAL.groundAnchor, ORIGINAL.boxLeft);
    expect(top + MASCOT_SIZE.center * ORIGINAL.groundAnchor.y).toBeCloseTo(CENTER_FEET_Y, 10);
  });

  it("puts the anchor's x on the card centre", () => {
    const { left } = placeMascot("center", ORIGINAL.groundAnchor, ORIGINAL.boxLeft);
    expect(left! + MASCOT_SIZE.center * ORIGINAL.groundAnchor.x).toBeCloseTo(CARD_WIDTH / 2, 10);
  });

  it("gives the original the values the review renders used", () => {
    expect(placeMascot("center", ORIGINAL.groundAnchor, ORIGINAL.boxLeft)).toEqual({
      top: expect.closeTo(10.1 - 192 * 0.8438, 10),
      left: expect.closeTo(340 - 192 * 0.5131, 10),
    });
  });

  it("does not depend on the visible box", () => {
    expect(placeMascot("center", ORIGINAL.groundAnchor, 0)).toEqual(
      placeMascot("center", ORIGINAL.groundAnchor, 0.4),
    );
  });

  it("sets neither right nor the pill offset", () => {
    const placement = placeMascot("center", ORIGINAL.groundAnchor, ORIGINAL.boxLeft);
    expect(placement.right).toBeUndefined();
    expect(placement.anchorFromRight).toBeUndefined();
  });
});

describe("placeMascot in sidekick mode", () => {
  it("puts the anchor's y on the sidekick feet line", () => {
    const { top } = placeMascot("sidekick", ORIGINAL.groundAnchor, ORIGINAL.boxLeft);
    expect(top + MASCOT_SIZE.sidekick * ORIGINAL.groundAnchor.y).toBeCloseTo(SIDEKICK_FEET_Y, 10);
  });

  it("puts the anchor at the fixed inset from the card's right edge", () => {
    const placement = placeMascot("sidekick", ORIGINAL.groundAnchor, ORIGINAL.boxLeft);
    expect(placement.right).toBeCloseTo(SIDEKICK_ANCHOR_INSET - 96 * 0.5131, 10);
    expect(placement.anchorFromRight).toBeCloseTo(SIDEKICK_ANCHOR_INSET, 10);
    expect(placement.left).toBeUndefined();
  });

  it("moves a wide figure left until its mirrored edge sits on the card edge", () => {
    // Anchor far right in the image, figure reaching almost to the left
    // image edge: at the inset its mirrored edge would pass the card edge.
    const anchor = { x: 0.6, y: 0.9 };
    const placement = placeMascot("sidekick", anchor, 0.05);
    expect(placement.right).toBeCloseTo(-96 * 0.05, 10);
    // The visible edge, size * boxLeft from the image's right edge on
    // screen, lands exactly on the card edge.
    expect(placement.right! + 96 * 0.05).toBeCloseTo(0, 10);
    // The pill follows the anchor, which moved away from the inset.
    expect(placement.anchorFromRight).toBeCloseTo(-96 * 0.05 + 96 * 0.6, 10);
    expect(placement.anchorFromRight!).toBeGreaterThan(SIDEKICK_ANCHOR_INSET);
  });

  it("does not move a figure whose mirrored edge stays inside the card", () => {
    const anchor = { x: 0.5, y: 0.9 };
    const placement = placeMascot("sidekick", anchor, 0.3);
    expect(placement.right).toBeCloseTo(SIDEKICK_ANCHOR_INSET - 48, 10);
  });

  it("treats a figure whose edge lands exactly on the card edge as inside", () => {
    // 48 - 96x == -96 * boxLeft for x = 0.75, boxLeft = 0.25.
    const placement = placeMascot("sidekick", { x: 0.75, y: 0.9 }, 0.25);
    expect(placement.right).toBeCloseTo(-24, 10);
    expect(placement.anchorFromRight).toBeCloseTo(SIDEKICK_ANCHOR_INSET, 10);
  });
});
