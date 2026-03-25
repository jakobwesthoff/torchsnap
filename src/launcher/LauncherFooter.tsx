// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Contextual action footer for the launcher result list.
 *
 * Shows the available actions for the currently selected entry as
 * key pill + label pairs. Updates as the selection changes.
 *
 * The primary action always shows with "↵". Additional actions
 * display their declared keybinding label when available.
 */

import { KeyPill } from "../components/KeyPill";
import type { Action } from "./types";

interface LauncherFooterProps {
  actions: Action[];
}

export function LauncherFooter({ actions }: LauncherFooterProps) {
  if (actions.length === 0) return null;

  return (
    <div className="flex items-center gap-4 border-t border-border px-5 py-2">
      {actions.map((action, index) => {
        // The primary action (index 0) always gets the Enter key.
        // Secondary actions use their declared keybinding label
        // if available.
        const keyLabel =
          index === 0
            ? "↵"
            : (action.keybinding?.label ?? null);

        // Don't render actions without a keybinding (except primary).
        if (keyLabel === null) return null;

        return (
          <span
            key={action.id.type === "custom" ? action.id.value : action.id.type}
            className="inline-flex items-center gap-1.5 text-xs text-text-muted"
          >
            <KeyPill>{keyLabel}</KeyPill>
            <span>{action.label}</span>
          </span>
        );
      })}
    </div>
  );
}
