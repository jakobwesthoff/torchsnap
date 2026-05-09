// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// CommandCap
//
// Process-spawning capability with permission checking,
// credential-stripping environment, timeout enforcement, and
// output capping. The gadget declares `[[permissions.command]]`
// rules in its manifest; the bridge compiles them via
// `argv_matcher` and passes them here. At runtime, `run()`
// matches the requested `(binary, argv)` against the compiled
// rules — no match means `PermissionDenied`.
//
// All I/O (spawn, stdin write, stdout/stderr drain) happens
// inside `tokio::task::block_in_place` so the synchronous-
// from-the-guest's-perspective call doesn't block the tokio
// executor.
// =========================================================

use std::path::PathBuf;

use crate::paths::PathResolver;
use crate::wasm::argv_matcher::{self, CompiledCommandRule};
use crate::wasm::manifest::CommandPermissionDef;

// =========================================================
// Constants
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

// =========================================================
// Error type
// =========================================================

/// Command capability error. Mirrors the WIT `command-error`
/// variants 1:1; the WASM bridge layer provides `From` impls
/// for the WIT type.
#[derive(Debug, thiserror::Error)]
pub enum CommandCapError {
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("spawn failed: {0}")]
    SpawnFailed(String),
    #[error("command timed out")]
    Timeout,
    #[error("output too large")]
    OutputTooLarge(Vec<u8>, Vec<u8>),
}

// =========================================================
// Native types
// =========================================================

/// Options for a single `command::run` call. Fields mirror
/// the WIT `command-options` record; the bridge provides
/// `From` impls for conversion.
pub struct CommandOptions {
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub env: Vec<(String, String)>,
    pub stdin: Option<Vec<u8>>,
    pub timeout_ms: Option<u32>,
    pub max_output_bytes: Option<u64>,
}

/// Result of a successful `command::run` call. Maps 1:1 to
/// the WIT `command-result` record.
#[derive(Debug)]
pub struct CommandResult {
    pub exit_code: Option<i32>,
    pub signal: Option<String>,
    pub timed_out: bool,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

// =========================================================
// CommandCap
// =========================================================

pub struct CommandCap {
    rules: Vec<CompiledCommandRule>,
    /// `<app_data_dir>/gadget-home/<gadget-id>/` — used to
    /// derive the default cwd (`exec-cwd/` subdirectory) when
    /// the guest does not supply an explicit cwd.
    gadget_data_dir: PathBuf,
}

impl CommandCap {
    /// Compile raw `[[permissions.command]]` rules from the
    /// manifest against the provided resolver and construct
    /// a capability. Fails on the first rule that cannot be
    /// compiled (consistent with `FilesystemCap::new`).
    pub fn new(
        rules_raw: &[CommandPermissionDef],
        resolver: &impl PathResolver,
        gadget_data_dir: PathBuf,
    ) -> anyhow::Result<Self> {
        let mut rules = Vec::with_capacity(rules_raw.len());
        for (index, raw) in rules_raw.iter().enumerate() {
            let rule = argv_matcher::compile_rule(raw, index, resolver)?;
            rules.push(rule);
        }
        Ok(Self {
            rules,
            gadget_data_dir,
        })
    }

    /// Construct from pre-compiled rules. Test-only escape
    /// hatch so integration tests can build fixtures with
    /// hand-crafted rule sets without going through the full
    /// manifest → compile pipeline.
    #[cfg(test)]
    pub(crate) fn from_compiled_rules(
        rules: Vec<CompiledCommandRule>,
        gadget_data_dir: PathBuf,
    ) -> Self {
        Self {
            rules,
            gadget_data_dir,
        }
    }

    /// Execute a command if the `(binary, argv)` pair matches
    /// a compiled permission rule. Handles cwd resolution,
    /// environment filtering, process spawn, timeout
    /// enforcement, and output capping.
    pub fn run(
        &self,
        binary: &str,
        options: CommandOptions,
    ) -> Result<CommandResult, CommandCapError> {
        if argv_matcher::matches(&self.rules, binary, &options.args).is_err() {
            return Err(CommandCapError::PermissionDenied(format!(
                "no `[[permissions.command]]` rule accepts `{binary}` with the given argv"
            )));
        }

        let cwd = match options.cwd.as_deref() {
            Some(explicit) => PathBuf::from(explicit),
            None => {
                let scratch = self.gadget_data_dir.join("exec-cwd");
                if let Err(e) = std::fs::create_dir_all(&scratch) {
                    return Err(CommandCapError::SpawnFailed(format!(
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

        tokio::task::block_in_place(|| {
            let runtime_handle = tokio::runtime::Handle::current();
            runtime_handle.block_on(run_child_with_caps(
                binary,
                &options.args,
                &cwd,
                env,
                options.stdin,
                timeout_ms,
                max_output_bytes,
            ))
        })
    }
}

// =========================================================
// Environment filtering
// =========================================================

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

// =========================================================
// Process spawn with timeout and output capping
// =========================================================

async fn run_child_with_caps(
    binary: &str,
    args: &[String],
    cwd: &std::path::Path,
    env: Vec<(std::ffi::OsString, std::ffi::OsString)>,
    stdin_bytes: Option<Vec<u8>>,
    timeout_ms: u64,
    max_output_bytes: u64,
) -> Result<CommandResult, CommandCapError> {
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
        .map_err(|e| CommandCapError::SpawnFailed(format!("spawn `{binary}`: {e}")))?;

    if let Some(bytes) = stdin_bytes {
        if let Some(mut child_stdin) = child.stdin.take() {
            if let Err(e) = child_stdin.write_all(&bytes).await {
                let _ = kill_child_with_grace(&mut child).await;
                return Err(CommandCapError::SpawnFailed(format!("writing stdin: {e}")));
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
                return Err(CommandCapError::OutputTooLarge(stdout_bytes, stderr_bytes));
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
        Ok(Err(e)) => Err(CommandCapError::SpawnFailed(format!(
            "waiting for child: {e}"
        ))),
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
            Err(CommandCapError::Timeout)
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

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ─── is_credential_var ────────────────────────────────

    #[test]
    fn hard_denylist_vars_are_credential() {
        for name in ENV_HARD_DENYLIST {
            assert!(
                is_credential_var(name),
                "`{name}` should be denied by hard denylist"
            );
        }
    }

    #[test]
    fn suffix_matching_is_case_insensitive() {
        assert!(is_credential_var("MY_API_TOKEN"));
        assert!(is_credential_var("my_api_token"));
        assert!(is_credential_var("Github_Token"));
        assert!(is_credential_var("DB_PASSWORD"));
        assert!(is_credential_var("db_password"));
        assert!(is_credential_var("AWS_SECRET"));
    }

    #[test]
    fn non_credential_vars_pass_through() {
        assert!(!is_credential_var("HOME"));
        assert!(!is_credential_var("PATH"));
        assert!(!is_credential_var("LANG"));
        assert!(!is_credential_var("TERM"));
        assert!(!is_credential_var("EDITOR"));
    }

    // ─── build_command_env ────────────────────────────────

    #[test]
    fn overrides_replace_existing_env_vars() {
        let overrides = vec![("TEST_VAR".to_string(), "override_value".to_string())];
        let env = build_command_env(&overrides);

        let found = env
            .iter()
            .filter(|(k, _)| k == "TEST_VAR")
            .collect::<Vec<_>>();
        // There should be exactly one entry with the override
        // value, regardless of whether the host env had a
        // TEST_VAR.
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].1, "override_value");
    }

    #[test]
    fn credential_vars_stripped_from_inherited_env() {
        let env = build_command_env(&[]);
        for (k, _) in &env {
            let name = k.to_string_lossy();
            assert!(
                !is_credential_var(&name),
                "credential var `{name}` leaked into child env"
            );
        }
    }

    // ─── strip_empty_path_entries ─────────────────────────

    #[test]
    fn removes_empty_path_segments() {
        let input = "/usr/bin::/usr/local/bin:";
        let cleaned = strip_empty_path_entries(input);
        assert_eq!(cleaned, "/usr/bin:/usr/local/bin");
    }

    #[test]
    fn preserves_clean_path() {
        let input = "/usr/bin:/usr/local/bin";
        assert_eq!(strip_empty_path_entries(input), input);
    }

    // ─── timeout / output clamp arithmetic ────────────────

    #[test]
    fn timeout_clamps_to_max() {
        let raw: u64 = 999_999;
        let clamped = raw.min(MAX_COMMAND_TIMEOUT_MS);
        assert_eq!(clamped, MAX_COMMAND_TIMEOUT_MS);
    }

    #[test]
    fn timeout_default_when_none() {
        let val: Option<u32> = None;
        let resolved = val
            .map(|v| v as u64)
            .unwrap_or(DEFAULT_COMMAND_TIMEOUT_MS)
            .min(MAX_COMMAND_TIMEOUT_MS);
        assert_eq!(resolved, DEFAULT_COMMAND_TIMEOUT_MS);
    }

    #[test]
    fn output_bytes_clamps_to_max() {
        let raw: u64 = u64::MAX;
        let clamped = raw.min(MAX_COMMAND_OUTPUT_BYTES);
        assert_eq!(clamped, MAX_COMMAND_OUTPUT_BYTES);
    }

    #[test]
    fn output_bytes_default_when_none() {
        let val: Option<u64> = None;
        let resolved = val
            .unwrap_or(DEFAULT_COMMAND_OUTPUT_BYTES)
            .min(MAX_COMMAND_OUTPUT_BYTES);
        assert_eq!(resolved, DEFAULT_COMMAND_OUTPUT_BYTES);
    }

    // ─── permission denied ────────────────────────────────

    #[test]
    fn run_with_empty_rules_returns_permission_denied() {
        let cap = CommandCap::from_compiled_rules(vec![], PathBuf::from("/tmp/test"));
        let result = cap.run(
            "/bin/echo",
            CommandOptions {
                args: vec!["hello".into()],
                cwd: None,
                env: vec![],
                stdin: None,
                timeout_ms: None,
                max_output_bytes: None,
            },
        );
        match result {
            Err(CommandCapError::PermissionDenied(msg)) => {
                assert!(msg.contains("/bin/echo"));
            }
            other => panic!("expected PermissionDenied, got {other:?}"),
        }
    }
}
