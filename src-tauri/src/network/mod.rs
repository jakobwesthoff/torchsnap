// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Network Infrastructure
//
// HTTP client abstraction that hides the underlying driver
// (reqwest) behind a simple, WASM-boundary-compatible API.
// Plugins construct their own `Http` instance in `enable()`,
// similar to how `SqlStorage` is used for per-plugin databases.
//
// The `website_metadata` submodule builds on the HTTP layer
// to provide a shared, cached website metadata service
// (title, description, favicon) for plugins.
// =========================================================

mod http;
pub mod website_metadata;

pub use http::Http;
