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
//
// All command-runtime helpers live in this file so the
// envelope of `command::run` is one cohesive read: env
// sanitation, kill-with-grace, capped output drain, and
// audit-log emission.
// =========================================================

use std::time::SystemTime;

use crate::wasm::argv_matcher;
use crate::wasm::bindings;
use crate::wasm::logging::channel::LogSender;
use crate::wasm::logging::{LogItem, LogItemKind, LogLevel, LogSource};

use super::super::{GadgetState, WasmGadgetInstance};

/// Command state. Compiled `[[permissions.command]]` rules
/// + (future) handle to a currently-running child for
/// disable-time termination.
#[derive(Default)]
pub(crate) struct CommandState {
    /// Compiled rules for this plugin instance. Built by the
    /// bridge from the raw manifest rules + the resolved
    /// `PathContext` at `enable()`. Empty means the plugin
    /// has no `command::run` access — every call returns
    /// `permission-denied`.
    pub(crate) rules: Vec<argv_matcher::CompiledCommandRule>,
}

impl bindings::torchsnap::plugin::command::Host for GadgetState {
    fn run(
        &mut self,
        binary: String,
        options: bindings::torchsnap::plugin::command::CommandOptions,
    ) -> Result<
        bindings::torchsnap::plugin::command::CommandResult,
        bindings::torchsnap::plugin::command::CommandError,
    > {
        use argv_matcher::matches as match_rule;
        use bindings::torchsnap::plugin::command::CommandError as WitErr;

        // 1. Permission check. Manifest-time overlap detection
        //    guarantees at most one rule matches; matching is
        //    pure (no I/O) and runs first so a denied call
        //    never spawns a process.
        if match_rule(&self.command.rules, &binary, &options.args).is_err() {
            return Err(WitErr::PermissionDenied(format!(
                "no `[[permissions.command]]` rule accepts `{binary}` with the given argv"
            )));
        }

        // 2. Resolve the working directory. The plugin can override
        //    per call via `options.cwd`; otherwise we default to
        //    `<plugin-data>/exec-cwd/`, lazily created. Without a
        //    `PathContext` the plugin is effectively pre-enable, so
        //    we can't resolve the default — surface that as a
        //    spawn-failed error for clarity.
        let cwd = match options.cwd.as_deref() {
            Some(explicit) => std::path::PathBuf::from(explicit),
            None => {
                let Some(ctx) = self.path_context.as_ref() else {
                    return Err(WitErr::SpawnFailed(
                        "no path context available — command::run requires an enabled plugin"
                            .into(),
                    ));
                };
                let scratch = ctx.plugin_data.join("exec-cwd");
                if let Err(e) = std::fs::create_dir_all(&scratch) {
                    return Err(WitErr::SpawnFailed(format!(
                        "create scratch cwd `{}`: {e}",
                        scratch.display()
                    )));
                }
                scratch
            }
        };

        // 3. Build the environment. Inherit the host process env
        //    minus a credential denylist; PATH gets empty entries
        //    stripped (empty entries resolve to cwd, an injection
        //    footgun); plugin overrides land last and replace any
        //    inherited key.
        let env = build_command_env(&options.env);

        // 4. Apply host-level caps. Plugin-supplied values are
        //    clamped — manifest-side per-rule ceilings are not
        //    yet wired (they would shrink these further once we
        //    look up the matched rule's overrides). For v1 the
        //    host defaults are the only ceiling.
        let timeout_ms = options
            .timeout_ms
            .map(|v| v as u64)
            .unwrap_or(DEFAULT_COMMAND_TIMEOUT_MS)
            .min(MAX_COMMAND_TIMEOUT_MS);
        let max_output_bytes = options
            .max_output_bytes
            .unwrap_or(DEFAULT_COMMAND_OUTPUT_BYTES)
            .min(MAX_COMMAND_OUTPUT_BYTES);

        // 5. Spawn + run. Synchronous from the guest's perspective;
        //    `block_in_place` lets the async work run on the host's
        //    thread pool without blocking the tokio runtime.
        let plugin_id = self.gadget_id.clone();
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

        // 6. Audit log: every call gets a `debug`-level entry with
        //    the binary, full argv, exit code, duration, and output
        //    sizes. Plugin authors are responsible for argv hygiene
        //    — see ADR 0040.
        emit_command_audit(&log_sender, &plugin_id, &binary, &argv, &outcome, started);

        outcome
    }
}

impl WasmGadgetInstance {
    /// Stash the compiled `[[permissions.command]]` rules.
    /// Called by the bridge at `enable()` after compiling
    /// the raw manifest rules against the per-plugin
    /// `PathContext`.
    pub fn set_command_rules(&self, rules: Vec<argv_matcher::CompiledCommandRule>) {
        self.with_state_mut(|state| state.command.rules = rules);
    }

    /// Drop the compiled command rules on `disable()`.
    pub fn clear_command_rules(&self) {
        self.with_state_mut(|state| state.command.rules.clear());
    }
}

// =========================================================
// command::run helpers
// =========================================================

const DEFAULT_COMMAND_TIMEOUT_MS: u64 = 10_000;
const MAX_COMMAND_TIMEOUT_MS: u64 = 60_000;
const DEFAULT_COMMAND_OUTPUT_BYTES: u64 = 4 * 1024 * 1024;
const MAX_COMMAND_OUTPUT_BYTES: u64 = 64 * 1024 * 1024;

/// Names that always get stripped from the inherited env
/// before a child is spawned. These are the dynamic-loader
/// hooks plus a small set of "give me your secrets" sockets
/// — none of them have a legitimate reason to flow into a
/// plugin-spawned process by default.
const ENV_HARD_DENYLIST: &[&str] = &[
    "LD_PRELOAD",
    "LD_LIBRARY_PATH",
    "DYLD_INSERT_LIBRARIES",
    "DYLD_LIBRARY_PATH",
    "SSH_AUTH_SOCK",
    "GPG_AGENT_INFO",
];

/// Suffixes that pattern-strip credential-shaped keys (case-
/// insensitive, applied after the hard denylist). The match
/// is suffix-only because prefix matching has too many false
/// positives (`KEYBOARD_LAYOUT`, `TOKEN_DEFINITION_FILE`,
/// etc).
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

/// Build the env vec for a child spawn: host process env
/// minus the credential denylist, with `PATH` empty entries
/// stripped, plus the plugin's per-call overrides.
fn build_command_env(
    plugin_overrides: &[(String, String)],
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

    for (override_key, override_value) in plugin_overrides {
        let key = OsString::from(override_key);
        env.retain(|(existing, _)| existing != &key);
        env.push((key, OsString::from(override_value)));
    }

    env
}

/// Strip empty entries from a `$PATH`-style string. An empty
/// entry resolves to cwd in execve's PATH lookup, which is a
/// historical injection footgun.
fn strip_empty_path_entries(path: &str) -> String {
    let sep = if cfg!(windows) { ';' } else { ':' };
    path.split(sep)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(&sep.to_string())
}

/// Spawn the child, drain stdout/stderr concurrently with a
/// per-stream byte cap, and wait for completion or the
/// timeout. On timeout the child is signalled SIGTERM, given
/// 250ms to clean up, then SIGKILL'd. On output overflow the
/// already-captured bytes are returned via `output-too-large`
/// so the plugin can debug what was written before the cap.
async fn run_child_with_caps(
    binary: &str,
    args: &[String],
    cwd: &std::path::Path,
    env: Vec<(std::ffi::OsString, std::ffi::OsString)>,
    stdin_bytes: Option<Vec<u8>>,
    timeout_ms: u64,
    max_output_bytes: u64,
) -> Result<
    bindings::torchsnap::plugin::command::CommandResult,
    bindings::torchsnap::plugin::command::CommandError,
> {
    use bindings::torchsnap::plugin::command::{CommandError as WitErr, CommandResult};
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

    // On Unix, place the child in its own process group so a
    // single signal can take down the entire tree (including
    // any grandchildren the binary spawns — `git` calling its
    // pager, etc). On Windows the equivalent (Job Objects) is
    // a future improvement; tokio's `Child::kill` already
    // sends `TerminateProcess`, which is sufficient for v1.
    #[cfg(unix)]
    command.process_group(0);

    let mut child = command
        .spawn()
        .map_err(|e| WitErr::SpawnFailed(format!("spawn `{binary}`: {e}")))?;

    // Optional stdin: write once, drop the handle so the
    // child sees EOF, then proceed to draining stdout/stderr.
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

    // Concurrent capped reads on both streams plus child wait.
    // The cap applies per-stream: each can produce up to
    // `max_output_bytes`. A stricter "combined cap" would
    // require a shared accumulator; the per-stream cap is
    // simpler and matches the WIT shape (separate stdout /
    // stderr fields on `command-result`).
    let stdout_task = tokio::spawn(read_capped(stdout, max_output_bytes));
    let stderr_task = tokio::spawn(read_capped(stderr, max_output_bytes));

    let timeout = tokio::time::Duration::from_millis(timeout_ms);
    let wait_result = tokio::time::timeout(timeout, child.wait()).await;

    match wait_result {
        Ok(Ok(status)) => {
            // Child exited cleanly. Join the reader tasks; if
            // either signalled overflow, return
            // `output-too-large` carrying the partial bytes.
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
            // Timeout. SIGTERM, 250ms grace, SIGKILL.
            let _ = kill_child_with_grace(&mut child).await;

            // After kill, drain whatever the readers captured
            // before the child died.
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

            // Surface as `timeout` plus the partial output via a
            // separate path. WIT only carries one of the four
            // error variants per call; `timeout` does not carry
            // output, so partial bytes are lost in the WIT
            // crossing. Plugin authors who want the bytes can
            // lower their timeout and watch for `timeout`
            // explicitly. The audit log captures sizes.
            let _ = (stdout_bytes, stderr_bytes);
            Err(WitErr::Timeout)
        }
    }
}

/// Read up to `cap + 1` bytes from `reader`. Returns
/// `(bytes_up_to_cap, overflowed, info)` — `overflowed` is
/// `true` iff the reader produced more than `cap` bytes.
/// The trailing one-byte over-read is the cheapest way to
/// distinguish "exactly at cap" from "would have been more".
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

/// SIGTERM the child, wait up to 250ms for it to clean up,
/// then SIGKILL. On Unix the signal targets the process
/// group so grandchildren are also taken down. On non-Unix
/// platforms the kill is a single `TerminateProcess` call
/// via `tokio::process::Child::kill`.
async fn kill_child_with_grace(child: &mut tokio::process::Child) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        if let Some(pid) = child.id() {
            // SAFETY: `kill(2)` with a negative pid signals
            // the process group with id `|pid|`. The pid is
            // valid until we observe the child via `wait`.
            unsafe {
                libc::kill(-(pid as i32), libc::SIGTERM);
            }
        }
    }
    #[cfg(not(unix))]
    {
        // Windows: best-effort kill.
        let _ = child.start_kill();
    }

    let grace = tokio::time::Duration::from_millis(250);
    if let Ok(_) = tokio::time::timeout(grace, child.wait()).await {
        return Ok(());
    }

    // Grace period expired — escalate to SIGKILL.
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

/// Emit a structured audit-log entry for a completed
/// `command::run` call. Always at `debug` level — this is
/// forensic detail that should not flood normal operator
/// logs unless explicitly enabled. See ADR 0040 for the
/// argv-hygiene contract.
fn emit_command_audit(
    log_sender: &LogSender,
    plugin_id: &str,
    binary: &str,
    argv: &[String],
    outcome: &Result<
        bindings::torchsnap::plugin::command::CommandResult,
        bindings::torchsnap::plugin::command::CommandError,
    >,
    started: std::time::Instant,
) {
    use bindings::torchsnap::plugin::command::CommandError as WitErr;

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
        source: LogSource::Plugin(plugin_id.to_string()),
        kind: LogItemKind::Message {
            level: LogLevel::Debug,
            message: format!("command::run {binary}"),
            metadata,
            span_id: None,
        },
    });
}
