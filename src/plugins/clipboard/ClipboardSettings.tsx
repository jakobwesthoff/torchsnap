// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Placeholder settings component for the clipboard plugin.
 *
 * Verifies the end-to-end plugin settings pipeline works. Will be
 * replaced with actual settings controls (retention, content type
 * toggles, excluded apps, etc.) in a follow-up.
 */

import type { PluginSettingsProps } from "../types";

export default function ClipboardSettings(_props: PluginSettingsProps) {
  return (
    <div className="text-text-secondary text-sm">
      Clipboard settings will appear here.
    </div>
  );
}
