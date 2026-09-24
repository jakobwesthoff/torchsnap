// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { accelToDisplayParts } from "../keybindings/accelerator";
import { KeyCap } from "./KeyCap";

/** A Tauri accelerator string shown as keycaps. */
export function ShortcutKeys({ shortcut }: { shortcut: string }) {
  const parts = accelToDisplayParts(shortcut);

  return (
    <span className="inline-flex items-center gap-0.5">
      {parts.map((part, i) => (
        <KeyCap key={i}>{part}</KeyCap>
      ))}
    </span>
  );
}
