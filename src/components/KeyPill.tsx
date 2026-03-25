// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * A small keyboard shortcut badge (pill) for displaying key hints.
 *
 * Renders as a bordered monospace label matching the style used in
 * the search input's ESC badge and the launcher footer actions.
 */

interface KeyPillProps {
  children: string;
}

export function KeyPill({ children }: KeyPillProps) {
  return (
    <kbd className="rounded border border-border bg-surface-inset px-1.5 py-0.5 font-mono text-[10px] text-text-muted shadow-[0_1px_0_var(--color-border)]">
      {children}
    </kbd>
  );
}
