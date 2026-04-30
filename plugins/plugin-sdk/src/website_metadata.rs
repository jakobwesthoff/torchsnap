// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Ergonomic wrappers around the host's `website-metadata` interface.
//!
//! The raw WIT exposes
//! `lookup(domain, mode) -> Result<LookupResult, WebsiteMetadataError>`
//! where `LookupResult` is a four-variant enum (`Hit` /
//! `ReachableNoData` / `Unreachable` / `Pending`) and the error
//! type carries two variants that are programmer bugs rather than
//! recoverable runtime conditions (`PermissionDenied` is a missing
//! manifest grant, `InvalidDomain` is bad input). Real call sites
//! collapse all of that to "do I have something to show, yes/no/
//! ask-again", so this module wraps the WIT into:
//!
//! - A flat [`Metadata`] enum (`Found` / `Empty` / `Pending`) where
//!   "no data" and "unreachable" merge into `Empty`.
//! - [`lookup_cached`] / [`lookup_blocking`] returning [`Metadata`],
//!   with the WIT's two error variants logged via the host's
//!   `logging` interface and folded into `Empty`.
//! - [`favicon_or`], the one-line idiom for the most common case:
//!   "give me the cached favicon, or this fallback icon."
//!
//! The flatten loses the distinction between `ReachableNoData` and
//! `Unreachable`, and discards the underlying error string. Plugins
//! that need either can call [`website_metadata_host::lookup`]
//! directly — the raw module stays accessible through the SDK
//! prelude under that name.
//!
//! # Example
//!
//! ```ignore
//! use torchsnap_plugin_sdk::prelude::*;
//!
//! let icon = website_metadata::favicon_or(
//!     "example.com",
//!     EntryIcon::HeroIcon("globe-alt".into()),
//! );
//! ```

use crate::website_metadata_host::{
    self, LookupMode, LookupResult, WebsiteMetadataError,
};
use crate::EntryIcon;

pub use crate::website_metadata_host::CacheEntry;

// =========================================================
// Metadata — flattened lookup result
// =========================================================

/// Outcome of a metadata lookup.
///
/// Collapses the WIT's `LookupResult` (`Hit` / `ReachableNoData` /
/// `Unreachable` / `Pending`) and `WebsiteMetadataError` variants
/// into the three states a typical plugin actually distinguishes
/// at the call site.
pub enum Metadata {
    /// Host returned cached metadata for the domain.
    Found(CacheEntry),
    /// Host has no usable data for this domain. Merges the WIT's
    /// `ReachableNoData` and `Unreachable` cases — plugins that
    /// need to tell them apart should call
    /// [`website_metadata_host::lookup`] directly. Errors
    /// (`PermissionDenied`, `InvalidDomain`) also collapse here
    /// after being logged, since they are programmer bugs that
    /// won't resolve by retrying.
    Empty,
    /// Cold cache, background fetch scheduled. Only ever returned
    /// from [`lookup_cached`]; [`lookup_blocking`] waits for the
    /// fetch and never returns this. The next call after the
    /// fetch completes will return `Found` (or `Empty`).
    Pending,
}

// =========================================================
// Lookup helpers
// =========================================================

/// Cache-first lookup that never blocks the calling thread.
///
/// On a cold cache the host schedules a background fetch and
/// returns [`Metadata::Pending`]; subsequent calls will see the
/// populated cache. Use this on hot paths like per-keystroke
/// `search()` where a network round-trip is unacceptable.
pub fn lookup_cached(domain: &str) -> Metadata {
    dispatch(domain, LookupMode::Cached)
}

/// Cache-first lookup that blocks until the host has an answer.
///
/// On a cold cache the host fetches synchronously (or coalesces
/// with any in-flight request for the same domain) before
/// returning. Never returns [`Metadata::Pending`]. Use only when
/// the result is load-bearing for the current render — `search()`
/// callers should prefer [`lookup_cached`].
pub fn lookup_blocking(domain: &str) -> Metadata {
    dispatch(domain, LookupMode::Blocking)
}

/// Cache-first favicon lookup with caller-supplied fallback.
///
/// Implicitly uses [`LookupMode::Cached`] — never blocks. Returns
/// the cached favicon on a hit, or `fallback` for any other
/// outcome (cold cache, no data, unreachable, error). The
/// canonical idiom for plugins that just want a favicon-or-
/// default for a domain in their result list.
pub fn favicon_or(domain: &str, fallback: EntryIcon) -> EntryIcon {
    match lookup_cached(domain) {
        Metadata::Found(entry) => entry.favicon,
        Metadata::Empty | Metadata::Pending => fallback,
    }
}

// =========================================================
// Internal dispatch
//
// Both error variants are programmer bugs that cannot recover
// at runtime — `PermissionDenied` means the plugin's manifest
// is missing the grant, `InvalidDomain` means the plugin
// passed a malformed domain string. We log loudly via the
// host so the bug surfaces in the host's log view, then
// collapse to `Empty` so call sites don't have to plumb error
// handling through every result construction.
// =========================================================
fn dispatch(domain: &str, mode: LookupMode) -> Metadata {
    match website_metadata_host::lookup(domain, mode) {
        Ok(LookupResult::Hit(entry)) => Metadata::Found(entry),
        Ok(LookupResult::ReachableNoData) | Ok(LookupResult::Unreachable) => Metadata::Empty,
        Ok(LookupResult::Pending) => Metadata::Pending,
        Err(WebsiteMetadataError::PermissionDenied(reason)) => {
            crate::log_error!(
                "website-metadata permission denied — add `website-metadata = true` to [permissions]",
                "domain" => domain,
                "reason" => reason,
            );
            Metadata::Empty
        }
        Err(WebsiteMetadataError::InvalidDomain(reason)) => {
            crate::log_warn!(
                "website-metadata rejected domain as invalid",
                "domain" => domain,
                "reason" => reason,
            );
            Metadata::Empty
        }
    }
}
