// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Control API — Handler Trait, Registry, and Error Types
//
// Core abstractions for the JSON-RPC control API. Each method
// (show, hide, query, etc.) is a struct implementing `Handler`.
// The `HandlerRegistry` maps method names to handlers for
// dispatch by the server's `process_request` function.
// =========================================================

use std::collections::HashMap;

use serde_json::Value;

// =========================================================
// ControlError
//
// Application-level errors returned by handlers. Each variant
// maps to a stable JSON-RPC error code. Protocol-level errors
// (parse error, method not found, etc.) are handled separately
// by the server framing layer.
// =========================================================

#[derive(Debug)]
pub enum ControlError {
    /// A precondition is not met (e.g., dismiss while already hidden).
    InvalidState { message: String },
    /// An unexpected internal failure.
    Internal { message: String },
}

impl ControlError {
    pub fn code(&self) -> i64 {
        match self {
            ControlError::InvalidState { .. } => -1,
            ControlError::Internal { .. } => -3,
        }
    }

    pub fn message(&self) -> &str {
        match self {
            ControlError::InvalidState { message }
            | ControlError::Internal { message } => message,
        }
    }

    /// Convert to a JSON-RPC error object `{ "code": ..., "message": ... }`.
    pub fn to_json_rpc_error(&self) -> Value {
        serde_json::json!({
            "code": self.code(),
            "message": self.message(),
        })
    }
}

// =========================================================
// Handler Trait
// =========================================================

/// A single JSON-RPC method handler.
///
/// Handlers are stateless structs that receive the request
/// params and an `AppHandle` for accessing app state. They
/// return either a success `Value` or a `ControlError`.
pub trait Handler: Send + Sync {
    fn handle(&self, params: Value, app: &tauri::AppHandle) -> Result<Value, ControlError>;
}

// =========================================================
// HandlerRegistry
// =========================================================

/// Maps JSON-RPC method names to handler implementations.
///
/// Built once at server startup and shared (via `Arc`) across
/// all client connections.
pub struct HandlerRegistry {
    handlers: HashMap<String, Box<dyn Handler>>,
}

impl HandlerRegistry {
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }

    pub fn register(&mut self, method: &str, handler: impl Handler + 'static) {
        self.handlers.insert(method.to_string(), Box::new(handler));
    }

    pub fn get(&self, method: &str) -> Option<&dyn Handler> {
        self.handlers.get(method).map(|b| b.as_ref())
    }
}
