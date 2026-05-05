// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Structured logging helpers for plugins.
//!
//! Flat re-exports of the host's `logging` interface plus
//! convenience macros modelled on `tracing::info!` /
//! `tracing::error!`. The macros cover the common case — a
//! message with optional `key => value` metadata pairs, no
//! parent span — so plugin code doesn't drown in
//! `logging_host::log(logging_host::LogLevel::Info, "...",
//! &[], None)` boilerplate.
//!
//! Call [`log`] / [`span_start`] / [`span_end`] directly when
//! you need the full WIT surface (explicit metadata arrays,
//! span handles). The macros deliberately don't forward
//! span handles — span-scoped logging is a deliberate
//! opt-in, and the default of "log outside any span" matches
//! how nearly every log call site in existing plugins is
//! already written.

pub use crate::logging_host::{LogLevel, log, span_end, span_start};

/// Log at `info` level. Use `log_info!("msg", key => value, …)`.
///
/// The `key => value` pairs are captured by value and
/// converted via `ToString`, so scalars (`i32`, `bool`) and
/// owned strings all work without manual conversion.
#[macro_export]
macro_rules! log_info {
    ($msg:expr $(, $key:expr => $val:expr)* $(,)?) => {
        $crate::__log_impl!($crate::logging::LogLevel::Info, $msg $(, $key => $val)*)
    };
}

/// Log at `warn` level. See [`log_info!`] for the arg shape.
#[macro_export]
macro_rules! log_warn {
    ($msg:expr $(, $key:expr => $val:expr)* $(,)?) => {
        $crate::__log_impl!($crate::logging::LogLevel::Warn, $msg $(, $key => $val)*)
    };
}

/// Log at `error` level. See [`log_info!`] for the arg shape.
#[macro_export]
macro_rules! log_error {
    ($msg:expr $(, $key:expr => $val:expr)* $(,)?) => {
        $crate::__log_impl!($crate::logging::LogLevel::Error, $msg $(, $key => $val)*)
    };
}

/// Log at `debug` level. See [`log_info!`] for the arg shape.
#[macro_export]
macro_rules! log_debug {
    ($msg:expr $(, $key:expr => $val:expr)* $(,)?) => {
        $crate::__log_impl!($crate::logging::LogLevel::Debug, $msg $(, $key => $val)*)
    };
}

/// Log at `trace` level. See [`log_info!`] for the arg shape.
#[macro_export]
macro_rules! log_trace {
    ($msg:expr $(, $key:expr => $val:expr)* $(,)?) => {
        $crate::__log_impl!($crate::logging::LogLevel::Trace, $msg $(, $key => $val)*)
    };
}

/// Internal dispatcher for the log macros.
///
/// The WIT `log` function takes `metadata: &[(String, String)]`
/// (by value after `wit-bindgen` name-mangling, but the caller
/// side sees owned `String`s in the tuple). The macro
/// stringifies each `$val` via `.to_string()` and collects an
/// owned `Vec` so temporaries live long enough for the call.
#[macro_export]
#[doc(hidden)]
macro_rules! __log_impl {
    ($level:expr, $msg:expr $(, $key:expr => $val:expr)*) => {{
        let metadata: &[(String, String)] = &[
            $( ($key.to_string(), $val.to_string()) ),*
        ];
        $crate::logging::log($level, $msg, metadata, None);
    }};
}
