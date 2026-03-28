// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Platform detection and OS-aware formatting for keyboard shortcuts.
 *
 * Provides display strings for modifier keys and special keys that match
 * platform conventions (e.g. ⌘ on macOS vs Ctrl on Windows/Linux).
 */

import type { ModifierKey } from "./matching";

// =========================================================
// Platform Detection
// =========================================================

export type Platform = "macos" | "other";

/**
 * Detect the current platform at module load time.
 *
 * Uses the modern `navigator.userAgentData.platform` API when available,
 * falling back to the legacy `navigator.platform` string.
 */
export function detectPlatform(): Platform {
  const ua = navigator as {
    userAgentData?: { platform?: string };
    platform?: string;
  };
  const raw: string = ua.userAgentData?.platform ?? ua.platform ?? "";
  return /mac/i.test(raw) ? "macos" : "other";
}

export const platform: Platform = detectPlatform();

// =========================================================
// Modifier Formatting
// =========================================================

const MODIFIER_SYMBOLS_MACOS: Record<ModifierKey, string> = {
  Meta: "⌘",
  Ctrl: "⌃",
  Shift: "⇧",
  Alt: "⌥",
};

const MODIFIER_LABELS_OTHER: Record<ModifierKey, string> = {
  Meta: "Ctrl",
  Ctrl: "Ctrl",
  Shift: "Shift",
  Alt: "Alt",
};

/**
 * Format a modifier key for display on a given platform.
 *
 * Exported with explicit platform parameter for testability;
 * consumers typically use `formatModifier` which reads the detected platform.
 */
export function formatModifierForPlatform(modifier: ModifierKey, p: Platform): string {
  return p === "macos" ? MODIFIER_SYMBOLS_MACOS[modifier] : MODIFIER_LABELS_OTHER[modifier];
}

export function formatModifier(modifier: ModifierKey): string {
  return formatModifierForPlatform(modifier, platform);
}

// =========================================================
// Key Formatting
// =========================================================

/** Keys with the same display string on all platforms. */
const UNIVERSAL_KEY_MAP: Record<string, string> = {
  ArrowUp: "↑",
  ArrowDown: "↓",
  ArrowLeft: "←",
  ArrowRight: "→",
  Enter: "↵",
  Escape: "Esc",
  Space: "Space",
  " ": "Space",
  Tab: "⇥",
};

/** Keys with platform-specific display strings. */
const PLATFORM_KEY_MAP: Record<string, Record<Platform, string>> = {
  Backspace: { macos: "⌫", other: "Backspace" },
  Delete: { macos: "⌦", other: "Del" },
};

/**
 * Format a key name for display on a given platform.
 *
 * Single lowercase letters are uppercased. Special keys (arrows, Enter, etc.)
 * are mapped to symbols. Everything else is passed through as-is.
 */
export function formatKeyForPlatform(key: string, p: Platform): string {
  if (key in UNIVERSAL_KEY_MAP) {
    return UNIVERSAL_KEY_MAP[key];
  }
  if (key in PLATFORM_KEY_MAP) {
    return PLATFORM_KEY_MAP[key][p];
  }

  // Single lowercase letter → uppercase for display
  if (key.length === 1 && key >= "a" && key <= "z") {
    return key.toUpperCase();
  }

  return key;
}

export function formatKey(key: string): string {
  return formatKeyForPlatform(key, platform);
}
