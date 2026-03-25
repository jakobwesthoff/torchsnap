// Types and matching
export type {
  ModifierKey,
  KeyCombo,
  KeyBinding,
  KeyBindingDefinition,
} from "./matching";
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
