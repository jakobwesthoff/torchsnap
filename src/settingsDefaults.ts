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
