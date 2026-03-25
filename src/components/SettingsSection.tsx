// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * A visually grouped section in the settings panel.
 *
 * Renders a bordered card with an optional title and stacked
 * children (typically `SettingsEntry` rows).
 */

import type { ReactNode } from "react";
import { cn } from "../lib/cn";

interface SettingsSectionProps {
  title?: string;
  children: ReactNode;
  className?: string;
}

export function SettingsSection({
  title,
  children,
  className,
}: SettingsSectionProps) {
  return (
    <div className={cn("rounded-xl border border-border p-4", className)}>
      {title && (
        <h3 className="text-sm font-medium text-text-secondary mb-3">
          {title}
        </h3>
      )}
      <div className="flex flex-col gap-3">{children}</div>
    </div>
  );
}
