// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Split text at matched character boundaries and wrap matched
 * runs in a highlighted span.
 *
 * Positions are character indices from nucleo. For ASCII text
 * these map 1:1 to JS string indices.
 */
// TODO: For non-ASCII (emoji, CJK), nucleo returns grapheme
// indices that may differ from JS string indices. Add a
// conversion layer when needed.

import type { ReactNode } from "react";

const DEFAULT_HIGHLIGHT = "text-accent font-semibold";

export function highlightText(
  text: string,
  positions: number[],
  highlightClassName = DEFAULT_HIGHLIGHT,
): ReactNode[] {
  if (positions.length === 0) return [text];

  const posSet = new Set(positions);
  const segments: ReactNode[] = [];
  let current = "";
  let inMatch = false;

  for (let i = 0; i < text.length; i++) {
    const isMatch = posSet.has(i);
    if (isMatch !== inMatch) {
      if (current) {
        segments.push(
          inMatch ? (
            <span key={`m${i}`} className={highlightClassName}>
              {current}
            </span>
          ) : (
            <span key={`t${i}`}>{current}</span>
          ),
        );
      }
      current = "";
      inMatch = isMatch;
    }
    current += text[i];
  }

  if (current) {
    segments.push(
      inMatch ? (
        <span key="me" className={highlightClassName}>
          {current}
        </span>
      ) : (
        <span key="te">{current}</span>
      ),
    );
  }

  return segments;
}
