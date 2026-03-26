// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Keyboard navigation for the launcher.
 *
 * Registers two groups of keybindings via `useKeyBindings`:
 *
 * 1. **Static bindings** — arrow keys, page up/down, enter (primary
 *    action), and escape. These never change.
 *
 * 2. **Dynamic action bindings** — derived from the currently selected
 *    entry's secondary actions. When the selection changes, the action
 *    keybindings are structurally different, so `useKeyBindings`
 *    automatically re-registers them. This is cheap because the
 *    keybinding engine uses refs internally — no React re-renders
 *    are triggered.
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
  /** When false, all bindings are deregistered. Used when a plugin
   *  custom UI takes over and handles its own keybindings. */
  enabled: boolean;
}

// All launcher navigation bindings live above LAYER.COMPONENT so they
// take priority over any app-level bindings for the same keys.
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
  // Static Bindings
  //
  // Navigation and primary action — these never change.
  // =========================================================

  const staticBindings: KeyBindingDefinition[] = useMemo(
    () =>
      !enabled
        ? []
        : [
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
      {
        id: "launcher-escape",
        layer: LAUNCHER_LAYER,
        order: 10,
        handler: () => {
          // Clear the query first; only dismiss when already empty.
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
    [enabled, moveSelection, onExecute, query, setQuery, dismiss],
  );

  useKeyBindings(staticBindings);

  // =========================================================
  // Dynamic Action Bindings
  //
  // Derived from the currently selected entry's actions.
  // Secondary actions (index > 0) that declare a keybinding
  // are registered here. When the selection changes, the
  // keybinding definitions change structurally and
  // `useKeyBindings` re-registers them automatically.
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
