// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Range slider styled to match the app's design system.
 *
 * Uses a native `<input type="range">` with custom CSS for the
 * track and thumb. The filled portion of the track uses the
 * `accent` color, unfilled uses `surface-inset`.
 *
 * An optional `formatValue` prop lets callers customize the
 * displayed value (e.g., "30 days" instead of "30").
 */

import { useCallback, useMemo } from "react";

interface SliderProps {
  value: number;
  onChange: (value: number) => void;
  min: number;
  max: number;
  step?: number;
  /** Custom formatter for the displayed value. */
  formatValue?: (value: number) => string;
  disabled?: boolean;
}

export function Slider({
  value,
  onChange,
  min,
  max,
  step = 1,
  formatValue,
  disabled = false,
}: SliderProps) {
  const displayValue = useMemo(
    () => (formatValue ? formatValue(value) : String(value)),
    [value, formatValue],
  );

  const handleChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      onChange(Number(e.target.value));
    },
    [onChange],
  );

  // Compute the fill percentage for the gradient background.
  const fillPercent = ((value - min) / (max - min)) * 100;

  return (
    <div className="flex items-center gap-3">
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={handleChange}
        disabled={disabled}
        className="slider-track h-1.5 w-full appearance-none rounded-full outline-none disabled:opacity-40"
        style={{
          background: `linear-gradient(to right, var(--color-accent) ${fillPercent}%, var(--color-surface-inset) ${fillPercent}%)`,
        }}
      />
      <span className="text-sm text-text-secondary tabular-nums whitespace-nowrap min-w-[3ch] text-right">
        {displayValue}
      </span>
    </div>
  );
}
