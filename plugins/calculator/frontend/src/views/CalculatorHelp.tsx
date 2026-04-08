// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Help card shown when just `=` is typed (empty expression).
 * Compact layout to fit within the fixed-height inline area (76px).
 */

const EXAMPLES = ["2 + 3 * 4", "sqrt(144)", "2^10 > 1000", "a = 5; a * 3"];

export function CalculatorHelp() {
  return (
    <div className="flex flex-col items-center justify-center h-full px-5">
      <p className="text-sm text-text-muted">Type a math expression</p>
      <div className="flex items-center gap-3 mt-1">
        {EXAMPLES.map((ex, i) => (
          <span key={ex}>
            <code className="text-xs text-text-tertiary font-mono">{ex}</code>
            {i < EXAMPLES.length - 1 && <span className="text-text-tertiary/40 ml-3">·</span>}
          </span>
        ))}
      </div>
    </div>
  );
}

/**
 * Error display for failed expressions. Shown without the help
 * examples — just the error message in a muted style.
 */
export function CalculatorError({ message }: { message: string }) {
  return (
    <div className="flex items-center justify-center h-full px-5">
      <p className="text-sm text-text-tertiary">{message}</p>
    </div>
  );
}
