// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import type { RefObject } from "react";
import type { ClipboardHistoryEntry } from "../types";

/**
 * Detail preview that renders `displayText` in a monospace `<pre>` block.
 *
 * Used for text, HTML, and RTF entries — all formats where the meaningful
 * user-facing content is the derived display text rather than a richer
 * visual representation.
 */
export function DisplayTextPreview({
  detail,
  scrollRef,
}: {
  detail: ClipboardHistoryEntry;
  scrollRef: RefObject<HTMLDivElement | null>;
}) {
  return (
    <div ref={scrollRef} className="w-[60%] overflow-y-auto overflow-x-hidden p-4 scrollbar-accent">
      <pre className="text-sm text-text-secondary whitespace-pre-wrap break-words select-none font-mono leading-relaxed">
        {detail.displayText}
      </pre>
    </div>
  );
}
