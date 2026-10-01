// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { SnappyEmblem } from "./SnappyEmblem";

// The emblem's drawing box in its 1024 viewBox.
const BOX = { x: 18, y: 55, width: 988, height: 892 };

function frameOf(element: Element) {
  const [x, y, width, height] = (element.getAttribute("viewBox") ?? "").split(" ").map(Number);
  return { x, y, width, height };
}

// Applies a `feColorMatrix` to an sRGB colour as the webview would, with
// each channel clamped to 0..1.
function applyMatrix(matrix: Element, [r, g, b]: [number, number, number]) {
  const v = (matrix.getAttribute("values") ?? "").trim().split(/\s+/).map(Number);
  const row = (i: number) =>
    Math.min(Math.max(v[i] * r + v[i + 1] * g + v[i + 2] * b + v[i + 3] + v[i + 4], 0), 1);
  return { r: row(0), g: row(5), b: row(10), a: row(15) };
}

// The `feColorMatrix` of the filter an `<image>` refers to.
function matrixOf(container: HTMLElement, image: Element) {
  const id = /^url\(#(.+)\)$/.exec(image.getAttribute("filter") ?? "")?.[1];
  return container.querySelector(`filter[id="${id}"] feColorMatrix`)!;
}

describe("SnappyEmblem", () => {
  it("draws the emblem file", () => {
    const { container } = render(<SnappyEmblem width={96} />);
    const image = container.querySelector("image");
    expect(image).toHaveAttribute("href", expect.stringContaining("snappy-emblem-feathered.svg"));
    expect(image).toHaveAttribute("width", "1024");
    expect(image).toHaveAttribute("height", "1024");
  });

  it("takes its height from the frame's aspect ratio", () => {
    render(<SnappyEmblem width={96} />);
    const svg = screen.getByRole("img");
    const frame = frameOf(svg);
    expect(svg).toHaveAttribute("width", "96");
    expect(Number(svg.getAttribute("height"))).toBeCloseTo((96 * frame.height) / frame.width, 5);
  });

  describe("rising framing", () => {
    it("is the default", () => {
      render(<SnappyEmblem width={96} />);
      const frame = frameOf(screen.getByRole("img"));
      expect(frame.y + frame.height).toBeCloseTo(BOX.y + 0.74 * BOX.height, 5);
    });

    it("gives the owl the app icon's share of the width, centred", () => {
      render(<SnappyEmblem width={96} framing="rising" />);
      const frame = frameOf(screen.getByRole("img"));
      expect(BOX.width / frame.width).toBeCloseTo(780 / 824, 5);
      expect(frame.x + frame.width / 2).toBeCloseTo(BOX.x + BOX.width / 2, 5);
    });

    it("cuts the owl below the beak, as the app icon does", () => {
      render(<SnappyEmblem width={96} framing="rising" />);
      const frame = frameOf(screen.getByRole("img"));
      expect(frame.y + frame.height).toBeCloseTo(BOX.y + 0.74 * BOX.height, 5);
    });

    it("leaves a little room above the ears", () => {
      render(<SnappyEmblem width={96} framing="rising" />);
      const frame = frameOf(screen.getByRole("img"));
      expect(BOX.y - frame.y).toBeCloseTo(0.12 * BOX.height, 5);
    });
  });

  describe("full framing", () => {
    it("shows the whole owl with the rising framing's side margin all round", () => {
      render(<SnappyEmblem width={96} framing="full" />);
      const frame = frameOf(screen.getByRole("img"));
      const margin = (BOX.width / (780 / 824) - BOX.width) / 2;
      expect(frame.x).toBeCloseTo(BOX.x - margin, 5);
      expect(frame.y).toBeCloseTo(BOX.y - margin, 5);
      expect(frame.width).toBeCloseTo(BOX.width + 2 * margin, 5);
      expect(frame.height).toBeCloseTo(BOX.height + 2 * margin, 5);
    });
  });

  describe("colour tone", () => {
    it("is the default and draws the emblem unfiltered", () => {
      const { container } = render(<SnappyEmblem width={96} />);
      const images = container.querySelectorAll("image");
      expect(images).toHaveLength(1);
      expect(images[0]).not.toHaveAttribute("filter");
    });

    it("gets a light rim in dark mode", () => {
      render(<SnappyEmblem width={96} tone="colour" />);
      expect(screen.getByRole("img")).toHaveClass(
        "dark:drop-shadow-[0_0_3px_rgb(255_255_255/0.5)]",
      );
    });

    it("leaves the one-ink tones without a rim", () => {
      render(<SnappyEmblem width={96} tone="ink" />);
      expect(screen.getByRole("img").getAttribute("class") ?? "").not.toContain("drop-shadow");
    });
  });

  describe("ink tone", () => {
    it("draws navy ink on light backgrounds and the negative in dark mode", () => {
      const { container } = render(<SnappyEmblem width={96} tone="ink" />);
      const [light, dark] = container.querySelectorAll("image");
      expect(light).toHaveClass("dark:hidden");
      expect(dark).toHaveClass("hidden", "dark:inline");
      expect(applyMatrix(matrixOf(container, light), [0, 0, 0])).toMatchObject({
        r: 23 / 255,
        g: 34 / 255,
        b: 59 / 255,
      });
      expect(applyMatrix(matrixOf(container, dark), [1, 1, 1])).toMatchObject({
        r: 243 / 255,
        g: 239 / 255,
        b: 226 / 255,
      });
    });

    it("prints dark tones solid and the cream face as paper", () => {
      const { container } = render(<SnappyEmblem width={96} tone="ink" />);
      const matrix = matrixOf(container, container.querySelectorAll("image")[0]);
      expect(applyMatrix(matrix, [0, 0, 0]).a).toBe(1);
      expect(applyMatrix(matrix, [0.5, 0.5, 0.5]).a).toBeCloseTo((1 - 0.5 - 0.06) / 0.7, 5);
      expect(applyMatrix(matrix, [1, 1, 1]).a).toBe(0);
    });

    it("weighs the channels by their luminance", () => {
      const { container } = render(<SnappyEmblem width={96} tone="ink" />);
      const matrix = matrixOf(container, container.querySelectorAll("image")[0]);
      expect(applyMatrix(matrix, [0, 1, 0]).a).toBeCloseTo(clamp((1 - 0.7152 - 0.06) / 0.7), 5);
    });

    it("keeps the background transparent", () => {
      const { container } = render(<SnappyEmblem width={96} tone="ink" />);
      for (const image of container.querySelectorAll("image")) {
        const composite = container.querySelector(`${filterSelector(image)} feComposite`);
        expect(composite).toHaveAttribute("operator", "in");
        expect(composite).toHaveAttribute("in2", "SourceGraphic");
      }
    });

    it("computes in sRGB like the icon tool", () => {
      const { container } = render(<SnappyEmblem width={96} tone="ink" />);
      for (const filter of container.querySelectorAll("filter")) {
        expect(filter).toHaveAttribute("color-interpolation-filters", "sRGB");
      }
    });
  });

  describe("negative tone", () => {
    it("draws only the pale negative, also on light backgrounds", () => {
      const { container } = render(<SnappyEmblem width={96} tone="negative" />);
      const images = container.querySelectorAll("image");
      expect(images).toHaveLength(1);
      expect(images[0]).not.toHaveClass("hidden");
    });

    it("draws the light tones in pale ink and leaves the dark ones out", () => {
      const { container } = render(<SnappyEmblem width={96} tone="negative" />);
      const matrix = matrixOf(container, container.querySelector("image")!);
      expect(applyMatrix(matrix, [1, 1, 1]).a).toBe(1);
      expect(applyMatrix(matrix, [0.5, 0.5, 0.5]).a).toBeCloseTo((0.5 - 0.2) / 0.65, 5);
      expect(applyMatrix(matrix, [0, 0, 0]).a).toBe(0);
    });
  });

  it("gives every instance its own filter ids", () => {
    const { container } = render(
      <>
        <SnappyEmblem width={96} tone="ink" />
        <SnappyEmblem width={96} tone="ink" />
      </>,
    );
    const ids = [...container.querySelectorAll("filter")].map((filter) => filter.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("is named by its alt text", () => {
    render(<SnappyEmblem width={96} alt="Open torchsnap.app" />);
    expect(screen.getByRole("img", { name: "Open torchsnap.app" })).toBeInTheDocument();
  });

  it("is hidden from assistive technology with an empty alt text", () => {
    const { container } = render(<SnappyEmblem width={96} alt="" />);
    expect(screen.queryByRole("img")).not.toBeInTheDocument();
    expect(container.querySelector("svg")).toHaveAttribute("aria-hidden", "true");
  });

  it("passes classes through", () => {
    render(<SnappyEmblem width={96} className="shrink-0" />);
    expect(screen.getByRole("img")).toHaveClass("shrink-0");
  });
});

function clamp(value: number) {
  return Math.min(Math.max(value, 0), 1);
}

function filterSelector(image: Element) {
  const id = /^url\(#(.+)\)$/.exec(image.getAttribute("filter") ?? "")?.[1];
  return `filter[id="${id}"]`;
}
