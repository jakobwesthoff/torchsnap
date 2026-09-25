// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Frontend ↔ gadget messaging.
//!
//! The frontend calls `sendMessage(method, payload)`; the host
//! hands both to the gadget's `messaging::handle-message` export as
//! a method name and a JSON-encoded payload, and returns the
//! gadget's JSON-encoded response to the frontend.
//!
//! Implement [`Messaging`] with one enum that lists every method
//! the frontend may call. The SDK decodes each call into that enum
//! before gadget code runs:
//!
//! ```ignore
//! #[derive(Deserialize)]
//! #[serde(tag = "method", content = "payload", rename_all = "snake_case")]
//! enum Request {
//!     SaveHistory { expression: String, result: String },
//!     ClearHistory,
//! }
//!
//! impl Messaging for Calculator {
//!     type Request = Request;
//!
//!     fn handle(request: Request) -> Result<serde_json::Value, String> {
//!         match request {
//!             Request::SaveHistory { expression, result } => { /* ... */ }
//!             Request::ClearHistory => { /* ... */ }
//!         }
//!     }
//! }
//! ```
//!
//! A gadget can still implement the generated
//! [`MessagingGuest`](crate::MessagingGuest) by hand instead, with
//! [`parse_payload`] and [`to_response`] for the JSON.

use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Map, Value};

use crate::MessagingGuest;

/// Typed `handle-message` for a gadget.
///
/// `Request` is deserialized from `{"method": …, "payload": …}`, so
/// an enum with `#[serde(tag = "method", content = "payload")]`
/// maps each method name to a variant and its payload to the
/// variant's fields. Methods without arguments are unit variants:
/// the `{}` payload the frontend sends for them is accepted.
pub trait Messaging {
    type Request: DeserializeOwned;

    /// Handle one decoded call. The returned value is serialized as
    /// the response the frontend's `sendMessage` resolves with;
    /// `Err` rejects it.
    fn handle(request: Self::Request) -> Result<Value, String>;
}

impl<T: Messaging> MessagingGuest for T {
    fn handle_message(method: String, payload: String) -> Result<String, String> {
        let request = decode_request::<T::Request>(&method, &payload)?;
        let response = T::handle(request)?;
        to_response(&response)
    }
}

/// Decode a `handle-message` call into `R`, as the [`Messaging`]
/// blanket impl does. Public so gadget tests can check their
/// request enum against the payloads their frontend sends.
///
/// Unit variants reject any payload but a missing one or `null`,
/// while the frontend sends `{}` for methods without arguments. An
/// empty object is therefore decoded as given first, which keeps
/// struct variants such as `ClearAll {}` working, and only when
/// that fails decoded again without the payload.
pub fn decode_request<R: DeserializeOwned>(method: &str, payload: &str) -> Result<R, String> {
    let payload: Value = serde_json::from_str(payload)
        .map_err(|e| format!("invalid payload for `{method}`: {e}"))?;
    let is_empty_object = payload.as_object().is_some_and(Map::is_empty);

    let with_payload = Value::Object(Map::from_iter([
        ("method".to_string(), Value::String(method.to_string())),
        ("payload".to_string(), payload),
    ]));
    let error = match serde_json::from_value::<R>(with_payload) {
        Ok(request) => return Ok(request),
        Err(error) => error,
    };

    if is_empty_object {
        let without_payload = Value::Object(Map::from_iter([(
            "method".to_string(),
            Value::String(method.to_string()),
        )]));
        if let Ok(request) = serde_json::from_value::<R>(without_payload) {
            return Ok(request);
        }
    }
    Err(format!("invalid message `{method}`: {error}"))
}

/// Parse a JSON-encoded message payload into `T`.
pub fn parse_payload<T: DeserializeOwned>(payload: &str) -> Result<T, String> {
    serde_json::from_str(payload).map_err(|e| format!("invalid payload: {e}"))
}

/// Serialize `value` as a JSON-encoded response string.
pub fn to_response<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string(value).map_err(|e| format!("serialize response: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use serde_json::json;

    #[derive(Debug, PartialEq, Deserialize)]
    #[serde(tag = "method", content = "payload", rename_all = "snake_case")]
    enum Request {
        Save { expression: String, result: String },
        Echo(Value),
        ClearHistory,
        ClearAll {},
        Fail,
    }

    struct TestGadget;

    impl Messaging for TestGadget {
        type Request = Request;

        fn handle(request: Request) -> Result<Value, String> {
            match request {
                Request::Save { expression, result } => {
                    Ok(json!({ "saved": [expression, result] }))
                }
                Request::Echo(value) => Ok(value),
                Request::ClearHistory => Ok(json!({ "cleared": "history" })),
                Request::ClearAll {} => Ok(json!({ "cleared": "all" })),
                Request::Fail => Err("gadget refused".to_string()),
            }
        }
    }

    fn call(method: &str, payload: &str) -> Result<Value, String> {
        let response = <TestGadget as MessagingGuest>::handle_message(
            method.to_string(),
            payload.to_string(),
        )?;
        Ok(serde_json::from_str(&response).expect("response is JSON"))
    }

    #[test]
    fn struct_payload_reaches_the_variant_fields() {
        assert_eq!(
            call("save", r#"{"expression":"6*7","result":"42"}"#),
            Ok(json!({ "saved": ["6*7", "42"] }))
        );
    }

    #[test]
    fn newtype_variant_receives_the_payload_as_is() {
        assert_eq!(call("echo", r#"[1,"two"]"#), Ok(json!([1, "two"])));
    }

    #[test]
    fn empty_object_is_accepted_for_a_unit_variant() {
        assert_eq!(
            call("clear_history", "{}"),
            Ok(json!({ "cleared": "history" }))
        );
    }

    #[test]
    fn null_is_accepted_for_a_unit_variant() {
        assert_eq!(
            call("clear_history", "null"),
            Ok(json!({ "cleared": "history" }))
        );
    }

    #[test]
    fn empty_object_still_reaches_an_empty_struct_variant() {
        assert_eq!(call("clear_all", "{}"), Ok(json!({ "cleared": "all" })));
    }

    #[test]
    fn unknown_method_is_rejected_before_gadget_code_runs() {
        let error = call("nope", "{}").expect_err("unknown method");
        assert!(error.contains("`nope`"), "{error}");
        assert!(error.contains("unknown variant"), "{error}");
    }

    #[test]
    fn payload_that_does_not_match_the_variant_is_rejected() {
        let error = call("save", r#"{"expression":"6*7"}"#).expect_err("result missing");
        assert!(error.contains("`save`"), "{error}");
        assert!(error.contains("result"), "{error}");
    }

    #[test]
    fn non_empty_payload_for_a_unit_variant_is_rejected() {
        call("clear_history", r#"{"all":true}"#).expect_err("unit variant takes no payload");
    }

    #[test]
    fn payload_that_is_not_json_is_rejected() {
        let error = call("save", "not json").expect_err("invalid JSON");
        assert!(error.contains("invalid payload for `save`"), "{error}");
    }

    #[test]
    fn gadget_error_is_passed_through() {
        assert_eq!(call("fail", "{}"), Err("gadget refused".to_string()));
    }
}
