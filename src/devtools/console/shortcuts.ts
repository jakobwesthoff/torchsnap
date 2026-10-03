// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Console Shortcuts
//
// The devtools window mounts no KeyBindingProvider, so the
// console matches its few window-local shortcuts itself with
// the shared `matchesCombo`. `Meta` is Cmd on macOS and Ctrl
// elsewhere.
// =========================================================

import { matchesCombo, type KeyCombo } from "../../keybindings/matching";
import {
  formatKeyForPlatform,
  formatModifiersForPlatform,
  physicalModifierFor,
  platform,
} from "../../keybindings/platform";

export const CLEAR_LOG: KeyCombo = { modifiers: ["Meta"], key: "k" };
export const FOCUS_SEARCH: KeyCombo = { modifiers: ["Meta"], key: "f" };

export function matchesConsoleShortcut(event: KeyboardEvent, combo: KeyCombo): boolean {
  return matchesCombo(event, combo, platform === "macos");
}

/** The combo as tooltip text: `⌘K` on macOS, `Ctrl+K` elsewhere. */
export function consoleShortcutLabel(combo: KeyCombo): string {
  const modifiers = formatModifiersForPlatform(
    combo.modifiers.map((m) => physicalModifierFor(m, platform)),
    platform,
  );
  const parts = [...modifiers, formatKeyForPlatform(combo.key, platform)];
  return parts.join(platform === "macos" ? "" : "+");
}
