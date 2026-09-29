// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Launcher-specific mascot button that positions Snappy above the
 * launcher card in either center or sidekick mode.
 *
 * Receives pre-computed placement values from `useLauncherMascotPlacement`
 * — this component is a pure renderer with no positioning logic.
 *
 * The mascot is double-clickable — `onInfoClick` fires on double-click,
 * keeping the easter egg hidden from casual interaction.
 */

import { Mascot } from "../components/Mascot";
import { MASCOT_SIZE } from "./layout";

interface LauncherMascotProps {
  mode: "center" | "sidekick";
  variant: string;
  top: number;
  /** Centre mode: offset of the image's left edge from the card's. */
  left?: number;
  /** Sidekick mode: offset of the image's right edge from the card's. */
  right?: number;
  onInfoClick: () => void;
}

export function LauncherMascot({
  mode,
  variant,
  top,
  left,
  right,
  onInfoClick,
}: LauncherMascotProps) {
  const size = MASCOT_SIZE[mode];

  if (mode === "sidekick") {
    return (
      <button
        type="button"
        onDoubleClick={onInfoClick}
        style={{ top, right }}
        className="absolute z-10 -scale-x-100 cursor-default bg-transparent border-none p-0 outline-none focus:outline-none"
      >
        <Mascot variant={variant} size={size} />
      </button>
    );
  }

  return (
    <button
      type="button"
      onDoubleClick={onInfoClick}
      style={{ top, left }}
      className="absolute z-10 cursor-default bg-transparent border-none p-0 outline-none focus:outline-none"
    >
      <Mascot variant={variant} size={size} />
    </button>
  );
}
