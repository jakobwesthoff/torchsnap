// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Cross-window settings store singleton.
 *
 * Wraps `tauri-plugin-store` with a global event layer so that writes
 * in one webview (e.g. the settings window) are observed by listeners
 * in every other webview (e.g. the launcher).
 *
 * Each webview gets its own JS `Store` object (separate `rid`), but
 * the underlying Rust store is a singleton per path — so reads after
 * a cross-window write already see the new value. The only missing
 * piece is *notification*, which we solve with a global Tauri event
 * (`settings-changed`).
 *
 * Consumers should not use this module directly — use the
 * `useSetting()` hook instead.
 */

import { load, type Store } from "@tauri-apps/plugin-store";
import { emit, listen } from "@tauri-apps/api/event";

// =========================================================
// Types
// =========================================================

type Listener = (key: string, value: unknown) => void;

interface SettingsChangedPayload {
  key: string;
}

// =========================================================
// Singleton state
// =========================================================

let storePromise: Promise<Store> | null = null;
const listeners = new Set<Listener>();
let eventListenerInstalled = false;

// =========================================================
// Lazy store initialization
// =========================================================

function getStore(): Promise<Store> {
  if (storePromise) {
    return storePromise;
  }

  storePromise = load("settings.json");

  return storePromise;
}

// =========================================================
// Cross-window event listener (installed once per webview)
// =========================================================

function ensureEventListener() {
  if (eventListenerInstalled) {
    return;
  }
  eventListenerInstalled = true;

  listen<SettingsChangedPayload>("settings-changed", async (event) => {
    // Re-read the value from the store. The Rust-side singleton
    // already has the updated data — we just need the new value
    // to pass to subscribers.
    const s = await getStore();
    const value = await s.get<unknown>(event.payload.key);
    notifyListeners(event.payload.key, value);
  });
}

// =========================================================
// Notify all registered listeners
// =========================================================

function notifyListeners(key: string, value: unknown) {
  for (const listener of listeners) {
    listener(key, value);
  }
}

// =========================================================
// Public API
// =========================================================

/**
 * Read a single key from the settings store.
 */
export async function getSetting<T>(key: string): Promise<T | undefined> {
  const s = await getStore();
  return s.get<T>(key);
}

/**
 * Write a single key to the settings store and notify all webviews.
 */
export async function setSetting<T>(key: string, value: T): Promise<void> {
  const s = await getStore();
  await s.set(key, value);
  await s.save();

  // Broadcast globally so every webview (including this one) picks
  // up the change through the same code path.
  await emit("settings-changed", { key } satisfies SettingsChangedPayload);
}

/**
 * Subscribe to setting changes. The callback fires for any key change
 * regardless of which webview originated it.
 *
 * Returns an unsubscribe function.
 */
export function subscribe(listener: Listener): () => void {
  ensureEventListener();
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}
