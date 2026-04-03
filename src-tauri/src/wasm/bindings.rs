// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// WASM Component Bindings
//
// Generates typed Rust bindings from the WIT definitions via
// `wasmtime::component::bindgen!`. This produces:
//
// - A `Plugin` struct with methods to call guest exports
//   (e.g., `call_enable`, `call_entries`, `call_execute`)
// - Traits for each host import interface that we implement
//   on the store state (e.g., `logging::Host`)
// - Typed records/enums mirroring the WIT definitions
//
// This module also provides `From` conversions between the
// generated types and the native `search::types` used by
// the rest of the application.
// =========================================================

wasmtime::component::bindgen!({
    path: "../wit",
    world: "plugin",
});

// =========================================================
// Type Conversions: WIT types → native types
// =========================================================

use crate::search::types as native;
use torchsnap::plugin::types as wit;

impl From<wit::EntryIcon> for native::EntryIcon {
    fn from(icon: wit::EntryIcon) -> Self {
        match icon {
            wit::EntryIcon::HeroIcon(name) => native::EntryIcon::HeroIcon(name),
            wit::EntryIcon::DataUrl(data) => native::EntryIcon::DataUrl(data),
            wit::EntryIcon::AssetIcon(path) => native::EntryIcon::AssetIcon(path),
            wit::EntryIcon::Emoji(emoji) => native::EntryIcon::Emoji(emoji),
        }
    }
}

impl From<wit::ActionId> for native::ActionId {
    fn from(id: wit::ActionId) -> Self {
        match id {
            wit::ActionId::Open => native::ActionId::Open,
            wit::ActionId::Copy => native::ActionId::Copy,
            wit::ActionId::Reveal => native::ActionId::Reveal,
            wit::ActionId::OpenWith => native::ActionId::OpenWith,
            wit::ActionId::Delete => native::ActionId::Delete,
            wit::ActionId::Custom(s) => native::ActionId::Custom(s),
        }
    }
}

impl From<native::ActionId> for wit::ActionId {
    fn from(id: native::ActionId) -> Self {
        match id {
            native::ActionId::Open => wit::ActionId::Open,
            native::ActionId::Copy => wit::ActionId::Copy,
            native::ActionId::Reveal => wit::ActionId::Reveal,
            native::ActionId::OpenWith => wit::ActionId::OpenWith,
            native::ActionId::Delete => wit::ActionId::Delete,
            native::ActionId::Custom(s) => wit::ActionId::Custom(s),
        }
    }
}

impl From<wit::Action> for native::Action {
    fn from(action: wit::Action) -> Self {
        native::Action {
            id: action.id.into(),
            label: action.label,
            // Keybindings are host-managed — plugins don't set them.
            keybinding: None,
        }
    }
}

impl From<wit::CatalogEntry> for native::CatalogEntry {
    fn from(entry: wit::CatalogEntry) -> Self {
        native::CatalogEntry {
            id: entry.id,
            title: entry.title,
            subtitle: entry.subtitle,
            icon: entry.icon.map(Into::into),
            keywords: entry.keywords,
            actions: entry.actions.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<wit::PostAction> for native::PostAction {
    fn from(action: wit::PostAction) -> Self {
        match action {
            wit::PostAction::Nothing => native::PostAction::Nothing,
            wit::PostAction::Dismiss => native::PostAction::Dismiss,
            wit::PostAction::KeepOpen => native::PostAction::KeepOpen,
        }
    }
}
