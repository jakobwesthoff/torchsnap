// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Logging host import
//
// Routes guest `logging::log` / `span-start` / `span-end`
// calls into the host's `LogSender` and `SpanRegistry`.
// `log_sender` and `span_registry` are foundational fields
// on `PluginState` (set at instance construction, not at
// enable time) so this Host impl needs no setter — every
// instance has them populated by the time any guest call
// runs.
// =========================================================

use std::time::SystemTime;

use crate::wasm::bindings;
use crate::wasm::logging::{LogItem, LogItemKind, LogLevel, LogSource};

use super::super::PluginState;

impl bindings::torchsnap::plugin::logging::Host for PluginState {
    fn log(
        &mut self,
        level: bindings::torchsnap::plugin::logging::LogLevel,
        message: String,
        metadata: Vec<(String, String)>,
        span: Option<u64>,
    ) {
        let log_level = match level {
            bindings::torchsnap::plugin::logging::LogLevel::Trace => LogLevel::Trace,
            bindings::torchsnap::plugin::logging::LogLevel::Debug => LogLevel::Debug,
            bindings::torchsnap::plugin::logging::LogLevel::Info => LogLevel::Info,
            bindings::torchsnap::plugin::logging::LogLevel::Warn => LogLevel::Warn,
            bindings::torchsnap::plugin::logging::LogLevel::Error => LogLevel::Error,
        };

        self.log_sender.send(LogItem {
            seq: 0,
            timestamp: SystemTime::now(),
            source: LogSource::Plugin(self.plugin_id.clone()),
            kind: LogItemKind::Message {
                level: log_level,
                message,
                metadata,
                span_id: span,
            },
        });
    }

    fn span_start(
        &mut self,
        name: String,
        parent: Option<u64>,
        metadata: Vec<(String, String)>,
    ) -> u64 {
        let name_for_item = name.clone();
        let meta_for_item = metadata.clone();

        match self.span_registry.start(
            name,
            parent,
            LogSource::Plugin(self.plugin_id.clone()),
            metadata,
        ) {
            Some((id, depth)) => {
                // Emit span-start log item so the frontend can
                // track open spans in real time.
                self.log_sender.send(LogItem {
                    seq: 0,
                    timestamp: SystemTime::now(),
                    source: LogSource::Plugin(self.plugin_id.clone()),
                    kind: LogItemKind::SpanStart {
                        span_id: id,
                        name: name_for_item,
                        parent_id: parent,
                        depth,
                        metadata: meta_for_item,
                    },
                });
                id
            }
            // If nesting depth exceeded, return 0 as a sentinel.
            // The guest can still pass this to span_end, which
            // will be a no-op (ID not found in registry).
            None => 0,
        }
    }

    fn span_end(&mut self, span_id: u64, metadata: Vec<(String, String)>) {
        if let Some(completed) = self.span_registry.end(span_id, metadata) {
            self.log_sender.send(LogItem {
                seq: 0,
                timestamp: SystemTime::now(),
                source: completed.source.clone(),
                kind: completed.into(),
            });
        }
    }
}
