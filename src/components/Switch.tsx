// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Toggle switch with accessible role="switch" semantics.
 *
 * Styled as a macOS-style pill toggle. The track color uses the
 * `accent` token when on and `surface-inset` when off.
 */

import { cn } from "../lib/cn";

interface SwitchProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
  /** Accessible label — applied as aria-label when the switch is
   *  not wrapped in a visible `<label>`. */
  "aria-label"?: string;
  disabled?: boolean;
}

export function Switch({
  checked,
  onChange,
  "aria-label": ariaLabel,
  disabled = false,
}: SwitchProps) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={ariaLabel}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={cn(
        "relative inline-flex h-6 w-11 shrink-0 rounded-full transition-colors duration-200",
        disabled && "opacity-40 cursor-not-allowed",
        checked ? "bg-accent" : "bg-surface-inset",
      )}
    >
      <span
        className={cn(
          "pointer-events-none inline-block h-5 w-5 rounded-full bg-white shadow-sm",
          "transform transition-transform duration-200",
          checked ? "translate-x-[22px]" : "translate-x-0.5",
          "mt-0.5",
        )}
      />
    </button>
  );
}
