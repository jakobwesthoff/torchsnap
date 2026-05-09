// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Command host import
//
// Synchronous-from-the-guest-perspective `command::run`
// implementation. The runtime owns the spawn, draining,
// timeout, and process-group termination — every guest
// call ends up calling `run_child_with_caps` via
// `block_in_place`. See ADR 0040 for the trust model and
// the per-call invariants this file enforces.
// =========================================================

use std::time::SystemTime;

use crate::wasm::argv_matcher;
use crate::wasm::bindings;
use crate::wasm::logging::channel::LogSender;
use crate::wasm::logging::{LogItem, LogItemKind, LogLevel, LogSource};

use super::super::GadgetState;

/// Command state. Compiled `[[permissions.command]]` rules.
#[derive(Default)]
pub(crate) struct CommandState {
    pub(crate) rules: Vec<argv_matcher::CompiledCommandRule>,
}

impl bindings::torchsnap::gadget::command::Host for GadgetState {
    fn run(
        &mut self,
        binary: String,
        options: bindings::torchsnap::gadget::command::CommandOptions,
    ) -> Result<
        bindings::torchsnap::gadget::command::CommandResult,
        bindings::torchsnap::gadget::command::CommandError,
    > {
        use argv_matcher::matches as match_rule;
        use bindings::torchsnap::gadget::command::CommandError as WitErr;

        // Access caps and GadgetState fields through disjoint
        // field borrows — the borrow checker allows this because
        // `self.caps` and `self.gadget_id`/`self.log_sender` are
        // separate fields.
        let caps = self
            .caps
            .as_ref()
            .ok_or_else(|| WitErr::SpawnFailed("capability accessed outside enable lifetime".into()))?;

        if match_rule(&caps.command.rules, &binary, &options.args).is_err() {
            return Err(WitErr::PermissionDenied(format!(
                "no `[[permissions.command]]` rule accepts `{binary}` with the given argv"
            )));
        }

        let cwd = match options.cwd.as_deref() {
            Some(explicit) => std::path::PathBuf::from(explicit),
            None => {
                let scratch = caps.gadget_paths.gadget_data.join("exec-cwd");
                if let Err(e) = std::fs::create_dir_all(&scratch) {
                    return Err(WitErr::SpawnFailed(format!(
                        "create scratch cwd `{}`: {e}",
                        scratch.display()
                    )));
                }
                scratch
            }
        };

        let env = build_command_env(&options.env);

        let timeout_ms = options
            .timeout_ms
            .map(|v| v as u64)
            .unwrap_or(DEFAULT_COMMAND_TIMEOUT_MS)
            .min(MAX_COMMAND_TIMEOUT_MS);
        let max_output_bytes = options
            .max_output_bytes
            .unwrap_or(DEFAULT_COMMAND_OUTPUT_BYTES)
            .min(MAX_COMMAND_OUTPUT_BYTES);

        let gadget_id = self.gadget_id.clone();
        let log_sender = self.log_sender.clone();
        let argv = options.args.clone();
        let stdin_bytes = options.stdin.clone();
        let started = std::time::Instant::now();

        let outcome = tokio::task::block_in_place(|| {
            let runtime_handle = tokio::runtime::Handle::current();
            runtime_handle.block_on(run_child_with_caps(
                &binary,
                &argv,
                &cwd,
                env,
                stdin_bytes,
                timeout_ms,
                max_output_bytes,
            ))
        });

        emit_command_audit(&log_sender, &gadget_id, &binary, &argv, &outcome, started);

        outcome
    }
}

// =========================================================
// command::run helpers
// =========================================================

const DEFAULT_COMMAND_TIMEOUT_MS: u64 = 10_000;
const MAX_COMMAND_TIMEOUT_MS: u64 = 60_000;
const DEFAULT_COMMAND_OUTPUT_BYTES: u64 = 4 * 1024 * 1024;
const MAX_COMMAND_OUTPUT_BYTES: u64 = 64 * 1024 * 1024;

const ENV_HARD_DENYLIST: &[&str] = &[
    "LD_PRELOAD",
    "LD_LIBRARY_PATH",
    "DYLD_INSERT_LIBRARIES",
    "DYLD_LIBRARY_PATH",
    "SSH_AUTH_SOCK",
    "GPG_AGENT_INFO",
];

const ENV_CREDENTIAL_SUFFIXES: &[&str] = &[
    "_TOKEN",
    "_KEY",
    "_PASSWORD",
    "_SECRET",
    "_PASS",
    "_PASSWD",
    "_APIKEY",
];

fn is_credential_var(name: &str) -> bool {
    if ENV_HARD_DENYLIST.contains(&name) {
        return true;
    }
    let upper = name.to_uppercase();
    ENV_CREDENTIAL_SUFFIXES
        .iter()
        .any(|suffix| upper.ends_with(suffix))
}

fn build_command_env(
    gadget_overrides: &[(String, String)],
) -> Vec<(std::ffi::OsString, std::ffi::OsString)> {
    use std::ffi::OsString;

    let mut env: Vec<(OsString, OsString)> = std::env::vars_os()
        .filter(|(k, _)| {
            let name = k.to_string_lossy();
            !is_credential_var(&name)
        })
        .map(|(k, v)| {
            if k == "PATH" {
                let cleaned = strip_empty_path_entries(&v.to_string_lossy());
                (k, OsString::from(cleaned))
            } else {
                (k, v)
            }
        })
        .collect();

    for (override_key, override_value) in gadget_overrides {
        let key = OsString::from(override_key);
        env.retain(|(existing, _)| existing != &key);
        env.push((key, OsString::from(override_value)));
    }

    env
}

fn strip_empty_path_entries(path: &str) -> String {
    let sep = if cfg!(windows) { ';' } else { ':' };
    path.split(sep)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(&sep.to_string())
}

async fn run_child_with_caps(
    binary: &str,
    args: &[String],
    cwd: &std::path::Path,
    env: Vec<(std::ffi::OsString, std::ffi::OsString)>,
    stdin_bytes: Option<Vec<u8>>,
    timeout_ms: u64,
    max_output_bytes: u64,
) -> Result<
    bindings::torchsnap::gadget::command::CommandResult,
    bindings::torchsnap::gadget::command::CommandError,
> {
    use bindings::torchsnap::gadget::command::{CommandError as WitErr, CommandResult};
    use std::process::Stdio;
    use tokio::io::AsyncWriteExt;

    let mut command = tokio::process::Command::new(binary);
    command
        .args(args)
        .current_dir(cwd)
        .env_clear()
        .envs(env)
        .stdin(if stdin_bytes.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(unix)]
    command.process_group(0);

    let mut child = command
        .spawn()
        .map_err(|e| WitErr::SpawnFailed(format!("spawn `{binary}`: {e}")))?;

    if let Some(bytes) = stdin_bytes {
        if let Some(mut child_stdin) = child.stdin.take() {
            if let Err(e) = child_stdin.write_all(&bytes).await {
                let _ = kill_child_with_grace(&mut child).await;
                return Err(WitErr::SpawnFailed(format!("writing stdin: {e}")));
            }
        }
    }

    let stdout = child
        .stdout
        .take()
        .expect("stdout configured to piped on spawn");
    let stderr = child
        .stderr
        .take()
        .expect("stderr configured to piped on spawn");

    let stdout_task = tokio::spawn(read_capped(stdout, max_output_bytes));
    let stderr_task = tokio::spawn(read_capped(stderr, max_output_bytes));

    let timeout = tokio::time::Duration::from_millis(timeout_ms);
    let wait_result = tokio::time::timeout(timeout, child.wait()).await;

    match wait_result {
        Ok(Ok(status)) => {
            let stdout_join = stdout_task.await.unwrap_or_else(|e| {
                Ok((Vec::new(), false, format!("stdout reader panicked: {e}")))
            });
            let stderr_join = stderr_task.await.unwrap_or_else(|e| {
                Ok((Vec::new(), false, format!("stderr reader panicked: {e}")))
            });

            let (stdout_bytes, stdout_over, _) = stdout_join
                .unwrap_or_else(|e| (Vec::new(), false, format!("stdout read error: {e}")));
            let (stderr_bytes, stderr_over, _) = stderr_join
                .unwrap_or_else(|e| (Vec::new(), false, format!("stderr read error: {e}")));

            if stdout_over || stderr_over {
                return Err(WitErr::OutputTooLarge((stdout_bytes, stderr_bytes)));
            }

            #[cfg(unix)]
            let signal = {
                use std::os::unix::process::ExitStatusExt;
                status.signal().map(signal_name)
            };
            #[cfg(not(unix))]
            let signal = None;

            Ok(CommandResult {
                exit_code: status.code(),
                signal,
                timed_out: false,
                stdout: stdout_bytes,
                stderr: stderr_bytes,
            })
        }
        Ok(Err(e)) => Err(WitErr::SpawnFailed(format!("waiting for child: {e}"))),
        Err(_elapsed) => {
            let _ = kill_child_with_grace(&mut child).await;

            let stdout_bytes = stdout_task
                .await
                .ok()
                .and_then(|r| r.ok().map(|(b, _, _)| b))
                .unwrap_or_default();
            let stderr_bytes = stderr_task
                .await
                .ok()
                .and_then(|r| r.ok().map(|(b, _, _)| b))
                .unwrap_or_default();

            let _ = (stdout_bytes, stderr_bytes);
            Err(WitErr::Timeout)
        }
    }
}

async fn read_capped<R>(reader: R, cap: u64) -> std::io::Result<(Vec<u8>, bool, String)>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    use tokio::io::AsyncReadExt;

    let mut buf = Vec::new();
    let cap_usize = cap.try_into().unwrap_or(usize::MAX);

    let probe_limit = cap.saturating_add(1);
    let mut limited = reader.take(probe_limit);
    limited.read_to_end(&mut buf).await?;

    let overflowed = buf.len() as u64 > cap;
    if overflowed {
        buf.truncate(cap_usize);
    }

    Ok((buf, overflowed, String::new()))
}

async fn kill_child_with_grace(child: &mut tokio::process::Child) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        if let Some(pid) = child.id() {
            unsafe {
                libc::kill(-(pid as i32), libc::SIGTERM);
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = child.start_kill();
    }

    let grace = tokio::time::Duration::from_millis(250);
    if let Ok(_) = tokio::time::timeout(grace, child.wait()).await {
        return Ok(());
    }

    #[cfg(unix)]
    {
        if let Some(pid) = child.id() {
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = child.start_kill();
    }

    let _ = child.wait().await;
    Ok(())
}

#[cfg(unix)]
fn signal_name(signal: i32) -> String {
    match signal {
        libc::SIGTERM => "SIGTERM".to_string(),
        libc::SIGKILL => "SIGKILL".to_string(),
        libc::SIGINT => "SIGINT".to_string(),
        libc::SIGHUP => "SIGHUP".to_string(),
        libc::SIGSEGV => "SIGSEGV".to_string(),
        libc::SIGABRT => "SIGABRT".to_string(),
        libc::SIGPIPE => "SIGPIPE".to_string(),
        n => format!("SIG({n})"),
    }
}

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
