// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Help card shown when the calculator has no result to display:
 * - Empty expression (just `=` typed)
 * - Evaluation error (invalid expression)
 *
 * Shows usage examples demonstrating different capabilities.
 */

const EXAMPLES = [
  { label: "Arithmetic", expr: "2 + 3 * 4" },
  { label: "Parentheses", expr: "(10 + 5) / 3" },
  { label: "Functions", expr: "sqrt(144)" },
  { label: "Comparisons", expr: "2^10 > 1000" },
  { label: "Variables", expr: "a = 5; a * 3" },
];

export function CalculatorHelp() {
  return (
    <div className="px-5 py-3">
      <p className="text-sm text-text-muted mb-2">Type a math expression to evaluate</p>
      <div className="flex flex-col gap-1">
        {EXAMPLES.map((ex) => (
          <div key={ex.expr} className="flex items-baseline gap-2">
            <span className="text-xs text-text-tertiary w-24 shrink-0">{ex.label}</span>
            <code className="text-xs text-text-secondary font-mono">{ex.expr}</code>
          </div>
        ))}
      </div>
    </div>
  );
}
