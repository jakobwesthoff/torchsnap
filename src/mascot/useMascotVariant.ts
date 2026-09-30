// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Manages the Snappy mascot the launcher shows across launcher shows.
 *
 * The next mascot is drawn on every window blur (launcher dismiss), so
 * its image is rendered and cached by the browser before the launcher
 * appears again. The launcher stays mounted between shows, so React
 * renders the new <Mascot> into the hidden DOM. The draw evaluates the
 * occasions at that moment: a launcher that stays hidden across an
 * occasion's edge shows a draw from the other side of it first (ADR 59).
 *
 * The characters that were on screen go into a recent list, which the
 * draw uses to avoid repeats. It lives in memory only; a restart clears
 * it.
 *
 * Settings:
 *   - `randomMascots` off: shows `"original"`, and blurs draw nothing.
 *     The draw from before stays and shows again once it is turned on.
 *   - `showNsfwMascots` off: the draw leaves NSFW variants out. An NSFW
 *     variant on screen when it is turned off is replaced at once by a
 *     new draw.
 *   - `mascotMode` off: nothing is on screen, so nothing is remembered.
 */

import { useEffect, useState } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { useSetting } from "../hooks/useSetting";
import { getSettingSync } from "../settingsStore";
import { CONDITIONS } from "./conditions";
import { drawMascot, rememberShown, type MascotDraw } from "./selection";
import { ORIGINAL_MASCOT, isNsfwVariant, mascotFacts, mascotPools } from "./variants";

/** The draw on hand and the recently shown characters. Both change only
 *  through pure functional updates, so StrictMode's double calls in dev
 *  change nothing. */
interface MascotState {
  draw: MascotDraw;
  recent: string[];
}

/** Draws with the settings of this moment, read from the store rather
 *  than a render, so the blur listener registered once sees current
 *  values. */
function draw(
  recent: string[],
  allowNsfw = getSettingSync<boolean>("showNsfwMascots"),
): MascotDraw {
  return drawMascot({
    pools: mascotPools,
    mascots: mascotFacts,
    conditions: CONDITIONS,
    now: new Date(),
    allowNsfw,
    recent,
    random: Math.random,
  });
}

/** Whether the settings allow a drawn variant to stay on screen. */
function isAllowed(variant: string, allowNsfw: boolean): boolean {
  return allowNsfw || !isNsfwVariant(variant);
}

export function useMascotVariant(): { variant: string; draw: MascotDraw | null } {
  const [randomMascots] = useSetting<boolean>("randomMascots");
  const [showNsfwMascots] = useSetting<boolean>("showNsfwMascots");

  const [state, setState] = useState<MascotState>(() => ({ draw: draw([]), recent: [] }));

  // The window is looked up here rather than at import, so modules that
  // import the mascot package without rendering the launcher (the welcome
  // window, tests) never touch the Tauri window API.
  useEffect(() => {
    const unlisten = getCurrentWebviewWindow().listen("tauri://blur", () => {
      if (!getSettingSync<boolean>("randomMascots")) return;
      setState((previous) => {
        // A blur without a dismiss (the launcher was never shown in
        // between) moves the same character to the front again, which
        // changes nothing.
        const wasShown = getSettingSync<string>("mascotMode") !== "off";
        const recent = wasShown
          ? rememberShown(previous.recent, previous.draw.character)
          : previous.recent;
        return { draw: draw(recent), recent };
      });
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  // A drawn variant the settings no longer allow (NSFW turned off while
  // an NSFW variant is drawn) is replaced by a new draw during this
  // render, and React renders again right away with it. The new draw is
  // kept, so allowing the old variant again changes nothing until the
  // next dismiss.
  let current = state;
  if (!isAllowed(state.draw.variant, showNsfwMascots)) {
    current = { draw: draw(state.recent, showNsfwMascots), recent: state.recent };
    setState(current);
  }

  if (!randomMascots) return { variant: ORIGINAL_MASCOT, draw: null };
  return { variant: current.draw.variant, draw: current.draw };
}
