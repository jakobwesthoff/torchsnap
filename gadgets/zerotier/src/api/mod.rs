// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! ZeroTier service-API client.
//!
//! Wraps the local `zerotier-one` HTTP API surface
//! (<http://localhost:9993>) the gadget actually exercises:
//! list networks, get a single network, join, leave, node
//! status. Every call goes through the gadget's `http::fetch`
//! host import and carries the `X-ZT1-Auth` header.

mod client;
mod error;
mod types;

pub use client::Client;
pub use error::ApiError;
pub use types::{Network, NetworkStatus};
