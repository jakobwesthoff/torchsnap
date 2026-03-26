// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Emoji picker grid — first plugin custom UI (ADR 0013).
 *
 * Renders emoji results as a 10-column grid instead of the standard
 * vertical result list. Each cell shows the emoji glyph; the
 * shortcode and label are displayed in the footer.
 */

import type { PluginViewProps } from "../types";

// Placeholder — full implementation in step 9.
export default function EmojiGrid(_props: PluginViewProps) {
  return <div className="p-4 text-center text-text-muted">Emoji grid loading…</div>;
}
