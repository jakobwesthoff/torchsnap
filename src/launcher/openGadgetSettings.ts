// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { command } from "../lib/command";

/**
 * `openSettings()` for gadget views: open the Settings window on the
 * gadget's own section, then close the launcher. This is the same
 * effect as a gadget returning the `open-settings` post-action from
 * `execute()`, where the host opens the window and the launcher
 * dismisses on the forwarded `Dismiss`.
 *
 * The launcher passes the gadget id of the view that asked, so a view
 * can only open its own gadget's section. When opening fails, the
 * launcher stays open and the error reaches the caller.
 */
export async function openGadgetSettings(gadgetId: string, dismiss: () => void): Promise<void> {
  await command("gadget_open_settings", { gadgetId });
  dismiss();
}
