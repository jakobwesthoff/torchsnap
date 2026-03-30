// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Keyboard shortcut badge that renders a structured keybinding
 * as individual keycaps using the platform-aware formatters —
 * e.g., Meta + Enter renders as ⌘ ↵ on macOS and Ctrl ↵ on
 * Windows/Linux.
 */

import type { ModifierKey } from "../keybindings";
import { formatModifier, formatKey } from "../keybindings";
import { cn } from "../lib/cn";
import { KeyCap } from "./KeyCap";

interface KeyBindingPillProps {
  modifiers?: string[];
  keyName: string;
  className?: string;
  style?: React.CSSProperties;
}

export function KeyBindingPill({ modifiers, keyName, className, style }: KeyBindingPillProps) {
  const parts = [
    ...(modifiers ?? []).map((m) => formatModifier(m as ModifierKey)),
    formatKey(keyName),
  ];

  return (
    <span className={cn("inline-flex items-center gap-0.5", className)} style={style}>
      {parts.map((part, i) => (
        <KeyCap key={i}>{part}</KeyCap>
      ))}
    </span>
  );
}
