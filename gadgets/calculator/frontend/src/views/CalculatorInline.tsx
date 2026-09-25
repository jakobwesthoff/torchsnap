// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Inline result component for the calculator's heuristic mode.
 *
 * Registered as `inline-views["result"]` in the gadget manifest.
 * Renders the calculator result above the standard result list.
 * Participates in the host's selection model at index 0.
 *
 * Enter copies the result to clipboard and dismisses.
 */

import { memo, useEffect } from "react";
import type { InlineViewProps } from "@torchsnap/gadget-sdk";
import { useLauncher } from "@torchsnap/gadget-sdk/hooks";
import { LAYER, useKeyBindings } from "@torchsnap/gadget-sdk/keybindings";
import { CalculatorResult } from "./CalculatorResult";
import { useCopyResult } from "./useCopyResult";

interface CalcData {
  expression: string;
  result: string;
  resultType: string;
}

export const CalculatorInline = memo(function CalculatorInline({
  data,
  selected,
}: InlineViewProps) {
  const { onFooterChange } = useLauncher();
  const copyResult = useCopyResult();
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
        void copyResult({
          expression: calcData.expression,
          result: calcData.result,
          resultType: calcData.resultType,
        });
      },
    },
  ]);

  if (!calcData) return null;

  return (
    <div className={selected ? "bg-selection" : ""}>
      <CalculatorResult
        expression={calcData.expression}
        result={calcData.result}
        resultType={calcData.resultType}
      />
    </div>
  );
});
