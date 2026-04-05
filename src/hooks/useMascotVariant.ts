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

import { useEffect, useRef, useState } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { useSetting } from "./useSetting";
import { selectMascotVariant } from "./useRandomMascot";
import { SnappyHeroSets, isNsfwVariant } from "../mascotVariants";

const appWindow = getCurrentWebviewWindow();

export function useMascotVariant(): { variant: string } {
  const [randomMascots] = useSetting<boolean>("randomMascots");
  const [showNsfwMascots] = useSetting<boolean>("showNsfwMascots");

  // Build a filter predicate from the current NSFW setting. `undefined`
  // when NSFW is allowed (no filtering needed — avoids the array scan).
  const nsfwFilter = showNsfwMascots ? undefined : (v: string) => !isNsfwVariant(v);

  // Initial variant — rolled once on mount respecting both settings.
  const [variant, setVariant] = useState(() =>
    randomMascots ? selectMascotVariant(SnappyHeroSets, nsfwFilter) : "original",
  );

  // Re-roll the mascot on every launcher dismiss so the next variant is
  // already rendered (and its image cached by the browser) before the
  // launcher appears again. The launcher stays mounted between shows, so
  // React renders the new <Mascot> into the hidden DOM and the browser
  // fetches the image while the window is invisible.
  useEffect(() => {
    const unlisten = appWindow.listen("tauri://blur", () => {
      const filter = showNsfwMascots ? undefined : (v: string) => !isNsfwVariant(v);
      setVariant(randomMascots ? selectMascotVariant(SnappyHeroSets, filter) : "original");
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, [randomMascots, showNsfwMascots]);

  // ESLINT: Keep the displayed variant in sync when settings change while
  // the launcher is visible (e.g. user toggles a setting in the settings
  // window). Writing during render is safe — the only reader is the
  // useEffect below, which runs after commit.
  const variantRef = useRef(variant);
  // eslint-disable-next-line react-hooks/refs
  variantRef.current = variant;
  useEffect(() => {
    if (!randomMascots) {
      setVariant("original");
      return;
    }
    // If NSFW was just disabled and the current mascot is NSFW, swap it
    // immediately rather than waiting for the next dismiss.
    if (!showNsfwMascots && isNsfwVariant(variantRef.current)) {
      setVariant(selectMascotVariant(SnappyHeroSets, (v) => !isNsfwVariant(v)));
    }
  }, [randomMascots, showNsfwMascots]);

  return { variant };
}
