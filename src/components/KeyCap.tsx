// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * A single styled keycap. Used to display individual keys in
 * keyboard shortcut indicators throughout the app.
 */

export function KeyCap({ children }: { children: string }) {
  return (
    <kbd className="inline-flex h-5 min-w-5 items-center justify-center rounded border border-border bg-surface-inset px-1 font-mono text-[11px] text-text-muted shadow-[0_1px_0_var(--color-border)]">
      {children}
    </kbd>
  );
}
