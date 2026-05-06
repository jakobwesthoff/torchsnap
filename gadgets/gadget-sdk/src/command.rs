// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Ergonomic builder for the `command::run` host import.
//!
//! Gadget code typically looks like:
//!
//! ```ignore
//! use std::time::Duration;
//! use torchsnap_gadget_sdk::command;
//!
//! let result = command::run("mdfind")
//!     .arg("kMDItemContentType == 'com.apple.application-bundle'")
//!     .timeout(Duration::from_secs(5))
//!     .invoke()?;
//!
//! let stdout = String::from_utf8_lossy(&result.stdout);
//! ```
//!
//! Every call to [`run`] must match exactly one
//! `[[permissions.command]]` rule in the gadget's
//! `manifest.toml`. Mismatches surface as
//! [`CommandError::PermissionDenied`]; see ADR 0040 for the
//! trust model and constraint vocabulary.
//!
//! The raw host import lives at
//! [`crate::command_host`](crate::command_host) for gadget
//! code that wants to call into the WIT surface directly
//! without the builder.

use std::time::Duration;

use crate::command_host::{
    self, CommandOptions as RawOptions,
};

pub use crate::command_host::{CommandError, CommandResult};

/// Start building a `command::run` invocation against
/// `binary`. The returned [`CommandBuilder`] owns its
/// argv/cwd/env/stdin/timeout state until [`invoke`] is
/// called; the builder methods consume `self` and return
/// `Self` so calls chain.
///
/// [`invoke`]: CommandBuilder::invoke
pub fn run(binary: impl Into<String>) -> CommandBuilder {
    CommandBuilder {
        binary: binary.into(),
        args: Vec::new(),
        cwd: None,
        env: Vec::new(),
        stdin: None,
        timeout_ms: None,
        max_output_bytes: None,
    }
}

/// Builder for a single `command::run` invocation. Matches
/// the WIT `command-options` record one-for-one, with
/// argument-style ergonomics so callers don't have to
/// hand-construct the raw record.
pub struct CommandBuilder {
    binary: String,
    args: Vec<String>,
    cwd: Option<String>,
    env: Vec<(String, String)>,
    stdin: Option<Vec<u8>>,
    timeout_ms: Option<u32>,
    max_output_bytes: Option<u64>,
}

impl CommandBuilder {
    /// Append one argv element.
    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Append every element of an iterable to argv.
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    /// Set the child's working directory. When omitted, the
    /// host falls back to `${gadget-data}/exec-cwd/`.
    pub fn cwd(mut self, cwd: impl Into<String>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    /// Add or override one environment variable. Repeated
    /// calls with the same key keep all entries — the host's
    /// env-vec semantics treat the last one as authoritative.
    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    /// Provide stdin bytes for the child. The handle is
    /// closed after the bytes are written so the child sees
    /// EOF; for streaming stdin, drop down to the raw
    /// [`crate::command_host::run`] entry point.
    pub fn stdin(mut self, bytes: impl Into<Vec<u8>>) -> Self {
        self.stdin = Some(bytes.into());
        self
    }

    /// Wall-clock timeout for the call. Capped by the host's
    /// hard ceiling (60s in v1).
    pub fn timeout(mut self, duration: Duration) -> Self {
        let millis = duration.as_millis().min(u32::MAX as u128) as u32;
        self.timeout_ms = Some(millis);
        self
    }

    /// Per-stream output cap (applies to both stdout and
    /// stderr). When the cap is exceeded the host kills the
    /// child and returns
    /// [`CommandError::OutputTooLarge`] carrying the bytes
    /// captured up to the cap. Capped by the host's hard
    /// ceiling (64 MiB in v1).
    pub fn max_output_bytes(mut self, max: u64) -> Self {
        self.max_output_bytes = Some(max);
        self
    }

    /// Spawn the binary, drain output, and return the
    /// captured result. Synchronous from the gadget's
    /// perspective; runs to completion before returning.
    pub fn invoke(self) -> Result<CommandResult, CommandError> {
        let options = RawOptions {
            args: self.args,
            cwd: self.cwd,
            env: self.env,
            stdin: self.stdin,
            timeout_ms: self.timeout_ms,
            max_output_bytes: self.max_output_bytes,
        };
        command_host::run(&self.binary, &options)
    }
}
