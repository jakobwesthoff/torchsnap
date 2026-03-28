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
 * ## Pre-loading
 *
 * The store must be loaded before React mounts so that `useSetting`
 * can read values synchronously on the very first render — no loading
 * states, no default values, no `ready` flag. Both entry points
 * (`launcher/main.tsx`, `settings/main.tsx`) call `await initStore()`
 * before `createRoot().render()`. After that, `getSettingSync()` is
 * safe to call from any render path.
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

/** The loaded store instance. `null` until `initStore()` completes. */
let store: Store | null = null;

/**
 * In-memory mirror of all store entries, populated by `initStore()`
 * and kept in sync by the cross-window event listener. This enables
 * synchronous reads from React render paths.
 */
const cache = new Map<string, unknown>();

const listeners = new Set<Listener>();

// =========================================================
// Store initialization
// =========================================================

/**
 * Eagerly load the store and populate the synchronous cache.
 *
 * Must be awaited once per webview before React mounts. Subsequent
 * calls are idempotent and return immediately.
 */
export async function initStore(): Promise<void> {
  if (store) {
    return;
  }

  store = await load("settings.json");

  // Populate the synchronous cache with all current entries so that
  // getSettingSync() works from the very first render.
  const entries = await store.entries();
  for (const [key, value] of entries) {
    cache.set(key, value);
  }

  // Install the cross-window event listener. When any webview writes
  // a setting, all webviews (including the originator) re-read the
  // value and update their caches + subscribers.
  const unlisten = await listen<SettingsChangedPayload>(
    "settings-changed",
    async (event) => {
      const value = await store!.get<unknown>(event.payload.key);
      cache.set(event.payload.key, value);
      notifyListeners(event.payload.key, value);
    },
  );

  // During Vite HMR the module is re-evaluated from scratch, resetting
  // all module-level state (`store` goes back to `null`). Without
  // cleanup the old listener would stay registered while `initStore`
  // runs again and adds a second one. `import.meta.hot.dispose` runs
  // just before the old module is discarded, giving us a chance to
  // remove the stale listener.
  if (import.meta.hot) {
    import.meta.hot.dispose(() => {
      unlisten();
      store = null;
    });
  }
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
 * Read a setting synchronously from the in-memory cache.
 *
 * Only safe to call after `initStore()` has completed. Returns
 * the value cast to `T`. The backend initializes all defaults
 * at startup, so every expected key is guaranteed to be present.
 */
export function getSettingSync<T>(key: string): T {
  return cache.get(key) as T;
}

/**
 * Write a single key to the settings store and notify all webviews.
 */
export async function setSetting<T>(key: string, value: T): Promise<void> {
  if (!store) {
    throw new Error("setSetting called before initStore()");
  }

  await store.set(key, value);
  await store.save();

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
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}
