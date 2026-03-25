/**
 * Keyboard navigation for the launcher.
 *
 * Uses `useKeyBindings` from the keybinding engine for document-level
 * key handling. All bindings are registered with `allowInInput: true`
 * so they fire even when the search input is focused.
 *
 * Currently only wires up Escape (dismiss). Arrow keys, Enter, and
 * PageUp/PageDown will be added once the result list exists.
 */

import { useMemo } from "react";
import {
  useKeyBindings,
  LAYER,
  type KeyBindingDefinition,
} from "../../keybindings";

interface UseKeyboardNavigationParams {
  dismiss: () => void;
}

// All launcher navigation bindings live above LAYER.COMPONENT so they
// take priority over any app-level bindings for the same keys.
const LAUNCHER_LAYER = LAYER.COMPONENT + 1;

export function useKeyboardNavigation({
  dismiss,
}: UseKeyboardNavigationParams) {
  const bindings: KeyBindingDefinition[] = useMemo(
    () => [
      // TODO: Add ArrowDown, ArrowUp, PageDown, PageUp, Enter once
      // the result list is implemented.
      {
        id: "launcher-escape",
        layer: LAUNCHER_LAYER,
        order: 0,
        handler: () => dismiss(),
        keybindings: [
          { combo: { modifiers: [], key: "Escape" }, allowInInput: true },
        ],
      },
    ],
    [dismiss],
  );

  useKeyBindings(bindings);
}
