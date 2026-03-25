// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * KeyBinding Provider — Central registry and document-level keydown listener
 *
 * Maintains a registry of keybinding groups, a sorted match-order array,
 * and a single `document.addEventListener('keydown', ...)` listener.
 * First match wins; inactive bindings still consume the event.
 *
 * The `suppressed` prop allows a parent (e.g. ActionProvider) to disable
 * all keybinding dispatch while a modal or palette is open.
 */

import { useCallback, useEffect, useMemo, useRef, type ReactNode } from "react";
import {
  KeyBindingContext,
  type KeyBindingContextValue,
} from "./KeyBindingContext";
import {
  matchesCombo,
  isInputFocused,
  type KeyBindingDefinition,
} from "./matching";
import { platform } from "./platform";

export interface KeyBindingProviderProps {
  children: ReactNode;
  /** When true, the keydown listener short-circuits and no bindings fire. */
  suppressed?: boolean;
  /** When true, pressing Escape while an input element is focused blurs it
   *  instead of dispatching to keybindings. Default: false. */
  escapeBlursInput?: boolean;
}

/** Match order: layer DESC (more specific first), order ASC, id ASC. */
function matchSort(a: KeyBindingDefinition, b: KeyBindingDefinition): number {
  if (a.layer !== b.layer) {
    return b.layer - a.layer;
  }
  const oa = a.order ?? 0;
  const ob = b.order ?? 0;
  if (oa !== ob) {
    return oa - ob;
  }
  return a.id.localeCompare(b.id);
}

export function KeyBindingProvider({
  children,
  suppressed = false,
  escapeBlursInput = false,
}: KeyBindingProviderProps) {
  const isMacOS = platform === "macos";

  // =========================================================
  // Registry
  // =========================================================

  // Map<groupId, KeyBindingDefinition[]> — one entry per useKeyBindings call.
  const registryRef = useRef(new Map<number, KeyBindingDefinition[]>());
  const nextGroupIdRef = useRef(0);

  // Sorted snapshot for matching (layer DESC).
  const matchBindingsRef = useRef<KeyBindingDefinition[]>([]);

  /** Rebuild sorted array from the registry. */
  const syncBindings = useCallback(() => {
    const all: KeyBindingDefinition[] = [];
    for (const group of registryRef.current.values()) {
      all.push(...group);
    }
    matchBindingsRef.current = all.sort(matchSort);
  }, []);

  const register = useCallback(
    (bindingGroup: KeyBindingDefinition[]): (() => void) => {
      const groupId = nextGroupIdRef.current++;
      registryRef.current.set(groupId, bindingGroup);
      syncBindings();

      return () => {
        // Only remove if it's still the same group reference (not replaced).
        if (registryRef.current.get(groupId) === bindingGroup) {
          registryRef.current.delete(groupId);
          syncBindings();
        }
      };
    },
    [syncBindings],
  );

  // =========================================================
  // Global Keydown Listener
  // =========================================================

  useEffect(() => {
    function handleKeydown(event: KeyboardEvent) {
      if (suppressed) {
        return;
      }

      const inputFocused = isInputFocused();

      // When enabled, Escape while an input is focused blurs it and stops
      // propagation. This gives inputs a predictable dismiss gesture without
      // routing through the keybinding system.
      if (escapeBlursInput && inputFocused && event.key === "Escape") {
        (document.activeElement as HTMLElement).blur();
        event.preventDefault();
        return;
      }

      for (const binding of matchBindingsRef.current) {
        if (!binding.keybindings) {
          continue;
        }

        for (const kb of binding.keybindings) {
          const allowInInput = kb.allowInInput ?? false;

          if (inputFocused && !allowInInput) {
            continue;
          }

          if (matchesCombo(event, kb.combo, isMacOS)) {
            event.preventDefault();
            if (binding.active !== false) {
              binding.handler();
            }
            return; // First match wins; inactive still consumes
          }
        }
      }
    }

    document.addEventListener("keydown", handleKeydown);
    return () => document.removeEventListener("keydown", handleKeydown);
  }, [isMacOS, suppressed, escapeBlursInput]);

  // =========================================================
  // Context Value
  // =========================================================

  const contextValue: KeyBindingContextValue = useMemo(
    () => ({ register }),
    [register],
  );

  return (
    <KeyBindingContext.Provider value={contextValue}>
      {children}
    </KeyBindingContext.Provider>
  );
}
