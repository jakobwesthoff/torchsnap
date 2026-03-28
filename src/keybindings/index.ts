// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// Types and matching
export type { ModifierKey, KeyCombo, KeyBinding, KeyBindingDefinition } from "./matching";
export {
  LAYER,
  matchesCombo,
  isInputFocused,
  keyBindingArraysEqual,
  keyBindingDefinitionsChanged,
} from "./matching";

// Platform detection and formatting
export type { Platform } from "./platform";
export {
  platform,
  detectPlatform,
  formatModifier,
  formatModifierForPlatform,
  formatKey,
  formatKeyForPlatform,
} from "./platform";

// Context
export type { KeyBindingContextValue } from "./KeyBindingContext";
export { KeyBindingContext } from "./KeyBindingContext";

// Provider
export type { KeyBindingProviderProps } from "./KeyBindingProvider";
export { KeyBindingProvider } from "./KeyBindingProvider";

// Hook
export { useKeyBindings } from "./useKeyBindings";
