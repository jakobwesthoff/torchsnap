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
