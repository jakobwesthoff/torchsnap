// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Settings-page record list with per-row actions.
 *
 * Shared component for gadgets that need to render a manageable
 * collection — recent history, remembered networks, saved
 * presets — inside their settings panel. Each row carries a
 * primary text, an optional secondary descriptor, and zero or
 * more action buttons.
 *
 * Not intended for launcher-results virtualized lists. Settings
 * lists are short by definition; consumers needing thousands of
 * rows should reach for `useWindowedList` directly and roll
 * their own DOM.
 */

import type { ReactNode } from "react";
import { cn } from "../lib/cn";

export interface ListAction {
  label: string;
  onClick: () => void;
  /** `danger` styles the action red for destructive operations
   *  (`Delete`, `Forget`, `Clear`). Omit for neutral actions. */
  variant?: "default" | "danger";
  disabled?: boolean;
}

export interface ListItem {
  /** Stable identifier used as the React key. */
  key: string;
  primary: ReactNode;
  secondary?: ReactNode;
  actions?: ListAction[];
}

export interface ListProps {
  items: ListItem[];
  /** Rendered when `items` is empty. Pass a string for a
   *  default-styled message; pass any node for full control. */
  emptyState?: ReactNode;
  className?: string;
}

export function List({ items, emptyState, className }: ListProps) {
  if (items.length === 0) {
    if (emptyState === undefined) return null;
    if (typeof emptyState === "string") {
      return (
        <div className={cn("py-3 text-sm text-text-tertiary text-center", className)}>
          {emptyState}
        </div>
      );
    }
    return <div className={className}>{emptyState}</div>;
  }

  return (
    <ul
      className={cn(
        "flex flex-col rounded-md border border-border bg-surface-group divide-y divide-border",
        className,
      )}
    >
      {items.map((item) => (
        <li key={item.key} className="flex items-center justify-between gap-3 px-3 py-2">
          <div className="flex flex-col min-w-0 flex-1">
            <div className="text-sm text-text-primary truncate">{item.primary}</div>
            {item.secondary !== undefined && (
              <div className="text-xs text-text-tertiary truncate">{item.secondary}</div>
            )}
          </div>
          {item.actions && item.actions.length > 0 && (
            <div className="flex items-center gap-1 shrink-0">
              {item.actions.map((action, idx) => (
                <button
                  key={idx}
                  type="button"
                  onClick={action.onClick}
                  disabled={action.disabled}
                  className={cn(
                    "px-2 py-1 text-xs rounded border transition-colors",
                    action.disabled && "opacity-40 cursor-not-allowed",
                    action.variant === "danger"
                      ? "border-border text-red-500 hover:bg-red-500/10"
                      : "border-border text-text-primary hover:bg-surface-inset",
                  )}
                >
                  {action.label}
                </button>
              ))}
            </div>
          )}
        </li>
      ))}
    </ul>
  );
}
