// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! # Torchsnap Plugin SDK
//!
//! This crate owns the `wit_bindgen::generate!` invocation for
//! the Torchsnap plugin world so that downstream plugin crates
//! don't each pay for rebuilding identical bindings. A plugin
//! author writes:
//!
//! ```ignore
//! use torchsnap_plugin_sdk::prelude::*;
//!
//! struct MyPlugin;
//! torchsnap_plugin_sdk::define_plugin!(MyPlugin);
//!
//! impl LifecycleGuest for MyPlugin { /* ... */ }
//! impl SearchGuest for MyPlugin { /* ... */ }
//! // MessagingGuest / TasksGuest via `impl_noop_messaging!` /
//! // `impl_noop_tasks!` when the plugin doesn't use those.
//! ```
//!
//! ## Cross-crate macro mechanics
//!
//! The `wit_bindgen::generate!` macro emits an `export!` macro
//! alongside the trait bindings. Two options are non-default
//! here:
//!
//! - `pub_export_macro: true` — emits `#[macro_export]` and a
//!   `pub use` re-export so downstream crates can reach the
//!   macro through `torchsnap_plugin_sdk::export!`.
//! - `default_bindings_module: "::torchsnap_plugin_sdk"` — sets
//!   the absolute path baked into the generated macro's
//!   recursive self-call. Without this, the recursive call
//!   resolves against the *caller's* crate root and fails to
//!   find the host bindings module.
//!
//! Both come from the same upstream concern: wit-bindgen's
//! default mode assumes the bindings and the `export!` live in
//! the same crate.

wit_bindgen::generate!({
    path: "wit",
    world: "gadget",
    pub_export_macro: true,
    default_bindings_module: "::torchsnap_plugin_sdk",
});

pub mod command;
pub mod logging;
pub mod messaging;
pub mod settings;
pub mod sql;
pub mod website_metadata;

// =========================================================
// Flat trait re-exports
//
// The `wit_bindgen`-generated traits live under
// `exports::torchsnap::plugin::<iface>::Guest`, which makes
// plugin `lib.rs` files drown in `use exports::...` lines. We
// re-export them flatly so plugins only need a single
// `use torchsnap_plugin_sdk::prelude::*;`.
// =========================================================
pub use exports::torchsnap::gadget::lifecycle::Guest as LifecycleGuest;
pub use exports::torchsnap::gadget::messaging::Guest as MessagingGuest;
pub use exports::torchsnap::gadget::search::Guest as SearchGuest;
pub use exports::torchsnap::gadget::tasks::Guest as TasksGuest;

// =========================================================
// WIT-generated record / variant re-exports
//
// Plugins work with `SearchResponse`, `ScoredEntry`,
// `CatalogEntry`, … constantly; surfacing them at the crate
// root keeps per-plugin `use` blocks short.
// =========================================================
pub use exports::torchsnap::gadget::search::{
    Action, ActionId, CatalogEntry, EntryIcon, Guest as _SearchGuest, PostAction, ScoredEntry,
    SearchResponse, ViewResponse,
};

// =========================================================
// Host import re-exports
//
// The generated import modules live deep under
// `torchsnap::plugin::<iface>`; flat aliases shorten plugin
// call sites and keep the SDK as the single point of contact
// between plugin code and the WIT boundary.
//
// `settings_host` is renamed to avoid shadowing the SDK's
// higher-level `settings` helper module. The same applies to
// `logging_host`.
// =========================================================
pub use torchsnap::gadget::{
    assets, clipboard, frecency, fs, http, opener, paths, platform,
};
pub use torchsnap::gadget::command as command_host;
pub use torchsnap::gadget::logging as logging_host;
pub use torchsnap::gadget::settings as settings_host;
pub use torchsnap::gadget::website_metadata as website_metadata_host;

pub mod prelude {
    //! Common glob import for plugin authors.
    //!
    //! `use torchsnap_plugin_sdk::prelude::*;` pulls in the
    //! four guest traits, the search record / variant types
    //! plugins work with every file, the SDK's helper
    //! modules, and the `define_plugin!` / `impl_noop_*!`
    //! macros plugins use to wire themselves up. Host
    //! import modules (`assets`, `clipboard`, `frecency`,
    //! `http`, `opener`) are also surfaced so plugin code
    //! can call them without an extra `use`.
    pub use super::{LifecycleGuest, MessagingGuest, SearchGuest, TasksGuest};
    pub use super::{
        Action, ActionId, CatalogEntry, EntryIcon, PostAction, ScoredEntry, SearchResponse,
        ViewResponse,
    };
    pub use super::{command, logging, messaging, settings, sql, website_metadata};
    pub use super::{
        assets, clipboard, frecency, fs, http, opener, paths, platform, website_metadata_host,
    };
    // Macros re-exported through the prelude so a single
    // `use torchsnap_plugin_sdk::prelude::*;` is enough to
    // write a minimal plugin.
    pub use super::{define_plugin, impl_noop_messaging, impl_noop_tasks};
}

// =========================================================
// define_plugin!
//
// One-liner replacement for the `export!(MyPlugin);` call
// each plugin used to hand-write. Expands to the
// `wit_bindgen`-generated `export!` macro — which is itself
// re-exported through this crate thanks to
// `pub_export_macro: true` in `generate!` above.
// =========================================================

/// Register a plugin type as the WASM component implementation.
///
/// Emits the component-model FFI shims for every guest export
/// in the `plugin` world, binding them to trait impls on the
/// given type. Call this once, at the top of your plugin's
/// `lib.rs`, after declaring the plugin struct.
///
/// The argument is an identifier (a plain type name like
/// `MyPlugin`), matching the upstream `wit_bindgen::export!`
/// signature this macro delegates to.
#[macro_export]
macro_rules! define_plugin {
    ($plugin:ident) => {
        $crate::export!($plugin);
    };
}

// =========================================================
// Noop guest-export macros
//
// The `plugin` world mandates `messaging` and `tasks`
// exports even when the plugin declares no RPC methods and
// no `[[tasks]]`. These macros fold the stub into a single
// line; misrouted calls still surface in host logs because
// the stub returns an error rather than silently succeeding.
// =========================================================

/// Stub `MessagingGuest` for plugins that don't implement RPC.
#[macro_export]
macro_rules! impl_noop_messaging {
    ($t:ty) => {
        impl $crate::MessagingGuest for $t {
            fn handle_message(method: String, _payload: String) -> Result<String, String> {
                Err(format!("plugin does not handle messages: {method}"))
            }
        }
    };
}

/// Stub `TasksGuest` for plugins that declare no scheduled tasks.
#[macro_export]
macro_rules! impl_noop_tasks {
    ($t:ty) => {
        impl $crate::TasksGuest for $t {
            fn run_task(task_id: String) -> Result<(), String> {
                Err(format!("plugin declares no scheduled tasks: {task_id}"))
            }
        }
    };
}
