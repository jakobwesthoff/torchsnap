// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! JSON helpers for `handle_message` dispatch.
//!
//! Both sides of the `messaging::handle-message` export are
//! JSON-encoded strings, so typical gadget code looks like:
//!
//! ```ignore
//! "save_history" => {
//!     let req: SaveHistoryRequest = messaging::parse_payload(&payload)?;
//!     messaging::to_response(&SaveHistoryResponse { saved: true })
//! }
//! ```
//!
//! The error messages carry the `serde_json` diagnostic so
//! frontend / host logs can tell a malformed payload from a
//! gadget-level rejection.

use serde::{Serialize, de::DeserializeOwned};

/// Parse a JSON-encoded message payload into `T`.
pub fn parse_payload<T: DeserializeOwned>(payload: &str) -> Result<T, String> {
    serde_json::from_str(payload).map_err(|e| format!("invalid payload: {e}"))
}

/// Serialize `value` as a JSON-encoded response string.
pub fn to_response<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string(value).map_err(|e| format!("serialize response: {e}"))
}
