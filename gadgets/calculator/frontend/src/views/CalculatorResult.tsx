// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Shared result display component used by both inline and custom UI modes.
 *
 * Shows the evaluated result prominently with the expression as
 * smaller muted context below it.
 */

interface CalculatorResultProps {
  expression: string;
  result: string;
  resultType: string;
}

export function CalculatorResult({ expression, result, resultType }: CalculatorResultProps) {
  return (
    <div className="flex flex-col items-start px-5 py-3">
      <span
        className={`text-2xl font-semibold tabular-nums ${
          resultType === "boolean" ? "text-accent" : "text-text-primary"
        }`}
      >
        {result}
      </span>
      <span className="text-sm text-text-muted mt-0.5 truncate max-w-full">{expression}</span>
    </div>
  );
}
