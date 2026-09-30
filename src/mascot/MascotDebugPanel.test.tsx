// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { fireEvent, render } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { MascotDebugPanel } from "./MascotDebugPanel";
import { ALL_YEAR, type MascotDraw } from "./selection";

/** A full-moon night at Christmas: FullMoon 90%, Christmas 4%, all-year 6%. */
function christmasOnAFullMoonNight(overrides: Partial<MascotDraw> = {}): MascotDraw {
  return {
    variant: "christmas-thief-santa-sack",
    character: "christmas-thief-green-fur",
    pool: "Christmas",
    conditionsHeld: ["fullMoonNight", "christmas"],
    shares: [
      { pool: "FullMoon", share: 0.9 },
      { pool: "Christmas", share: 0.4 * 0.1 },
      { pool: ALL_YEAR, share: 0.6 * 0.1 },
    ],
    excluded: 1,
    remembered: 20,
    now: new Date(2026, 11, 24, 22, 14),
    ...overrides,
  };
}

/** The panel's rows as "label | value" lines. */
function rowsOf(container: HTMLElement): string[] {
  return [...container.querySelectorAll("tr")].map((row) =>
    [...row.querySelectorAll("td")].map((cell) => cell.textContent).join(" | "),
  );
}

describe("MascotDebugPanel", () => {
  it("shows every part of a draw from an occasion below the first", () => {
    const { container } = render(
      <MascotDebugPanel draw={christmasOnAFullMoonNight()} onClose={() => {}} />,
    );
    expect(rowsOf(container)).toEqual([
      "drawn | 2026-12-24 22:14",
      "pool | Christmas: 40% of the 10% left = 4%",
      "character | christmas-thief-green-fur (variant christmas-thief-santa-sack)",
      "active | FullMoon 90%   full moon within 1.5 days, dusk to dawn",
      " | Christmas 4%   December 20 to 26",
      " | all-year 6%",
      "recent | 20 remembered, 1 excluded from this pool",
    ]);
  });

  it("gives the first occasion its share alone", () => {
    const { container } = render(
      <MascotDebugPanel
        draw={christmasOnAFullMoonNight({
          pool: "FullMoon",
          variant: "werewolf",
          character: "werewolf",
        })}
        onClose={() => {}}
      />,
    );
    expect(rowsOf(container)).toContain("pool | FullMoon: 90%");
  });

  it("gives the all-year pool what is left", () => {
    const { container } = render(
      <MascotDebugPanel draw={christmasOnAFullMoonNight({ pool: ALL_YEAR })} onClose={() => {}} />,
    );
    expect(rowsOf(container)).toContain("pool | all-year: 6%");
  });

  it("marks an occasion whose condition held without an eligible character", () => {
    const draw = christmasOnAFullMoonNight({
      pool: ALL_YEAR,
      shares: [{ pool: ALL_YEAR, share: 1 }],
      conditionsHeld: ["fullMoonNight"],
    });
    const { container } = render(<MascotDebugPanel draw={draw} onClose={() => {}} />);
    expect(rowsOf(container)).toEqual(
      expect.arrayContaining([
        "active | FullMoon no eligible character   full moon within 1.5 days, dusk to dawn",
        " | all-year 100%",
      ]),
    );
  });

  it("lists only the all-year pool on an ordinary day", () => {
    const draw = christmasOnAFullMoonNight({
      pool: ALL_YEAR,
      shares: [{ pool: ALL_YEAR, share: 1 }],
      conditionsHeld: [],
      excluded: 0,
      remembered: 3,
    });
    const { container } = render(<MascotDebugPanel draw={draw} onClose={() => {}} />);
    expect(rowsOf(container)).toEqual(
      expect.arrayContaining([
        "pool | all-year: 100%",
        "active | all-year 100%",
        "recent | 3 remembered, 0 excluded from this pool",
      ]),
    );
  });

  it("says so while random mascots are off", () => {
    const { container } = render(<MascotDebugPanel draw={null} onClose={() => {}} />);
    expect(rowsOf(container)).toEqual(["drawn | nothing: random mascots are off"]);
  });

  it("closes on a click", () => {
    const onClose = vi.fn();
    const { getByTestId } = render(
      <MascotDebugPanel draw={christmasOnAFullMoonNight()} onClose={onClose} />,
    );
    fireEvent.click(getByTestId("mascot-debug-panel"));
    expect(onClose).toHaveBeenCalledOnce();
  });
});
