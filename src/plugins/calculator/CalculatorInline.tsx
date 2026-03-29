// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Inline result component for the calculator's heuristic mode.
 *
 * Registered as `inlineViews["result"]` in the plugin registry.
 * Renders the calculator result above the standard result list.
 * Participates in the host's selection model at index 0.
 *
 * Enter copies the result to clipboard and dismisses.
 */

import { useEffect } from "react";
import type { InlineViewProps } from "../types";
import { CalculatorResult } from "./CalculatorResult";
import { useKeyBindings } from "../../keybindings/useKeyBindings";
import { LAYER } from "../../keybindings/matching";

interface CalcData {
  expression: string;
  result: string;
  resultType: string;
}

export default function CalculatorInline({
  data,
  selected,
  onExecute,
  onFooterChange,
}: InlineViewProps) {
  const calcData = data as CalcData | undefined;

  // Report our footer to the host on mount.
  useEffect(() => {
    onFooterChange({
      primary: { combo: { modifiers: [], key: "Enter" }, label: "Copy to Clipboard" },
      hints: [],
    });
  }, [onFooterChange]);

  // Register Enter key handler when the inline slot is selected.
  useKeyBindings([
    {
      id: "calculator-inline-enter",
      layer: LAYER.COMPONENT + 2,
      active: selected && calcData != null,
      keybindings: [{ combo: { modifiers: [], key: "Enter" }, allowInInput: true }],
      handler: () => {
        if (!calcData) return;
        // Copy the result value. The backend's execute() copies
        // to clipboard and returns Dismiss.
        onExecute(calcData.result, { type: "copy" });
      },
    },
  ]);

  if (!calcData) return null;

  return (
    <div className={`transition-colors ${selected ? "bg-selection" : ""}`}>
      <CalculatorResult
        expression={calcData.expression}
        result={calcData.result}
        resultType={calcData.resultType}
      />
    </div>
  );
}
