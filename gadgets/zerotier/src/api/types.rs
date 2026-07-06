// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Wire types matching the `zerotier-one` HTTP API JSON.
//!
//! Field naming follows the daemon's camelCase convention; we
//! map to snake_case Rust via `#[serde(rename_all)]` at the
//! struct level. Only the fields the gadget actually consumes
//! are modeled — everything else is dropped during deserialization.

use serde::{Deserialize, Serialize};

/// A single network the daemon is currently joined to (or
/// requesting configuration from). `id` is the canonical
/// 16-hex-char network identifier; `name` is the
/// human-friendly label assigned by the network controller.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Network {
    pub id: String,
    /// Empty before the daemon has fetched the network's
    /// config from the controller. Always present in the JSON.
    #[serde(default)]
    pub name: String,
    pub status: NetworkStatus,
    /// Network type as reported by the daemon (`PRIVATE`,
    /// `PUBLIC`). Optional because some controllers omit it
    /// from the response.
    #[serde(default, rename = "type")]
    pub network_type: Option<String>,
    #[serde(default)]
    pub mac: String,
    #[serde(default)]
    pub port_device_name: String,
    #[serde(default)]
    pub mtu: u32,
    #[serde(default)]
    pub allow_managed: bool,
    #[serde(default)]
    pub allow_global: bool,
    #[serde(default)]
    pub allow_default: bool,
    #[serde(default, rename = "allowDNS")]
    pub allow_dns: bool,
    #[serde(default)]
    pub assigned_addresses: Vec<String>,
    #[serde(default)]
    pub routes: Vec<Route>,
    #[serde(default)]
    pub dns: Option<Dns>,
}

/// Per-network status reported by the daemon. The connected
/// state in the launcher UI is derived from
/// [`NetworkStatus::Ok`]; every other variant is "joined but
/// not currently connected".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NetworkStatus {
    /// Connected, traffic can flow.
    Ok,
    /// Joined; daemon is fetching config from the controller.
    RequestingConfiguration,
    /// Controller has explicitly rejected this node.
    AccessDenied,
    /// Network ID does not exist on the controller.
    NotFound,
    /// Controller requires authentication that the daemon
    /// has not provided.
    AuthenticationRequired,
    /// Local TCP/UDP port problem.
    PortError,
    /// Catch-all for status strings the daemon adds in future
    /// releases.
    #[serde(other)]
    Unknown,
}

/// Managed route entry — pushed by the controller, optionally
/// installed into the host routing table when
/// `allow_managed` is true.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Route {
    pub target: String,
    #[serde(default)]
    pub via: Option<String>,
    #[serde(default)]
    pub flags: u32,
    #[serde(default)]
    pub metric: u32,
}

/// DNS configuration pushed by the controller. Empty when the
/// network has no DNS settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dns {
    #[serde(default)]
    pub domain: String,
    #[serde(default)]
    pub servers: Vec<String>,
}

/// Node-level status from `GET /status`. The gadget uses this
/// to validate that the configured auth token is accepted by
/// the daemon — a 200 response means the token works.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// 10-char hex node address.
    pub address: String,
    /// Whether the node has at least one upstream peer.
    #[serde(default)]
    pub online: bool,
    #[serde(default)]
    pub version: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal JSON payload the daemon emits for a
    /// successfully-connected network. Used as a baseline that
    /// individual status-variant tests mutate.
    fn ok_network_json() -> &'static str {
        r#"{
            "id": "abcdef0123456789",
            "name": "homenet",
            "status": "OK",
            "type": "PRIVATE",
            "mac": "fa:bc:de:01:23:45",
            "portDeviceName": "feth4824",
            "mtu": 2800,
            "allowManaged": true,
            "allowGlobal": false,
            "allowDefault": false,
            "allowDNS": true,
            "assignedAddresses": ["10.147.17.42/24"],
            "routes": [
                { "target": "10.147.17.0/24", "via": null, "flags": 0, "metric": 0 }
            ],
            "dns": { "domain": "homenet.zt", "servers": ["10.147.17.1"] }
        }"#
    }

    #[test]
    fn parses_ok_status() {
        let net: Network = serde_json::from_str(ok_network_json()).expect("parse");
        assert_eq!(net.status, NetworkStatus::Ok);
        assert_eq!(net.id, "abcdef0123456789");
        assert_eq!(net.assigned_addresses, vec!["10.147.17.42/24".to_string()]);
        assert_eq!(net.routes.len(), 1);
        assert_eq!(net.dns.as_ref().unwrap().servers, vec!["10.147.17.1"]);
    }

    #[test]
    fn parses_requesting_configuration() {
        let json = r#"{"id":"a","name":"","status":"REQUESTING_CONFIGURATION"}"#;
        let net: Network = serde_json::from_str(json).expect("parse");
        assert_eq!(net.status, NetworkStatus::RequestingConfiguration);
    }

    #[test]
    fn parses_every_documented_status_variant() {
        for (raw, expected) in [
            ("OK", NetworkStatus::Ok),
            (
                "REQUESTING_CONFIGURATION",
                NetworkStatus::RequestingConfiguration,
            ),
            ("ACCESS_DENIED", NetworkStatus::AccessDenied),
            ("NOT_FOUND", NetworkStatus::NotFound),
            (
                "AUTHENTICATION_REQUIRED",
                NetworkStatus::AuthenticationRequired,
            ),
            ("PORT_ERROR", NetworkStatus::PortError),
        ] {
            let json = format!(r#"{{"id":"x","name":"","status":"{raw}"}}"#);
            let net: Network = serde_json::from_str(&json).expect("parse");
            assert_eq!(net.status, expected, "status `{raw}`");
        }
    }

    #[test]
    fn unknown_status_falls_back_to_unknown_variant() {
        let json = r#"{"id":"x","name":"","status":"FUTURE_VARIANT"}"#;
        let net: Network = serde_json::from_str(json).expect("parse");
        assert_eq!(net.status, NetworkStatus::Unknown);
    }

    #[test]
    fn missing_optional_fields_default_cleanly() {
        // Daemon may omit fields when the network is still
        // requesting configuration. Tolerate that.
        let json = r#"{"id":"x","name":"","status":"REQUESTING_CONFIGURATION"}"#;
        let net: Network = serde_json::from_str(json).expect("parse");
        assert_eq!(net.assigned_addresses.len(), 0);
        assert_eq!(net.routes.len(), 0);
        assert!(net.dns.is_none());
    }

    #[test]
    fn parses_status_endpoint() {
        let json = r#"{"address":"deadbeef01","online":true,"version":"1.14.2"}"#;
        let s: Status = serde_json::from_str(json).expect("parse");
        assert_eq!(s.address, "deadbeef01");
        assert!(s.online);
        assert_eq!(s.version, "1.14.2");
    }
}
