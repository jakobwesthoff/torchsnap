// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Keyboard navigation for the launcher.
 *
 * Registers three groups of keybindings via `useKeyBindings`:
 *
 * 1. **Global bindings** — always active regardless of whether a
 *    gadget custom UI is mounted. Tab trap (prevent focus escape)
 *    and Escape (clear query → dismiss). Gadget UIs can override
 *    these at a higher layer.
 *
 * 2. **Navigation bindings** — arrow keys, page up/down, enter.
 *    Disabled when a gadget custom UI is active (`enabled=false`),
 *    since the gadget handles its own navigation.
 *
 * 3. **Dynamic action bindings**: the keys of the selected
 *    entry's filled slots other than `primary` (see
 *    `actionSlots.ts`). Also disabled when a gadget is active.
 *
 * All handler state is read from a ref that is updated after each
 * commit via `useEffect`. This keeps the `useMemo` arrays stable
 * when only handler-relevant values change (selectedIndex, query,
 * etc.), avoiding unnecessary keybinding re-registration. The
 * `useKeyBindings` hook separately wraps handlers in refs, so the
 * dispatch path always reaches the latest closure.
 */

import { useEffect, useMemo, useRef } from "react";
import { useKeyBindings, LAYER, type KeyBindingDefinition } from "../../keybindings";
import type { Action, ActionSlot } from "../../types";
import { slotBindings } from "../actionSlots";
import { PAGE_SIZE } from "../constants";

interface UseKeyboardNavigationParams {
  dismiss: () => void;
  query: string;
  setQuery: (query: string) => void;
  resultCount: number;
  selectedIndex: number;
  setSelectedIndex: (index: number) => void;
  /** Run the action in `slot` of the selected entry. */
  onExecute: (slot: ActionSlot) => void;
  selectedActions: Action[];
  mouseActiveRef: React.RefObject<boolean>;
  /** When false, navigation and action bindings are deregistered.
   *  Global bindings (Tab trap, Escape) remain active. */
  enabled: boolean;
}

// All launcher bindings live above LAYER.COMPONENT so they take
// priority over any app-level bindings for the same keys.
const LAUNCHER_LAYER = LAYER.COMPONENT + 1;

export function useKeyboardNavigation(params: UseKeyboardNavigationParams) {
  const { enabled, selectedActions } = params;

  // Ref holding the latest params, updated after each commit.
  // Handlers read from this instead of closing over individual
  // values, so the useMemo arrays below stay referentially stable
  // when only handler-relevant state changes.
  const stateRef = useRef(params);
  useEffect(() => {
    stateRef.current = params;
  });

  // =========================================================
  // Global Bindings (always active)
  //
  // Tab trap prevents browser focus navigation from moving
  // focus out of the search input. Escape clears the query
  // first, dismisses the launcher when already empty. Gadget
  // UIs can override these at a higher keybinding layer.
  // =========================================================

  const globalBindings: KeyBindingDefinition[] = useMemo(
    () => [
      {
        id: "launcher-tab-trap",
        layer: LAUNCHER_LAYER,
        order: 9,
        handler: () => {},
        keybindings: [
          { combo: { modifiers: [], key: "Tab" }, allowInInput: true },
          { combo: { modifiers: ["Shift"], key: "Tab" }, allowInInput: true },
        ],
      },
      {
        id: "launcher-escape",
        layer: LAUNCHER_LAYER,
        order: 10,
        handler: () => {
          const { query, setQuery, dismiss } = stateRef.current;
          if (query) {
            setQuery("");
          } else {
            dismiss();
          }
        },
        keybindings: [{ combo: { modifiers: [], key: "Escape" }, allowInInput: true }],
      },
    ],
    [],
  );

  useKeyBindings(globalBindings);

  // =========================================================
  // Navigation Bindings (disabled when gadget UI is active)
  //
  // Arrow keys, page up/down, and Enter for the primary slot.
  // =========================================================

  const navigationBindings: KeyBindingDefinition[] = useMemo(() => {
    if (!enabled) return [];

    const moveSelection = (delta: number) => {
      const { selectedIndex, resultCount, setSelectedIndex, mouseActiveRef } = stateRef.current;
      mouseActiveRef.current = false;
      setSelectedIndex(Math.max(0, Math.min(selectedIndex + delta, resultCount - 1)));
    };

    return [
      {
        id: "launcher-arrow-down",
        layer: LAUNCHER_LAYER,
        order: 0,
        handler: () => moveSelection(1),
        keybindings: [{ combo: { modifiers: [], key: "ArrowDown" }, allowInInput: true }],
      },
      {
        id: "launcher-arrow-up",
        layer: LAUNCHER_LAYER,
        order: 1,
        handler: () => moveSelection(-1),
        keybindings: [{ combo: { modifiers: [], key: "ArrowUp" }, allowInInput: true }],
      },
      {
        id: "launcher-page-down",
        layer: LAUNCHER_LAYER,
        order: 2,
        handler: () => moveSelection(PAGE_SIZE),
        keybindings: [{ combo: { modifiers: [], key: "PageDown" }, allowInInput: true }],
      },
      {
        id: "launcher-page-up",
        layer: LAUNCHER_LAYER,
        order: 3,
        handler: () => moveSelection(-PAGE_SIZE),
        keybindings: [{ combo: { modifiers: [], key: "PageUp" }, allowInInput: true }],
      },
      {
        id: "launcher-enter",
        layer: LAUNCHER_LAYER,
        order: 4,
        handler: () => stateRef.current.onExecute("primary"),
        keybindings: [{ combo: { modifiers: [], key: "Enter" }, allowInInput: true }],
      },
    ];
  }, [enabled]);

  useKeyBindings(navigationBindings);

  // =========================================================
  // Dynamic Action Bindings (disabled when gadget UI is active)
  //
  // Derived from the currently selected entry's filled slots.
  // When the selection changes, the definitions change
  // structurally and `useKeyBindings` re-registers them.
  // =========================================================

  const actionBindings: KeyBindingDefinition[] = useMemo(() => {
    if (!enabled) return [];

    return slotBindings(selectedActions).map(({ slot, combo }, i) => ({
      id: `launcher-action-${slot}`,
      layer: LAUNCHER_LAYER,
      order: 5 + i,
      handler: () => stateRef.current.onExecute(slot),
      keybindings: [{ combo, allowInInput: true }],
    }));
  }, [enabled, selectedActions]);

  useKeyBindings(actionBindings);
}
