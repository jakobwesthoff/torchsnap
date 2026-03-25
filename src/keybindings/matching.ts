// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Keybinding types and matching logic.
 *
 * Defines the base types for keyboard shortcut matching — combo types,
 * keybinding definitions, layer constants, and the core `matchesCombo`
 * function. Shared by both the `KeyBindingProvider` and the frontend's
 * higher-level action system.
 */

// =========================================================
// Key Combo Types
// =========================================================

/**
 * Supported modifier keys.
 *
 * `Meta` maps to Cmd on macOS and Ctrl on Windows/Linux — both for event
 * matching and display. There is intentionally no raw `Ctrl` modifier to
 * prevent collisions: on macOS Meta+A (Cmd+A) and Ctrl+A are distinct
 * physical combos, but on Windows both would map to ctrlKey+A. By not
 * exposing raw Ctrl, the type system makes this collision impossible.
 *
 * TODO: Add a `Ctrl` escape hatch if terminal-style bindings are ever needed.
 */
export type ModifierKey = "Meta" | "Shift" | "Alt";

export interface KeyCombo {
  modifiers: ModifierKey[];
  key: string;
}

// =========================================================
// Keybinding Types
// =========================================================

/** A keybinding attached to a definition — one definition can have multiple. */
export interface KeyBinding {
  combo: KeyCombo;
  /** When true the binding fires even while an input element is focused.
   *  Default: false — must be explicitly opted in. */
  allowInInput?: boolean;
}

/**
 * A single keybinding registration. This is the base type that
 * higher-level systems (e.g. an action palette) can extend with
 * display metadata.
 */
export interface KeyBindingDefinition {
  id: string;
  /** Layering priority. Higher-layer definitions take precedence on key conflicts. */
  layer: number;
  /** Sort order within the same layer. Defaults to array index in `useKeyBindings`. */
  order?: number;
  handler: () => void;
  /** When false the keybinding still consumes the event but the
   *  handler is a no-op. Defaults to true. */
  active?: boolean;
  keybindings?: KeyBinding[];
}

/** Well-known layer constants. Higher values = more specific. */
export const LAYER = {
  APP: 0,
  VIEW: 1,
  COMPONENT: 2,
} as const;

// =========================================================
// Key Combo Matching
// =========================================================

/**
 * Check whether a keyboard event matches a key combo.
 *
 * Platform-aware: `Meta` modifier checks `metaKey` on macOS and `ctrlKey`
 * on Windows/Linux. On macOS, physical Ctrl presses (ctrlKey without
 * metaKey) are rejected so they don't accidentally match `Meta` bindings.
 *
 * Shift handling: only enforced as a strict modifier for single lowercase
 * letter keys (a-z). Characters like `?`, `!`, `+` naturally require Shift
 * to produce but don't list it in modifiers — the matcher permits Shift
 * for those cases.
 */
export function matchesCombo(
  event: KeyboardEvent,
  combo: KeyCombo,
  isMacOS: boolean,
): boolean {
  // -------------------------------------------------------
  // Key match
  // -------------------------------------------------------
  if (event.key !== combo.key) {
    return false;
  }

  // -------------------------------------------------------
  // Modifier state extraction
  // -------------------------------------------------------
  const wantsMeta = combo.modifiers.includes("Meta");
  const wantsShift = combo.modifiers.includes("Shift");
  const wantsAlt = combo.modifiers.includes("Alt");

  // Meta maps to metaKey on macOS, ctrlKey on others
  const metaPressed = isMacOS ? event.metaKey : event.ctrlKey;

  if (wantsMeta !== metaPressed) {
    return false;
  }

  // On macOS, reject physical Ctrl presses that are not part of Meta.
  // This prevents Ctrl+A from matching a Meta+A binding on macOS where
  // Meta is Cmd, not Ctrl.
  if (isMacOS && event.ctrlKey && !wantsMeta) {
    return false;
  }
  // Also reject if ctrlKey is pressed alongside metaKey on macOS (extra modifier)
  if (isMacOS && event.ctrlKey && event.metaKey) {
    return false;
  }

  // On non-macOS, reject metaKey as an extra modifier (Windows/Super key)
  if (!isMacOS && event.metaKey) {
    return false;
  }

  // Alt
  if (wantsAlt !== event.altKey) {
    return false;
  }

  // -------------------------------------------------------
  // Shift handling
  // -------------------------------------------------------
  // For single lowercase letter keys, strictly enforce shift state.
  // For everything else (punctuation, symbols), allow shift since it may
  // be needed to produce the character.
  const isSingleLetter =
    combo.key.length === 1 && combo.key >= "a" && combo.key <= "z";
  if (isSingleLetter) {
    if (wantsShift !== event.shiftKey) {
      return false;
    }
  } else {
    // If the combo explicitly wants shift, it must be pressed
    if (wantsShift && !event.shiftKey) {
      return false;
    }
  }

  return true;
}

// =========================================================
// Input Focus Detection
// =========================================================

/**
 * Check whether the currently focused element is an input-like element
 * where keyboard shortcuts should be suppressed.
 *
 * Detects `<input>`, `<textarea>`, and elements with `contentEditable`.
 */
export function isInputFocused(): boolean {
  const el = document.activeElement;
  if (!el) {
    return false;
  }

  const tag = el.tagName;
  if (tag === "INPUT" || tag === "TEXTAREA") {
    return true;
  }

  // Check both the computed property and the raw attribute for maximum compatibility.
  if (
    el instanceof HTMLElement &&
    (el.isContentEditable || el.contentEditable === "true")
  ) {
    return true;
  }

  return false;
}

// =========================================================
// Structural Comparison Helpers
// =========================================================

/** Shallow-compare two KeyCombo values by their fields. */
function combosEqual(a: KeyCombo, b: KeyCombo): boolean {
  if (a.key !== b.key) {
    return false;
  }
  if (a.modifiers.length !== b.modifiers.length) {
    return false;
  }
  for (let i = 0; i < a.modifiers.length; i++) {
    if (a.modifiers[i] !== b.modifiers[i]) {
      return false;
    }
  }
  return true;
}

/** Compare two KeyBinding values. */
function keyBindingsEqual(a: KeyBinding, b: KeyBinding): boolean {
  if (!combosEqual(a.combo, b.combo)) {
    return false;
  }
  if ((a.allowInInput ?? undefined) !== (b.allowInInput ?? undefined)) {
    return false;
  }
  return true;
}

/** Compare two keybinding arrays. */
export function keyBindingArraysEqual(
  a: KeyBinding[] | undefined,
  b: KeyBinding[] | undefined,
): boolean {
  if (a === b) {
    return true;
  }
  if (!a || !b) {
    return false;
  }
  if (a.length !== b.length) {
    return false;
  }
  for (let i = 0; i < a.length; i++) {
    if (!keyBindingsEqual(a[i], b[i])) {
      return false;
    }
  }
  return true;
}

/**
 * Default structural comparison for `KeyBindingDefinition[]`.
 *
 * Returns `true` when re-registration is needed (i.e. they differ).
 * Compares all fields except `handler` — the handler is wrapped in a ref
 * and always calls the latest version without re-registration.
 */
export function keyBindingDefinitionsChanged(
  prev: KeyBindingDefinition[],
  next: KeyBindingDefinition[],
): boolean {
  if (prev.length !== next.length) {
    return true;
  }
  for (let i = 0; i < prev.length; i++) {
    const p = prev[i];
    const n = next[i];
    if (p.id !== n.id) {
      return true;
    }
    if (p.layer !== n.layer) {
      return true;
    }
    if ((p.order ?? i) !== (n.order ?? i)) {
      return true;
    }
    if ((p.active ?? true) !== (n.active ?? true)) {
      return true;
    }
    if (!keyBindingArraysEqual(p.keybindings, n.keybindings)) {
      return true;
    }
  }
  return false;
}
