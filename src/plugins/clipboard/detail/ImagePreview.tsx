// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { useState, type RefObject, type SyntheticEvent } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import type { ClipboardHistoryEntry } from "../types";

/**
 * Detail preview for image clipboard entries.
 *
 * Displays the captured image centered in the panel with its pixel
 * dimensions shown beneath it once the image has finished loading.
 */
export function ImagePreview({
  detail,
  scrollRef,
}: {
  detail: ClipboardHistoryEntry;
  scrollRef: RefObject<HTMLDivElement | null>;
}) {
  const [dimensions, setDimensions] = useState<{ w: number; h: number } | null>(null);

  const imageFormat = detail.formats.image;
  if (imageFormat?.type !== "asset") return null;

  const handleLoad = (e: SyntheticEvent<HTMLImageElement>) => {
    const img = e.currentTarget;
    setDimensions({ w: img.naturalWidth, h: img.naturalHeight });
  };

  return (
    <div
      ref={scrollRef}
      className="w-[60%] overflow-y-auto p-4 scrollbar-accent flex flex-col items-center justify-center gap-2"
    >
      <img
        src={convertFileSrc(imageFormat.path)}
        alt="Clipboard image"
        className="max-w-full max-h-full object-contain rounded"
        draggable={false}
        onLoad={handleLoad}
      />
      {dimensions && (
        <span className="text-xs text-text-muted select-none">
          {dimensions.w} × {dimensions.h}
        </span>
      )}
    </div>
  );
}
