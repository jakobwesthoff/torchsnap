// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Keyboard navigation for the launcher.
 *
 * Registers three groups of keybindings via `useKeyBindings`:
 *
 * 1. **Global bindings** — always active regardless of whether a
 *    plugin custom UI is mounted. Tab trap (prevent focus escape)
 *    and Escape (clear query → dismiss). Plugin UIs can override
 *    these at a higher layer.
 *
 * 2. **Navigation bindings** — arrow keys, page up/down, enter.
 *    Disabled when a plugin custom UI is active (`enabled=false`),
 *    since the plugin handles its own navigation.
 *
 * 3. **Dynamic action bindings** — derived from the currently
 *    selected entry's secondary actions. Also disabled when a
 *    plugin is active.
 */

import { useCallback, useMemo, type RefObject } from "react";
import {
  useKeyBindings,
  LAYER,
  type KeyBindingDefinition,
  type ModifierKey,
} from "../../keybindings";
import type { Action } from "../types";
import { PAGE_SIZE } from "../constants";

interface UseKeyboardNavigationParams {
  dismiss: () => void;
  query: string;
  setQuery: (query: string) => void;
  resultCount: number;
  selectedIndex: number;
  setSelectedIndex: (index: number) => void;
  onExecute: (entry?: undefined, actionIndex?: number) => void;
  selectedActions: Action[];
  mouseActiveRef: RefObject<boolean>;
  /** When false, navigation and action bindings are deregistered.
   *  Global bindings (Tab trap, Escape) remain active. */
  enabled: boolean;
}

// All launcher bindings live above LAYER.COMPONENT so they take
// priority over any app-level bindings for the same keys.
const LAUNCHER_LAYER = LAYER.COMPONENT + 1;

export function useKeyboardNavigation({
  dismiss,
  query,
  setQuery,
  resultCount,
  selectedIndex,
  setSelectedIndex,
  onExecute,
  selectedActions,
  mouseActiveRef,
  enabled,
}: UseKeyboardNavigationParams) {
  const moveSelection = useCallback(
    (delta: number) => {
      mouseActiveRef.current = false;
      setSelectedIndex(
        Math.max(0, Math.min(selectedIndex + delta, resultCount - 1)),
      );
    },
    [selectedIndex, resultCount, setSelectedIndex, mouseActiveRef],
  );

  // =========================================================
  // Global Bindings (always active)
  //
  // Tab trap prevents browser focus navigation from moving
  // focus out of the search input. Escape clears the query
  // first, dismisses the launcher when already empty. Plugin
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
          if (query) {
            setQuery("");
          } else {
            dismiss();
          }
        },
        keybindings: [
          { combo: { modifiers: [], key: "Escape" }, allowInInput: true },
        ],
      },
    ],
    [query, setQuery, dismiss],
  );

  useKeyBindings(globalBindings);

  // =========================================================
  // Navigation Bindings (disabled when plugin UI is active)
  //
  // Arrow keys, page up/down, and Enter for primary action.
  // =========================================================

  const navigationBindings: KeyBindingDefinition[] = useMemo(() => {
    if (!enabled) return [];

    return [
      {
        id: "launcher-arrow-down",
        layer: LAUNCHER_LAYER,
        order: 0,
        handler: () => moveSelection(1),
        keybindings: [
          { combo: { modifiers: [], key: "ArrowDown" }, allowInInput: true },
        ],
      },
      {
        id: "launcher-arrow-up",
        layer: LAUNCHER_LAYER,
        order: 1,
        handler: () => moveSelection(-1),
        keybindings: [
          { combo: { modifiers: [], key: "ArrowUp" }, allowInInput: true },
        ],
      },
      {
        id: "launcher-page-down",
        layer: LAUNCHER_LAYER,
        order: 2,
        handler: () => moveSelection(PAGE_SIZE),
        keybindings: [
          { combo: { modifiers: [], key: "PageDown" }, allowInInput: true },
        ],
      },
      {
        id: "launcher-page-up",
        layer: LAUNCHER_LAYER,
        order: 3,
        handler: () => moveSelection(-PAGE_SIZE),
        keybindings: [
          { combo: { modifiers: [], key: "PageUp" }, allowInInput: true },
        ],
      },
      {
        id: "launcher-enter",
        layer: LAUNCHER_LAYER,
        order: 4,
        handler: () => onExecute(),
        keybindings: [
          { combo: { modifiers: [], key: "Enter" }, allowInInput: true },
        ],
      },
    ];
  }, [enabled, moveSelection, onExecute]);

  useKeyBindings(navigationBindings);

  // =========================================================
  // Dynamic Action Bindings (disabled when plugin UI is active)
  //
  // Derived from the currently selected entry's secondary
  // actions. When the selection changes, the definitions change
  // structurally and `useKeyBindings` re-registers them.
  // =========================================================

  const actionBindings: KeyBindingDefinition[] = useMemo(() => {
    if (!enabled) return [];

    const bindings: KeyBindingDefinition[] = [];

    for (let i = 1; i < selectedActions.length; i++) {
      const action = selectedActions[i];
      if (!action.keybinding) continue;

      bindings.push({
        id: `launcher-action-${i}`,
        layer: LAUNCHER_LAYER,
        order: 5 + i,
        handler: () => onExecute(undefined, i),
        keybindings: [
          {
            combo: {
              modifiers: (action.keybinding.modifiers ?? []) as ModifierKey[],
              key: action.keybinding.key,
            },
            allowInInput: true,
          },
        ],
      });
    }

    return bindings;
  }, [enabled, selectedActions, onExecute]);

  useKeyBindings(actionBindings);
}
