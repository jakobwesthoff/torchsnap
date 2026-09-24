// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import type { ModifierKey } from "./matching";
import { formatKey, formatModifier } from "./platform";

/**
 * Parse a Tauri accelerator string (`CmdOrCtrl+Shift+Space`) into
 * display-formatted parts using the app's platform-aware key
 * formatters.
 */
export function accelToDisplayParts(shortcut: string): string[] {
  const tokens = shortcut.split("+");
  const parts: string[] = [];

  for (const token of tokens) {
    if (token === "CommandOrControl" || token === "CmdOrCtrl") {
      parts.push(formatModifier("Meta" as ModifierKey));
    } else if (token === "Shift") {
      parts.push(formatModifier("Shift" as ModifierKey));
    } else if (token === "Alt") {
      parts.push(formatModifier("Alt" as ModifierKey));
    } else {
      parts.push(formatKey(token));
    }
  }

  return parts;
}
