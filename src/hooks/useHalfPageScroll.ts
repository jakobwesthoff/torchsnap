// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * useHalfPageScroll — Vim-style Ctrl-U / Ctrl-D scrolling for any container.
 *
 * Registers keybindings via the app's keybinding system so that Ctrl-D
 * scrolls the referenced element down by half its visible height, and
 * Ctrl-U scrolls up by the same amount. Uses absolute `scrollTo` with
 * clamped targets so rapid repeated presses converge to the correct
 * position rather than stacking relative deltas during smooth-scroll
 * animation.
 *
 * The scroll distance is computed from `clientHeight` at invocation time,
 * so layout changes are automatically respected.
 */

import { useMemo, type RefObject } from "react";
import { useKeyBindings } from "../keybindings/useKeyBindings";
import { type KeyBindingDefinition } from "../keybindings/matching";

interface UseHalfPageScrollOptions {
  /** Ref to the scrollable container element. */
  ref: RefObject<HTMLElement | null>;
  /** Keybinding layer for registration. */
  layer: number;
  /**
   * Base order within the layer. Two bindings are registered at
   * `order` and `order + 1`. Default: 100.
   */
  order?: number;
  /** Whether the bindings should fire while an input is focused. Default: true. */
  allowInInput?: boolean;
}

export function useHalfPageScroll({
  ref,
  layer,
  order = 100,
  allowInInput = true,
}: UseHalfPageScrollOptions): void {
  const bindings: KeyBindingDefinition[] = useMemo(
    () => [
      {
        id: "half-page-scroll-down",
        layer,
        order,
        handler: () => {
          const el = ref.current;
          if (!el) return;
          const half = el.clientHeight / 2;
          const maxScroll = el.scrollHeight - el.clientHeight;
          const target = Math.min(el.scrollTop + half, maxScroll);
          el.scrollTo({ top: target, behavior: "smooth" });
        },
        keybindings: [
          { combo: { modifiers: ["Ctrl"], key: "d" }, allowInInput },
        ],
      },
      {
        id: "half-page-scroll-up",
        layer,
        order: order + 1,
        handler: () => {
          const el = ref.current;
          if (!el) return;
          const half = el.clientHeight / 2;
          const target = Math.max(el.scrollTop - half, 0);
          el.scrollTo({ top: target, behavior: "smooth" });
        },
        keybindings: [
          { combo: { modifiers: ["Ctrl"], key: "u" }, allowInInput },
        ],
      },
    ],
    [ref, layer, order, allowInInput],
  );

  useKeyBindings(bindings);
}
