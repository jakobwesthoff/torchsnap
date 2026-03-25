// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Default values for all settings keys.
 *
 * Centralized here so that every `useSetting()` call site uses the
 * same fallback. If a key has never been written to the store, the
 * hook returns the default from this map.
 */

export const SETTINGS_DEFAULTS = {
  globalShortcut: "CmdOrCtrl+Shift+Space",
  showMascot: true,
} satisfies Record<string, unknown>;
