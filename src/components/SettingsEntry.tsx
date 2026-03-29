// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * A single row inside a settings section.
 *
 * Displays a label on the left and an arbitrary control (switch,
 * input, button, etc.) on the right. Wraps both in a `<label>` so
 * clicking the text activates the control.
 */

import type { ReactNode } from "react";

interface SettingsEntryProps {
  label: string;
  description?: string;
  children: ReactNode;
}

export function SettingsEntry({ label, description, children }: SettingsEntryProps) {
  return (
    <label className="flex items-center justify-between cursor-pointer">
      <div className="flex flex-col">
        <span className="text-sm text-text-primary">{label}</span>
        {description && (
          <span className="text-xs text-text-tertiary">{description}</span>
        )}
      </div>
      {children}
    </label>
  );
}
