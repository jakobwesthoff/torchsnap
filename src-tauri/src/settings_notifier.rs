// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Settings Notifier
//
// Reactive settings change propagation. When the frontend
// writes a setting via the store, a `settings-changed` Tauri
// event fires. The caller reads the new value from the store
// and calls `notify(key, value)` on the notifier. Subscribers
// receive the update through `tokio::sync::watch` channels.
//
// `SettingsNotifier` is the app-wide layer that manages watch
// channels per full key (e.g., `frecency.enabled`). Used by
// non-plugin subsystems like FrecencyStore, control socket,
// and WebsiteMetadataService.
// =========================================================

use std::collections::HashMap;
use std::sync::Mutex;

use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::sync::watch;

// =========================================================
// SettingsWatch — typed receiver wrapper
// =========================================================

/// A typed wrapper around `watch::Receiver<Value>` that
/// deserializes the stored `serde_json::Value` into `T` on
/// each read.
///
/// Consumers never deal with raw JSON — they get `T` directly.
pub struct SettingsWatch<T> {
    rx: watch::Receiver<Value>,
    _marker: std::marker::PhantomData<T>,
}

impl<T: DeserializeOwned> SettingsWatch<T> {
    /// Read the current value, deserializing from the stored JSON.
    ///
    /// # Panics
    ///
    /// Panics if the stored value cannot be deserialized into `T`.
    /// This should not happen if defaults were initialized correctly
    /// via `SettingsInit`.
    pub fn get(&self) -> T {
        let value = self.rx.borrow();
        serde_json::from_value(value.clone()).expect("settings watch value deserializes to T")
    }

    /// Wait for the value to change, then return the new value.
    ///
    /// Returns `None` if the sender was dropped (notifier shut down).
    pub async fn changed(&mut self) -> Option<T> {
        self.rx.changed().await.ok()?;
        Some(self.get())
    }

    /// Block the current thread until the value changes, then return
    /// the new value. Intended for use in dedicated background threads
    /// (e.g., the retention cleanup thread) that are not running on
    /// the Tokio runtime.
    ///
    /// Returns `None` if the sender was dropped (notifier shut down).
    pub fn blocking_changed(&mut self) -> Option<T> {
        tauri::async_runtime::block_on(self.rx.changed()).ok()?;
        Some(self.get())
    }
}

impl<T> Clone for SettingsWatch<T> {
    fn clone(&self) -> Self {
        Self {
            rx: self.rx.clone(),
            _marker: std::marker::PhantomData,
        }
    }
}

// =========================================================
// SettingsNotifier — app-wide channel manager
// =========================================================

/// App-wide settings change notifier.
///
/// Manages one `watch` channel per observed key. The Tauri
/// event listener calls `notify()` when a setting changes;
/// subscribers hold `SettingsWatch<T>` receivers.
///
/// The notifier itself does not hold a reference to the
/// settings store — callers provide values when subscribing
/// (`watch_with_initial`) or notifying (`notify`). This keeps
/// the core free of Tauri runtime dependencies for testability.
pub struct SettingsNotifier {
    /// Map from full key to the watch sender. Each sender
    /// holds the latest `serde_json::Value` for that key.
    channels: Mutex<HashMap<String, watch::Sender<Value>>>,
}

impl SettingsNotifier {
    pub fn new() -> Self {
        Self {
            channels: Mutex::new(HashMap::new()),
        }
    }

    /// Subscribe to changes for a specific key, seeding the
    /// channel with the given initial value.
    ///
    /// If a channel for this key already exists, the initial
    /// value is ignored and a new receiver is returned on the
    /// existing channel.
    pub fn watch_with_initial<T: DeserializeOwned>(
        &self,
        key: &str,
        initial: Value,
    ) -> SettingsWatch<T> {
        let mut channels = self.channels.lock().expect("channels not poisoned");

        let rx = if let Some(tx) = channels.get(key) {
            tx.subscribe()
        } else {
            let (tx, rx) = watch::channel(initial);
            channels.insert(key.to_string(), tx);
            rx
        };

        SettingsWatch {
            rx,
            _marker: std::marker::PhantomData,
        }
    }

    /// Push a new value for a key to all subscribers.
    ///
    /// Keys without active subscribers are silently ignored.
    pub fn notify(&self, key: &str, value: Value) {
        let channels = self.channels.lock().expect("channels not poisoned");

        if let Some(tx) = channels.get(key) {
            // send() only fails if all receivers are dropped,
            // which is fine — the channel will be recreated if
            // someone watches again.
            let _ = tx.send(value);
        }
    }
}

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watch_returns_initial_value() {
        let notifier = SettingsNotifier::new();
        let watch: SettingsWatch<u32> = notifier.watch_with_initial("retention", Value::from(30));
        assert_eq!(watch.get(), 30);
    }

    #[test]
    fn notify_updates_value() {
        let notifier = SettingsNotifier::new();
        let watch: SettingsWatch<u32> = notifier.watch_with_initial("retention", Value::from(30));

        notifier.notify("retention", Value::from(60));
        assert_eq!(watch.get(), 60);
    }

    #[test]
    fn notify_unknown_key_is_noop() {
        let notifier = SettingsNotifier::new();
        let watch: SettingsWatch<bool> = notifier.watch_with_initial("enabled", Value::from(true));

        // Notifying a different key should not affect the watched key.
        notifier.notify("other_key", Value::from(false));
        assert!(watch.get());
    }

    #[test]
    fn multiple_receivers_same_key() {
        let notifier = SettingsNotifier::new();
        let w1: SettingsWatch<u32> = notifier.watch_with_initial("retention", Value::from(30));
        let w2: SettingsWatch<u32> = notifier.watch_with_initial("retention", Value::from(999));

        // w2 should join the existing channel, ignoring the new initial.
        assert_eq!(w1.get(), 30);
        assert_eq!(w2.get(), 30);

        notifier.notify("retention", Value::from(7));
        assert_eq!(w1.get(), 7);
        assert_eq!(w2.get(), 7);
    }

    #[test]
    fn cloned_receiver_sees_updates() {
        let notifier = SettingsNotifier::new();
        let w1: SettingsWatch<String> = notifier.watch_with_initial("name", Value::from("alice"));
        let w2 = w1.clone();

        notifier.notify("name", Value::from("bob"));
        assert_eq!(w1.get(), "bob");
        assert_eq!(w2.get(), "bob");
    }

    #[test]
    fn independent_keys_do_not_interfere() {
        let notifier = SettingsNotifier::new();
        let w_a: SettingsWatch<u32> = notifier.watch_with_initial("a", Value::from(1));
        let w_b: SettingsWatch<u32> = notifier.watch_with_initial("b", Value::from(2));

        notifier.notify("a", Value::from(10));
        assert_eq!(w_a.get(), 10);
        assert_eq!(w_b.get(), 2);
    }

    #[tokio::test]
    async fn changed_returns_new_value() {
        let notifier = SettingsNotifier::new();
        let mut watch: SettingsWatch<u32> =
            notifier.watch_with_initial("retention", Value::from(30));

        notifier.notify("retention", Value::from(60));
        let result = watch.changed().await;
        assert_eq!(result, Some(60));
    }

    #[tokio::test]
    async fn changed_returns_none_when_sender_dropped() {
        let notifier = SettingsNotifier::new();
        let mut watch: SettingsWatch<u32> =
            notifier.watch_with_initial("retention", Value::from(30));

        // Drop the notifier (and with it the sender).
        drop(notifier);

        let result = watch.changed().await;
        assert_eq!(result, None);
    }
}
