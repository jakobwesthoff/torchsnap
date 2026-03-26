// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Contextual action footer for the launcher result list.
 *
 * Shows the available actions for the currently selected entry as
 * keybinding pill + label pairs. The primary action is displayed on
 * the left, secondary actions on the right. Updates as the selection
 * changes.
 */

import { KeyBindingPill } from "../components/KeyBindingPill";
import type { Action } from "./types";

interface LauncherFooterProps {
  actions: Action[];
}

export function LauncherFooter({ actions }: LauncherFooterProps) {
  if (actions.length === 0) return null;

  const primary = actions[0];
  const secondaries = actions.slice(1).filter((a) => a.keybinding);

  return (
    <div className="flex items-center justify-between border-t border-border px-5 py-2.5">
      {/* Primary action — always on the left with Enter keybinding */}
      <span className="inline-flex items-center gap-1.5 text-xs text-text-muted">
        <KeyBindingPill modifiers={[]} keyName="Enter" />
        <span>{primary.label}</span>
      </span>

      {/* Secondary actions — grouped on the right */}
      {secondaries.length > 0 && (
        <div className="flex items-center gap-4">
          {secondaries.map((action) => (
            <span
              key={action.id.type === "custom" ? action.id.value : action.id.type}
              className="inline-flex items-center gap-1.5 text-xs text-text-muted"
            >
              <KeyBindingPill
                modifiers={action.keybinding!.modifiers}
                keyName={action.keybinding!.key}
              />
              <span>{action.label}</span>
            </span>
          ))}
        </div>
      )}
    </div>
  );
}
