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
  children: ReactNode;
}

export function SettingsEntry({ label, children }: SettingsEntryProps) {
  return (
    <label className="flex items-center justify-between cursor-pointer">
      <span className="text-sm text-text-primary">{label}</span>
      {children}
    </label>
  );
}
