// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Command encoding
//
// The WIT carries an action's command as an opaque string.
// The `Search` layer encodes the gadget's command type to JSON
// with these helpers and decodes it again in `execute()`.
// =========================================================

/// Encode a serializable value into a JSON string.
pub fn encode<T: serde::Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string(value).map_err(|e| format!("data::encode: {e}"))
}

/// Decode a JSON string back into a typed value.
pub fn decode<T: serde::de::DeserializeOwned>(s: &str) -> Result<T, String> {
    serde_json::from_str(s).map_err(|e| format!("data::decode: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Payload {
        url: String,
        count: u32,
    }

    #[test]
    fn round_trip_struct() {
        let original = Payload {
            url: "https://example.com".to_string(),
            count: 42,
        };
        let encoded = encode(&original).expect("encode");
        let decoded: Payload = decode(&encoded).expect("decode");
        assert_eq!(decoded, original);
    }

    #[test]
    fn round_trip_string() {
        let original = "hello world".to_string();
        let encoded = encode(&original).expect("encode");
        let decoded: String = decode(&encoded).expect("decode");
        assert_eq!(decoded, original);
    }

    #[test]
    fn round_trip_option() {
        let original: Option<String> = Some("present".to_string());
        let encoded = encode(&original).expect("encode");
        let decoded: Option<String> = decode(&encoded).expect("decode");
        assert_eq!(decoded, original);
    }

    #[test]
    fn decode_malformed_json_returns_error() {
        let result = decode::<Payload>("not json at all");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("data::decode"));
    }

    #[test]
    fn decode_type_mismatch_returns_error() {
        let encoded = encode(&42u32).expect("encode int");
        let result = decode::<Payload>(&encoded);
        assert!(result.is_err());
    }
}
