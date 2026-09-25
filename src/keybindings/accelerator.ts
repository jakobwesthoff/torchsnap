// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import {
  formatKey,
  formatModifiersForPlatform,
  platform,
  type PhysicalModifier,
  type Platform,
} from "./platform";

/**
 * Modifier tokens Tauri's accelerator parser accepts, uppercased. The
 * combined `CommandOrControl` spellings resolve per platform, like the
 * parser does: Cmd on macOS, Ctrl elsewhere.
 */
function modifierForToken(token: string, p: Platform): PhysicalModifier | null {
  switch (token.toUpperCase()) {
    case "CONTROL":
    case "CTRL":
      return "Control";
    case "ALT":
    case "OPTION":
      return "Alt";
    case "SHIFT":
      return "Shift";
    case "SUPER":
    case "COMMAND":
    case "CMD":
      return "Super";
    case "COMMANDORCONTROL":
    case "COMMANDORCTRL":
    case "CMDORCTRL":
    case "CMDORCONTROL":
      return p === "macos" ? "Super" : "Control";
    default:
      return null;
  }
}

/**
 * Parse a Tauri accelerator string (`CmdOrCtrl+Shift+Space`) into
 * display parts: the modifiers in the shared order, then the key.
 *
 * Exported with explicit platform parameter for testability;
 * consumers typically use `accelToDisplayParts`.
 */
export function accelToDisplayPartsForPlatform(shortcut: string, p: Platform): string[] {
  const modifiers: PhysicalModifier[] = [];
  const keys: string[] = [];

  for (const token of shortcut.split("+")) {
    const modifier = modifierForToken(token, p);
    if (modifier) {
      modifiers.push(modifier);
    } else {
      keys.push(formatKey(token));
    }
  }

  return [...formatModifiersForPlatform(modifiers, p), ...keys];
}

export function accelToDisplayParts(shortcut: string): string[] {
  return accelToDisplayPartsForPlatform(shortcut, platform);
}
