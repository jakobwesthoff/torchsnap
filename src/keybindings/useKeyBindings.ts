// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * useKeyBindings — register keybinding definitions that live/die with the component.
 *
 * Wraps handlers in refs so changing a handler never triggers re-registration.
 * An optional `changed` comparator controls when definitions are structurally
 * re-registered; it defaults to reference equality (always re-register).
 */

import { useContext, useEffect, useRef } from "react";
import { KeyBindingContext } from "./KeyBindingContext";
import {
  keyBindingDefinitionsChanged,
  type KeyBindingDefinition,
} from "./matching";

/**
 * Register a group of keybinding definitions tied to the component lifecycle.
 *
 * @param definitions - Array of keybinding definitions to register.
 * @param changed - Optional comparator returning `true` when re-registration
 *   is needed. Receives `(prev, next)`. Defaults to `keyBindingDefinitionsChanged`
 *   which structurally compares all fields except `handler`.
 */
export function useKeyBindings<T extends KeyBindingDefinition>(
  definitions: T[],
  changed?: (prev: T[], next: T[]) => boolean,
): void {
  const context = useContext(KeyBindingContext);

  if (!context) {
    throw new Error("useKeyBindings must be used within a KeyBindingProvider");
  }

  const { register } = context;

  // Map definition id → latest handler, updated every render.
  const handlersRef = useRef(new Map<string, () => void>());

  // Track previous definitions for structural comparison.
  const prevRef = useRef<T[] | null>(null);
  const cleanupRef = useRef<(() => void) | null>(null);

  // Use the provided comparator or fall back to the base structural comparison.
  const hasChanged = changed ?? keyBindingDefinitionsChanged;

  useEffect(() => {
    // Update handler refs every render.
    handlersRef.current.clear();
    for (const def of definitions) {
      handlersRef.current.set(def.id, def.handler);
    }

    // Only re-register when the registration identity changes.
    if (
      prevRef.current &&
      !hasChanged(prevRef.current, definitions) &&
      cleanupRef.current
    ) {
      prevRef.current = definitions;
      return;
    }

    // Clean up previous registration if any.
    if (cleanupRef.current) {
      cleanupRef.current();
    }

    // Assign default order from array index, then wrap handlers in stable refs.
    const registered: KeyBindingDefinition[] = definitions.map(
      (def, index) => ({
        ...def,
        order: def.order ?? index,
        handler: (): void => {
          const latest = handlersRef.current.get(def.id);
          if (latest) {
            latest();
          }
        },
      }),
    );

    cleanupRef.current = register(registered);
    prevRef.current = definitions;
  });

  // Clean up on unmount.
  useEffect(
    () => () => {
      if (cleanupRef.current) {
        cleanupRef.current();
        cleanupRef.current = null;
      }
    },
    [],
  );
}
