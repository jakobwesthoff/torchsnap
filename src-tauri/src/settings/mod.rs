// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

pub mod coalescing_dispatcher;
pub mod notifier;

// =========================================================
// Settings Infrastructure
//
// Two types serve different phases of the settings lifecycle:
//
// - SettingsInit: used once at startup to ensure default values
//   exist in the store. Works for both global app settings and
//   per-plugin settings via a key prefix.
//
// - GadgetSettings: runtime read wrapper injected into plugins
//   during setup. Scopes all reads to `plugins.<id>.` so plugins
//   can only access their own namespace.
// =========================================================

use std::collections::HashMap;
use std::sync::Arc;

use serde::de::DeserializeOwned;
use serde_json::Value;
use tauri_plugin_store::Store;

// =========================================================
// SettingsInit
// =========================================================

/// Builder for ensuring default setting values exist in the store.
///
/// Load current values with `from_store`, chain `ensure()` calls to
/// fill in any missing keys, then `apply()` to write back. Keys that
/// already have a value are left untouched.
///
/// ```ignore
/// SettingsInit::from_store(&store, "gadgets.clipboard-manager.")
///     .ensure("pollingInterval", 500)
///     .ensure("retentionDays", 30)
///     .apply(&store, "gadgets.clipboard-manager.");
/// ```
pub struct SettingsInit {
    entries: HashMap<String, Value>,
}

impl SettingsInit {
    /// Load all keys matching `prefix` from the store.
    ///
    /// The prefix is stripped from keys in the returned map, so
    /// callers work with short names (e.g., `"pollingInterval"`
    /// rather than `"gadgets.clipboard-manager.pollingInterval"`).
    pub fn from_store<R: tauri::Runtime>(store: &Arc<Store<R>>, prefix: &str) -> Self {
        let mut entries = HashMap::new();
        for (key, value) in store.entries() {
            if let Some(short_key) = key.strip_prefix(prefix) {
                entries.insert(short_key.to_string(), value);
            }
        }
        Self { entries }
    }

    /// Set `key` to `default` only if it does not already exist.
    ///
    /// This is the primary building block for declaring defaults.
    /// Chain multiple calls to fill in all expected keys:
    ///
    /// ```ignore
    /// settings
    ///     .ensure("retentionDays", 30)
    ///     .ensure("trackImages", true)
    /// ```
    pub fn ensure(mut self, key: &str, default: impl Into<Value>) -> Self {
        self.entries
            .entry(key.to_string())
            .or_insert_with(|| default.into());
        self
    }

    /// Remove a key from the initializer, returning its value if present.
    ///
    /// The documented primitive for settings-key migrations: pull the
    /// value out under its old name, then re-insert it via `ensure`
    /// under the new name.
    ///
    /// ```ignore
    /// if let Some(old) = settings.remove("pollInterval") {
    ///     settings = settings.ensure("pollingInterval", old);
    /// }
    /// ```
    #[allow(dead_code)]
    pub fn remove(&mut self, key: &str) -> Option<Value> {
        self.entries.remove(key)
    }

    /// Write all entries back to the store under the given prefix.
    ///
    /// Each key in the map is prefixed before writing, so
    /// `"retentionDays"` becomes `"{prefix}retentionDays"` in the
    /// store. The store is saved to disk after all writes.
    pub fn apply<R: tauri::Runtime>(self, store: &Arc<Store<R>>, prefix: &str) {
        for (key, value) in self.entries {
            let full_key = format!("{prefix}{key}");
            // Only write keys that don't already exist or whose value
            // changed. Since `from_store` loaded the current state and
            // `ensure` only fills gaps, most keys will already match —
            // but migrations via `remove` + `ensure` can produce new
            // values that need writing.
            store.set(full_key, value);
        }
        // Persist to disk so the frontend sees the values immediately
        // when its store instance loads the same file.
        let _ = store.save();
    }
}

// =========================================================
// GadgetSettings
// =========================================================

/// Scoped read-only access to a plugin's settings namespace.
///
/// Wraps the app-wide settings store with a fixed key prefix
/// (`gadgets.<id>.`) so plugins can read their own settings
/// without knowing the full key path.
///
/// Injected into `Plugin::enable()` (via `GadgetContext`)
/// after defaults have been initialized via `SettingsInit`.
pub struct GadgetSettings<R: tauri::Runtime = tauri::Wry> {
    store: Arc<Store<R>>,
    prefix: String,
}

// Manual `Clone` impl: the derive would require `R: Clone`, but
// we only need `Arc::clone` and `String::clone` — neither depends
// on `R` being `Clone`.
impl<R: tauri::Runtime> Clone for GadgetSettings<R> {
    fn clone(&self) -> Self {
        Self {
            store: Arc::clone(&self.store),
            prefix: self.prefix.clone(),
        }
    }
}

impl<R: tauri::Runtime> GadgetSettings<R> {
    /// Create a new scoped settings reader for the given plugin ID.
    pub fn new(store: Arc<Store<R>>, plugin_id: &str) -> Self {
        Self {
            store,
            prefix: format!("gadgets.{plugin_id}."),
        }
    }

    /// Read a setting value, deserializing from the stored JSON.
    ///
    /// Returns `None` if the key does not exist. Panics should not
    /// occur if defaults were initialized correctly via `SettingsInit`.
    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        let full_key = format!("{}{}", self.prefix, key);
        self.store
            .get(&full_key)
            .and_then(|v| serde_json::from_value(v).ok())
    }

    /// Read the raw JSON-encoded value for `key` as a string.
    ///
    /// Returns `None` if the key does not exist. Used by the WASM
    /// plugin bridge — WIT has no opaque JSON value type, so the
    /// host hands the JSON across the boundary as a string and the
    /// guest parses it on its side.
    ///
    /// Equivalent to `serde_json::to_string(&self.store.get(key))`,
    /// but skips the round-trip through `serde_json::Value` if the
    /// key is missing.
    pub fn get_raw(&self, key: &str) -> Option<String> {
        let full_key = format!("{}{}", self.prefix, key);
        self.store
            .get(&full_key)
            .and_then(|v| serde_json::to_string(&v).ok())
    }
}
