// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Shows how the mascot on screen was drawn: when, from which pool with
 * which share, the character and variant, the occasions whose condition
 * held, and the recent list (ADR 59).
 *
 * A development aid. The launcher renders it only behind
 * `import.meta.env.DEV`, which is `false` in every `vite build`, so
 * release builds leave it out.
 */

import { CONDITIONS } from "./conditions";
import { ALL_YEAR, type MascotDraw } from "./selection";
import { OCCASIONS } from "./variants";

interface MascotDebugPanelProps {
  /** The draw on screen; `null` while random mascots are off. */
  draw: MascotDraw | null;
  onClose: () => void;
}

/** A fraction as a percentage with at most one decimal: 0.04 is "4%". */
function percent(fraction: number): string {
  return `${Number((fraction * 100).toFixed(1))}%`;
}

function formatTime(date: Date): string {
  const pad = (value: number) => String(value).padStart(2, "0");
  return (
    `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ` +
    `${pad(date.getHours())}:${pad(date.getMinutes())}`
  );
}

/** The pool line: an occasion's own share of what the occasions above it
 *  left, and the result; the all-year pool gets what is left. */
function describePool(draw: MascotDraw): string {
  const index = draw.shares.findIndex((entry) => entry.pool === draw.pool);
  const { share } = draw.shares[index];
  if (draw.pool === ALL_YEAR || index === 0) return `${draw.pool}: ${percent(share)}`;
  const left = 1 - draw.shares.slice(0, index).reduce((sum, entry) => sum + entry.share, 0);
  return `${draw.pool}: ${percent(share / left)} of the ${percent(left)} left = ${percent(share)}`;
}

/** One line per occasion whose condition held, then the all-year pool. */
function activeLines(draw: MascotDraw): [string, string][] {
  const shareOf = (pool: string) => draw.shares.find((entry) => entry.pool === pool)?.share;
  const lines: [string, string][] = OCCASIONS.filter((occasion) =>
    draw.conditionsHeld.includes(occasion.condition),
  ).map((occasion) => {
    const share = shareOf(occasion.group);
    const description = CONDITIONS[occasion.condition]?.description ?? occasion.condition;
    return [
      `${occasion.group} ${share === undefined ? "no eligible character" : percent(share)}`,
      description,
    ];
  });
  lines.push([`${ALL_YEAR} ${percent(shareOf(ALL_YEAR) ?? 0)}`, ""]);
  return lines;
}

export function MascotDebugPanel({ draw, onClose }: MascotDebugPanelProps) {
  const rows: [string, string][] = draw
    ? [
        ["drawn", formatTime(draw.now)],
        ["pool", describePool(draw)],
        ["character", `${draw.character} (variant ${draw.variant})`],
        ...activeLines(draw).map(([share, description], index): [string, string] => [
          index === 0 ? "active" : "",
          description ? `${share}   ${description}` : share,
        ]),
        ["recent", `${draw.remembered} remembered, ${draw.excluded} excluded from this pool`],
      ]
    : [["drawn", "nothing: random mascots are off"]];

  return (
    <div
      className="absolute left-0 right-0 top-full mt-2 rounded-lg bg-surface/95 px-4 py-3 text-xs text-text-secondary shadow"
      onClick={onClose}
      data-testid="mascot-debug-panel"
    >
      <table className="font-mono">
        <tbody>
          {rows.map(([label, value], index) => (
            <tr key={index}>
              <td className="pr-4 align-top text-text-muted">{label}</td>
              <td className="whitespace-pre-wrap">{value}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
