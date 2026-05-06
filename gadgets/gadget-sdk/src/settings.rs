// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Typed accessors for the host's `settings::get` interface.
//!
//! The WIT boundary hands gadgets JSON-encoded strings so any
//! TOML-compatible value shape can cross without inventing a
//! dedicated variant. These helpers fold the JSON parse into
//! the same call site where the key is named, so gadget code
//! reads more like direct struct access:
//!
//! ```ignore
//! let verbose: bool = settings::get_or("verbose", false);
//! let greeting: String =
//!     settings::get_or_else("greeting", || "(unset)".into());
//! ```
//!
//! `get` returns `None` for both "unset" and "present but
//! failed to parse into `T`" — gadgets that need to
//! distinguish the two should call the underlying
//! `settings_host::get` directly.

use serde::de::DeserializeOwned;

use crate::settings_host;

/// Fetch a setting and attempt to decode it as `T`.
///
/// Returns `None` when the key is unset OR when the stored
/// JSON doesn't deserialize into `T`. For a hard default that
/// doesn't care about the parse failure distinction, prefer
/// [`get_or`] / [`get_or_else`].
pub fn get<T: DeserializeOwned>(key: &str) -> Option<T> {
    serde_json::from_str(&settings_host::get(key)?).ok()
}

/// Fetch a setting or fall back to `default`.
pub fn get_or<T: DeserializeOwned>(key: &str, default: T) -> T {
    get(key).unwrap_or(default)
}

/// Fetch a setting or compute a fallback lazily.
///
/// Useful when the default is expensive to construct (e.g. a
/// `String` allocation) and the setting is expected to be
/// present in the common case.
pub fn get_or_else<T: DeserializeOwned>(key: &str, f: impl FnOnce() -> T) -> T {
    get(key).unwrap_or_else(f)
}
