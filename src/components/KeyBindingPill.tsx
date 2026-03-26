// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Keyboard shortcut badge that renders a structured keybinding as a
 * platform-appropriate display string.
 *
 * Takes modifier keys and a key name (the same vocabulary as the
 * keybinding engine) and formats them using the platform-aware
 * formatters — e.g., Meta+Enter renders as "⌘↵" on macOS and
 * "Ctrl+↵" on Windows/Linux.
 */

import type { ModifierKey } from "../keybindings";
import { formatModifier, formatKey } from "../keybindings";

interface KeyBindingPillProps {
  modifiers?: string[];
  keyName: string;
}

export function KeyBindingPill({ modifiers, keyName }: KeyBindingPillProps) {
  const parts = [
    ...(modifiers ?? []).map((m) => formatModifier(m as ModifierKey)),
    formatKey(keyName),
  ];

  return (
    <kbd className="rounded border border-border bg-surface-inset px-1.5 py-0.5 font-mono text-[11px] text-text-muted shadow-[0_1px_0_var(--color-border)]">
      {parts.join("")}
    </kbd>
  );
}
