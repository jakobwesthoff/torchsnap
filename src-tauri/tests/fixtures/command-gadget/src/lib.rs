// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Command Test Gadget
//
// Test-only WASM gadget used by the Rust test suite in
// `src-tauri/src/wasm/runtime.rs`. Exercises the `command`
// host interface via `messaging::handle-message` dispatch
// so tests get a clean `Result<String, String>` oracle to
// assert against.
//
// Rebuild via `just build-test-fixtures` whenever the shared
// WIT world changes.
// =========================================================

wit_bindgen::generate!({
    path: "../../../../gadgets/gadget-sdk/wit",
    world: "gadget",
});

use exports::torchsnap::gadget::lifecycle::Guest as LifecycleGuest;
use exports::torchsnap::gadget::messaging::Guest as MessagingGuest;
use exports::torchsnap::gadget::search::{
    CatalogEntry, Guest as SearchGuest, PostAction, SearchResponse,
};
use exports::torchsnap::gadget::tasks::Guest as TasksGuest;
use torchsnap::gadget::command::{run, CommandOptions};

struct CommandPlugin;

export!(CommandPlugin);

impl LifecycleGuest for CommandPlugin {
    fn enable() -> Result<(), String> { Ok(()) }
    fn disable() {}
    fn on_setting_changed(_key: String, _value: String) {}
}

impl SearchGuest for CommandPlugin {
    fn entries() -> Vec<CatalogEntry> {
        Vec::new()
    }

    fn search(_query: String, _matched_prefix: Option<String>) -> SearchResponse {
        SearchResponse::Nothing
    }

    fn execute(_command: String) -> Result<PostAction, String> {
        Ok(PostAction::Nothing)
    }
}

impl MessagingGuest for CommandPlugin {
    /// Test dispatch table:
    ///
    /// - `"echo"` — run `/bin/echo <payload>`. Returns the captured
    ///   stdout on success. Hits the rule-allowed happy path.
    ///
    /// - `"deny:<binary>"` — run `<binary>` with no args. The
    ///   binary is not in any rule, so the host should return
    ///   `permission-denied`.
    ///
    /// - `"sleep:<seconds>"` — run `/bin/sh -c 'sleep N'` with a
    ///   100ms timeout. Used to exercise the timeout path.
    ///
    /// - `"flood:<bytes>"` — run `/bin/sh -c "head -c N /dev/zero"`
    ///   with a 64-byte output cap. Used to exercise the
    ///   `output-too-large` path.
    fn handle_message(method: String, payload: String) -> Result<String, String> {
        match method.as_str() {
            "echo" => {
                let opts = CommandOptions {
                    args: vec![payload],
                    cwd: None,
                    env: vec![],
                    stdin: None,
                    timeout_ms: Some(5_000),
                    max_output_bytes: None,
                };
                let result = run("/bin/echo", &opts).map_err(|e| format!("{e:?}"))?;
                Ok(String::from_utf8_lossy(&result.stdout).into_owned())
            }

            m if m.starts_with("deny:") => {
                let binary = m.trim_start_matches("deny:");
                let opts = CommandOptions {
                    args: vec![],
                    cwd: None,
                    env: vec![],
                    stdin: None,
                    timeout_ms: Some(5_000),
                    max_output_bytes: None,
                };
                let result = run(binary, &opts).map_err(|e| format!("{e:?}"))?;
                Ok(format!(
                    "unexpected success: status={:?}",
                    result.exit_code
                ))
            }

            m if m.starts_with("sleep:") => {
                let seconds = m.trim_start_matches("sleep:");
                let opts = CommandOptions {
                    args: vec!["-c".to_string(), format!("sleep {seconds}")],
                    cwd: None,
                    env: vec![],
                    stdin: None,
                    timeout_ms: Some(150),
                    max_output_bytes: None,
                };
                let result = run("/bin/sh", &opts).map_err(|e| format!("{e:?}"))?;
                Ok(format!(
                    "unexpected success: status={:?} timed_out={}",
                    result.exit_code, result.timed_out
                ))
            }

            m if m.starts_with("flood:") => {
                let bytes = m.trim_start_matches("flood:");
                let opts = CommandOptions {
                    args: vec!["-c".to_string(), format!("head -c {bytes} /dev/zero")],
                    cwd: None,
                    env: vec![],
                    stdin: None,
                    timeout_ms: Some(5_000),
                    max_output_bytes: Some(64),
                };
                let result = run("/bin/sh", &opts).map_err(|e| format!("{e:?}"))?;
                Ok(format!(
                    "unexpected success: stdout_len={}",
                    result.stdout.len()
                ))
            }

            _ => Err(format!("unknown method: {}", method)),
        }
    }
}

impl TasksGuest for CommandPlugin {
    fn run_task(_task_id: String) -> Result<(), String> {
        Ok(())
    }
}
