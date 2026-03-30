// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Launcher-specific mascot button that positions Snappy above the
 * launcher card in either center or sidekick mode.
 *
 * The mascot is double-clickable — `onInfoClick` fires on double-click,
 * keeping the easter egg hidden from casual interaction.
 */

import { Mascot } from "../components/Mascot";

interface LauncherMascotProps {
  mode: "center" | "sidekick";
  variant: string;
  onInfoClick: () => void;
}

export function LauncherMascot({ mode, variant, onInfoClick }: LauncherMascotProps) {
  if (mode === "sidekick") {
    return (
      <button
        type="button"
        onDoubleClick={onInfoClick}
        className="absolute -top-[72px] -right-2.5 z-10 -scale-x-100 cursor-default bg-transparent border-none p-0 outline-none focus:outline-none"
      >
        <Mascot variant={variant} size={96} />
      </button>
    );
  }

  return (
    <button
      type="button"
      onDoubleClick={onInfoClick}
      className="absolute -top-[156px] left-1/2 -translate-x-1/2 z-10 cursor-default bg-transparent border-none p-0 outline-none focus:outline-none"
    >
      <Mascot variant={variant} size={192} />
    </button>
  );
}
