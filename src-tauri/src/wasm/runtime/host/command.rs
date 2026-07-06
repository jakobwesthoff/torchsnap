// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Command host import
//
// Thin bridge layer converting between WIT types and the
// native `CommandCap` capability. Permission checking,
// environment filtering, process spawn, timeout, and output
// capping all happen inside `CommandCap`. This file handles
// WIT ↔ native type conversion and post-call audit logging.
// =========================================================

use std::sync::Arc;
use std::time::SystemTime;

use crate::caps::{CommandCapError, CommandOptions, CommandResult};
use crate::wasm::bindings;
use crate::wasm::logging::channel::LogSender;
use crate::wasm::logging::{LogItem, LogItemKind, LogLevel, LogSource};

use super::super::GadgetState;

// =========================================================
// From impls: WIT → Cap (inputs)
// =========================================================

impl From<bindings::torchsnap::gadget::command::CommandOptions> for CommandOptions {
    fn from(o: bindings::torchsnap::gadget::command::CommandOptions) -> Self {
        CommandOptions {
            args: o.args,
            cwd: o.cwd,
            env: o.env,
            stdin: o.stdin,
            timeout_ms: o.timeout_ms,
            max_output_bytes: o.max_output_bytes,
        }
    }
}

// =========================================================
// From impls: Cap → WIT (outputs + errors)
// =========================================================

impl From<CommandResult> for bindings::torchsnap::gadget::command::CommandResult {
    fn from(r: CommandResult) -> Self {
        Self {
            exit_code: r.exit_code,
            signal: r.signal,
            timed_out: r.timed_out,
            stdout: r.stdout,
            stderr: r.stderr,
        }
    }
}

impl From<CommandCapError> for bindings::torchsnap::gadget::command::CommandError {
    fn from(e: CommandCapError) -> Self {
        match e {
            CommandCapError::PermissionDenied(msg) => Self::PermissionDenied(msg),
            CommandCapError::SpawnFailed(msg) => Self::SpawnFailed(msg),
            CommandCapError::Timeout => Self::Timeout,
            CommandCapError::OutputTooLarge(stdout, stderr) => {
                Self::OutputTooLarge((stdout, stderr))
            }
        }
    }
}

// =========================================================
// Host trait impl
// =========================================================

impl bindings::torchsnap::gadget::command::Host for GadgetState {
    fn run(
        &mut self,
        binary: String,
        options: bindings::torchsnap::gadget::command::CommandOptions,
    ) -> Result<
        bindings::torchsnap::gadget::command::CommandResult,
        bindings::torchsnap::gadget::command::CommandError,
    > {
        use bindings::torchsnap::gadget::command::CommandError as WitErr;

        // Clone the command cap Arc before accessing other self fields.
        // `self.caps()` borrows self, so we clone the Arc first to release
        // the borrow before accessing `self.gadget_id` and `self.log_sender`.
        let command_cap = Arc::clone(self.caps.command.as_ref().ok_or_else(|| {
            WitErr::PermissionDenied("no `[[permissions.command]]` rules declared".into())
        })?);

        let argv_clone = options.args.clone();
        let native_options: CommandOptions = options.into();

        let gadget_id = self.gadget_id.clone();
        let log_sender = self.log_sender.clone();
        let started = std::time::Instant::now();

        let outcome = command_cap.run(&binary, native_options);

        // Convert to WIT types for audit logging, which
        // operates on WIT-level result types.
        let wit_outcome = match &outcome {
            Ok(r) => Ok(bindings::torchsnap::gadget::command::CommandResult {
                exit_code: r.exit_code,
                signal: r.signal.clone(),
                timed_out: r.timed_out,
                stdout: r.stdout.clone(),
                stderr: r.stderr.clone(),
            }),
            Err(e) => Err(match e {
                CommandCapError::PermissionDenied(msg) => WitErr::PermissionDenied(msg.clone()),
                CommandCapError::SpawnFailed(msg) => WitErr::SpawnFailed(msg.clone()),
                CommandCapError::Timeout => WitErr::Timeout,
                CommandCapError::OutputTooLarge(stdout, stderr) => {
                    WitErr::OutputTooLarge((stdout.clone(), stderr.clone()))
                }
            }),
        };

        emit_command_audit(
            &log_sender,
            &gadget_id,
            &binary,
            &argv_clone,
            &wit_outcome,
            started,
        );

        outcome.map(|r| r.into()).map_err(|e| e.into())
    }
}

// =========================================================
// Audit logging
// =========================================================

fn emit_command_audit(
    log_sender: &LogSender,
    gadget_id: &str,
    binary: &str,
    argv: &[String],
    outcome: &Result<
        bindings::torchsnap::gadget::command::CommandResult,
        bindings::torchsnap::gadget::command::CommandError,
    >,
    started: std::time::Instant,
) {
    use bindings::torchsnap::gadget::command::CommandError as WitErr;

    let duration_ms = started.elapsed().as_millis() as u64;

    let (status_label, exit_label, output_label) = match outcome {
        Ok(result) => {
            let exit = result
                .exit_code
                .map(|c| c.to_string())
                .or_else(|| result.signal.clone())
                .unwrap_or_else(|| "?".to_string());
            (
                "ok".to_string(),
                exit,
                format!(
                    "stdout={}b stderr={}b",
                    result.stdout.len(),
                    result.stderr.len()
                ),
            )
        }
        Err(WitErr::PermissionDenied(_)) => (
            "permission-denied".to_string(),
            "-".to_string(),
            "-".to_string(),
        ),
        Err(WitErr::SpawnFailed(_)) => {
            ("spawn-failed".to_string(), "-".to_string(), "-".to_string())
        }
        Err(WitErr::Timeout) => ("timeout".to_string(), "-".to_string(), "-".to_string()),
        Err(WitErr::OutputTooLarge((stdout, stderr))) => (
            "output-too-large".to_string(),
            "-".to_string(),
            format!("stdout={}b stderr={}b", stdout.len(), stderr.len()),
        ),
    };

    let metadata = vec![
        ("binary".to_string(), binary.to_string()),
        ("argv".to_string(), format!("{argv:?}")),
        ("status".to_string(), status_label),
        ("exit".to_string(), exit_label),
        ("output".to_string(), output_label),
        ("duration_ms".to_string(), duration_ms.to_string()),
    ];

    log_sender.send(LogItem {
        seq: 0,
        timestamp: SystemTime::now(),
        source: LogSource::Gadget(gadget_id.to_string()),
        kind: LogItemKind::Message {
            level: LogLevel::Debug,
            message: format!("command::run {binary}"),
            metadata,
            span_id: None,
        },
    });
}
