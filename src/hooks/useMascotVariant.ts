// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Manages the active Snappy mascot variant across launcher shows.
 *
 * Integrates the weighted random selection system with the Tauri window
 * lifecycle and user settings. The variant is re-rolled on every window
 * blur (launcher dismiss) so the next variant's image is pre-rendered
 * and cached by the browser before the launcher appears again.
 *
 * Respects two settings:
 *   - `randomMascots` — when false, always returns `"original"`.
 *   - `showNsfwMascots` — when false, NSFW-tagged variants are excluded
 *     from the selection pool.
 */

import { useEffect, useState } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { useSetting } from "./useSetting";
import { selectMascotVariant } from "./useRandomMascot";
import { SnappyHeroSets, isNsfwVariant } from "../mascotVariants";

const appWindow = getCurrentWebviewWindow();

/** A single roll of the mascot dice. Both pools are drawn at once so a
 *  settings change can be answered by picking the matching field rather
 *  than by re-rolling, which keeps the displayed variant a pure function
 *  of the roll and the current settings. */
interface MascotRoll {
  fromFullPool: string;
  fromSfwPool: string;
}

// Both draws happen on every roll, including the SFW one while NSFW is
// allowed. `selectMascotVariant` throws on a pool with no positive
// weight, so this assumes `SnappyHeroSets` always contains at least one
// weighted SFW variant.
function rollMascot(): MascotRoll {
  return {
    fromFullPool: selectMascotVariant(SnappyHeroSets),
    fromSfwPool: selectMascotVariant(SnappyHeroSets, (v) => !isNsfwVariant(v)),
  };
}

export function useMascotVariant(): { variant: string } {
  const [randomMascots] = useSetting<boolean>("randomMascots");
  const [showNsfwMascots] = useSetting<boolean>("showNsfwMascots");

  const [roll, setRoll] = useState(rollMascot);

  // Re-roll the mascot on every launcher dismiss so the next variant is
  // already rendered (and its image cached by the browser) before the
  // launcher appears again. The launcher stays mounted between shows, so
  // React renders the new <Mascot> into the hidden DOM and the browser
  // fetches the image while the window is invisible.
  useEffect(() => {
    const unlisten = appWindow.listen("tauri://blur", () => setRoll(rollMascot()));
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  // Settings can change while the launcher is visible (the user toggles
  // one in the settings window), so the variant is derived on every
  // render instead of being corrected after the fact. An NSFW pick is
  // swapped for the roll's SFW draw the moment NSFW is disallowed;
  // a pick that is already safe stays put.
  const variant = !randomMascots
    ? "original"
    : showNsfwMascots || !isNsfwVariant(roll.fromFullPool)
      ? roll.fromFullPool
      : roll.fromSfwPool;

  return { variant };
}
