// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Semi-transparent overlay that displays a mascot variant's alt text
 * as a title-card easter egg. Fades in and out via a CSS opacity
 * transition.
 *
 * Not mounted at all until the first trigger — avoids a brief opacity
 * flash on initial render. Once shown, stays in the DOM so the
 * fade-out transition can play.
 */

import { useRef } from "react";
import { cn } from "../lib/cn";
import { getMascotAlt } from "../mascotVariants";

interface MascotInfoOverlayProps {
  variant: string;
  visible: boolean;
  onDismiss: () => void;
}

export function MascotInfoOverlay({ variant, visible, onDismiss }: MascotInfoOverlayProps) {
  // Track whether the overlay has ever been shown. Once true, the
  // element stays mounted so the fade-out transition can play.
  const hasBeenVisible = useRef(false);
  if (visible) hasBeenVisible.current = true;

  if (!hasBeenVisible.current) return null;

  return (
    <div
      className={cn(
        "absolute inset-0 z-[5] flex items-center justify-center bg-surface/90 rounded-2xl transition-opacity duration-150",
        visible ? "opacity-100" : "opacity-0 pointer-events-none",
      )}
      onClick={onDismiss}
    >
      <p className="px-8 text-center text-sm italic text-text-secondary">{getMascotAlt(variant)}</p>
    </div>
  );
}
