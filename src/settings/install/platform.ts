// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { platform } from "@tauri-apps/plugin-os";
import type { Platform } from "./permissionModel";

/**
 * The operating system the app runs on, for setting aside permission
 * paths that only apply elsewhere. The OS plugin is unavailable outside
 * the Tauri runtime (tests), where macOS, the shipping platform, is
 * assumed.
 */
export function currentPlatform(): Platform {
  try {
    const name = platform();
    return name === "windows" || name === "linux" ? name : "macos";
  } catch {
    return "macos";
  }
}
