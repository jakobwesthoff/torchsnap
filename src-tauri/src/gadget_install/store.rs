// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The slice of the settings store that install and uninstall need.
//!
//! Uninstall strips a gadget's settings keys. Going through this trait
//! instead of `tauri_plugin_store::Store` directly lets tests use an
//! in-memory store and lets `setup` hand the real one to commands as
//! plain managed state.

use anyhow::Context;

pub trait SettingsKeys: Send + Sync {
    fn keys(&self) -> Vec<String>;
    fn delete(&self, key: &str);
    fn save(&self) -> anyhow::Result<()>;
}

impl<R: tauri::Runtime> SettingsKeys for tauri_plugin_store::Store<R> {
    fn keys(&self) -> Vec<String> {
        tauri_plugin_store::Store::keys(self)
    }

    fn delete(&self, key: &str) {
        tauri_plugin_store::Store::delete(self, key);
    }

    fn save(&self) -> anyhow::Result<()> {
        tauri_plugin_store::Store::save(self).context("write the settings store to disk")
    }
}

/// Remove every setting the host writes for a gadget and persist the
/// result.
///
/// The host writes exactly two shapes of key per gadget: `enabled.<id>`
/// and `gadgets.<id>.*`. Matching the exact key plus the prefix with
/// its trailing dot keeps gadgets whose ids share a text prefix
/// (`calc` and `calculator`) apart.
pub fn strip_gadget_settings(settings: &dyn SettingsKeys, gadget_id: &str) -> anyhow::Result<()> {
    let enabled_key = format!("enabled.{gadget_id}");
    let prefix = format!("gadgets.{gadget_id}.");
    for key in settings.keys() {
        if key == enabled_key || key.starts_with(&prefix) {
            settings.delete(&key);
        }
    }
    settings
        .save()
        .context("persist settings after removing the gadget's keys")
}

// =========================================================
// In-memory test double
// =========================================================

#[cfg(test)]
pub use memory::MemorySettings;

#[cfg(test)]
mod memory {
    use std::collections::BTreeSet;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::SettingsKeys;

    /// Settings store that keeps key names in memory, counts saves, and
    /// can be told to fail on save.
    #[derive(Default)]
    pub struct MemorySettings {
        keys: Mutex<BTreeSet<String>>,
        saves: AtomicUsize,
        fail_save: bool,
    }

    impl MemorySettings {
        pub fn with_keys(keys: &[&str]) -> Self {
            Self {
                keys: Mutex::new(keys.iter().map(|k| k.to_string()).collect()),
                ..Self::default()
            }
        }

        pub fn failing_save(mut self) -> Self {
            self.fail_save = true;
            self
        }

        pub fn save_count(&self) -> usize {
            self.saves.load(Ordering::SeqCst)
        }
    }

    impl SettingsKeys for MemorySettings {
        fn keys(&self) -> Vec<String> {
            self.keys
                .lock()
                .expect("test settings lock is never poisoned")
                .iter()
                .cloned()
                .collect()
        }

        fn delete(&self, key: &str) {
            self.keys
                .lock()
                .expect("test settings lock is never poisoned")
                .remove(key);
        }

        fn save(&self) -> anyhow::Result<()> {
            self.saves.fetch_add(1, Ordering::SeqCst);
            if self.fail_save {
                anyhow::bail!("disk full");
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_the_enabled_key_and_the_gadget_prefix() {
        let settings = MemorySettings::with_keys(&[
            "enabled.foo",
            "enabled.foobar",
            "gadgets.foo.alpha",
            "gadgets.foo.nested.key",
            "appearance.theme",
        ]);

        strip_gadget_settings(&settings, "foo").expect("strip should succeed");

        assert_eq!(settings.keys(), vec!["appearance.theme", "enabled.foobar"]);
        assert_eq!(settings.save_count(), 1);
    }

    /// A gadget id that is a text prefix of another id must not take
    /// the longer id's settings with it.
    #[test]
    fn keeps_the_keys_of_gadgets_whose_id_extends_the_stripped_one() {
        let settings = MemorySettings::with_keys(&[
            "enabled.calc",
            "enabled.calculator",
            "gadgets.calc.shortcut",
            "gadgets.calculator.history",
        ]);

        strip_gadget_settings(&settings, "calc").expect("strip should succeed");

        assert_eq!(
            settings.keys(),
            vec!["enabled.calculator", "gadgets.calculator.history"]
        );
    }

    #[test]
    fn saves_even_when_nothing_matched() {
        let settings = MemorySettings::with_keys(&["appearance.theme"]);

        strip_gadget_settings(&settings, "foo").expect("strip should succeed");

        assert_eq!(settings.keys(), vec!["appearance.theme"]);
        assert_eq!(settings.save_count(), 1);
    }

    #[test]
    fn propagates_a_failed_save() {
        let settings = MemorySettings::with_keys(&["enabled.foo"]).failing_save();

        let error = strip_gadget_settings(&settings, "foo").expect_err("save failure must surface");

        assert!(format!("{error:#}").contains("disk full"));
    }
}
