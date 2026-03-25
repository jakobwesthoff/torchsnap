// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * KeyBinding Context — Type definitions and context creation
 *
 * Provides the registration API for the keybinding engine. Components
 * use the `useKeyBindings` hook rather than consuming this context directly.
 */

import { createContext } from "react";
import type { KeyBindingDefinition } from "./matching";

export interface KeyBindingContextValue {
  /**
   * Register a group of keybinding definitions. Returns a cleanup function
   * that removes them. Each call creates a new group keyed by an internal ID.
   */
  register: (bindings: KeyBindingDefinition[]) => () => void;
}

/**
 * KeyBinding context — use `useKeyBindings()` hook to register bindings,
 * not this context directly.
 */
export const KeyBindingContext = createContext<KeyBindingContextValue | null>(
  null,
);
