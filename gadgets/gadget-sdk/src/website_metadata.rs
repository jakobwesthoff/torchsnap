// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Ergonomic wrappers around the host's `website-metadata` interface.
//!
//! The raw WIT exposes
//! `lookup(domain, mode) -> Result<LookupResult, WebsiteMetadataError>`
//! where the error type carries two variants that are programmer
//! bugs rather than recoverable runtime conditions
//! (`PermissionDenied` is a missing manifest grant, `InvalidDomain`
//! is bad input). The double-layer match plus per-call-site error
//! handling adds noise without adding expressive power, so this
//! module wraps the WIT into:
//!
//! - A flat [`Metadata`] enum that mirrors the WIT's `LookupResult`
//!   variants 1:1 (`Found` / `NoData` / `Unreachable` / `Pending`)
//!   minus the outer `Result` layer.
//! - [`lookup_cached`] / [`lookup_blocking`] returning [`Metadata`],
//!   with the WIT's two error variants logged via the host's
//!   `logging` interface and folded into `Unreachable` (errors
//!   are not "the host reached the domain," so `Unreachable` is
//!   the closer fold than `NoData`).
//! - [`favicon_or`], the one-line idiom for the most common case:
//!   "give me the cached favicon, or this fallback icon."
//!
//! The error string is discarded after logging. Gadgets that need
//! to surface it should call [`website_metadata_host::lookup`]
//! directly — the raw module stays accessible through the SDK
//! prelude under that name.
//!
//! # Example
//!
//! ```ignore
//! use torchsnap_gadget_sdk::prelude::*;
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
/// Mirrors the WIT's `LookupResult` variants 1:1 minus the
/// outer `Result` / `WebsiteMetadataError` layer, which the
/// wrapper logs and folds into `Unreachable`.
pub enum Metadata {
    /// Host returned cached metadata for the domain.
    Found(CacheEntry),
    /// Host reached the domain but extracted no usable metadata
    /// (no `<title>`, no parseable Open Graph, etc.). Mirrors
    /// the WIT `reachable-no-data` variant.
    NoData,
    /// Host could not reach the domain (DNS failure, connection
    /// refused, network down, ...). Mirrors the WIT `unreachable`
    /// variant. The wrapper also folds `PermissionDenied` and
    /// `InvalidDomain` errors here after logging, since they are
    /// programmer bugs that won't resolve by retrying.
    Unreachable,
    /// Cold cache, background fetch scheduled. Only ever returned
    /// from [`lookup_cached`]; [`lookup_blocking`] waits for the
    /// fetch and never returns this. The next call after the
    /// fetch completes will return `Found`, `NoData`, or
    /// `Unreachable`.
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
/// canonical idiom for gadgets that just want a favicon-or-
/// default for a domain in their result list.
pub fn favicon_or(domain: &str, fallback: EntryIcon) -> EntryIcon {
    match lookup_cached(domain) {
        Metadata::Found(entry) => entry.favicon,
        Metadata::NoData | Metadata::Unreachable | Metadata::Pending => fallback,
    }
}

// =========================================================
// Internal dispatch
//
// Both error variants are programmer bugs that cannot recover
// at runtime — `PermissionDenied` means the gadget's manifest
// is missing the grant, `InvalidDomain` means the gadget
// passed a malformed domain string. We log loudly via the
// host so the bug surfaces in the host's log view, then fold
// to `Unreachable` so call sites don't have to plumb error
// handling through every result construction. `Unreachable`
// (rather than `NoData`) is the closer fold: errors are not
// "the host reached the domain successfully and found
// nothing," they are "the lookup never produced an answer."
// =========================================================
fn dispatch(domain: &str, mode: LookupMode) -> Metadata {
    match website_metadata_host::lookup(domain, mode) {
        Ok(LookupResult::Hit(entry)) => Metadata::Found(entry),
        Ok(LookupResult::ReachableNoData) => Metadata::NoData,
        Ok(LookupResult::Unreachable) => Metadata::Unreachable,
        Ok(LookupResult::Pending) => Metadata::Pending,
        Err(WebsiteMetadataError::PermissionDenied(reason)) => {
            crate::log_error!(
                "website-metadata permission denied — add `website-metadata = true` to [permissions]",
                "domain" => domain,
                "reason" => reason,
            );
            Metadata::Unreachable
        }
        Err(WebsiteMetadataError::InvalidDomain(reason)) => {
            crate::log_warn!(
                "website-metadata rejected domain as invalid",
                "domain" => domain,
                "reason" => reason,
            );
            Metadata::Unreachable
        }
    }
}
