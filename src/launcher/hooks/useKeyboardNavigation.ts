/**
 * Keyboard navigation for the launcher.
 *
 * Uses `useKeyBindings` from the keybinding engine for document-level
 * key handling. All bindings are registered with `allowInInput: true`
 * so they fire even when the search input is focused.
 */

import { useCallback, useMemo, type RefObject } from "react";
import {
  useKeyBindings,
  LAYER,
  type KeyBindingDefinition,
} from "../../keybindings";

interface UseKeyboardNavigationParams {
  dismiss: () => void;
  resultCount: number;
  selectedIndex: number;
  setSelectedIndex: (index: number) => void;
  onExecute: () => void;
  mouseActiveRef: RefObject<boolean>;
}

// Approximate number of items visible at once. Used as the jump
// distance for PageUp/PageDown.
const PAGE_SIZE = 8;

// All launcher navigation bindings live above LAYER.COMPONENT so they
// take priority over any app-level bindings for the same keys.
const LAUNCHER_LAYER = LAYER.COMPONENT + 1;

export function useKeyboardNavigation({
  dismiss,
  resultCount,
  selectedIndex,
  setSelectedIndex,
  onExecute,
  mouseActiveRef,
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

  const bindings: KeyBindingDefinition[] = useMemo(
    () => [
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
        order: 5,
        handler: () => dismiss(),
        keybindings: [
          { combo: { modifiers: [], key: "Escape" }, allowInInput: true },
        ],
      },
    ],
    [moveSelection, onExecute, dismiss],
  );

  useKeyBindings(bindings);
}
