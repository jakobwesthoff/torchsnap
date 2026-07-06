// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Amphetamine.app keep-awake backend.
//!
//! Drives Amphetamine through `osascript`. Every piece of the
//! backend except the single `command::run` call is a pure
//! function: the AppleScript is a set of constants and one
//! builder ([`start_script`]), and interpreting the status
//! output is [`parse_status_output`]. The trait impl composes
//! these with the invocation so the moving parts stay
//! unit-testable without a live Amphetamine.
//!
//! The scripts double as the manifest's `command` permission:
//! the AppleScript shapes constrained in `manifest.toml` are
//! exactly the strings emitted here, verified by the
//! `manifest` consistency tests below. The constraints shape
//! the invocation surface as defense-in-depth (ADR 0040); they
//! are not a soundness boundary.

use std::time::Duration;

use torchsnap_gadget_sdk::command::{self, CommandError, CommandResult};

use super::{KeepAwakeBackend, SessionKind, SessionRequest, SessionStatus};

// =========================================================
// Timeouts
// =========================================================

/// Timeout for the status probe. It runs on the per-keystroke
/// search path, so it must stay well under the search-path
/// budget; Amphetamine answers a local Apple event in
/// milliseconds when it is running.
pub(crate) const STATUS_TIMEOUT: Duration = Duration::from_secs(5);

/// Timeout for start/stop. These are deliberate user actions,
/// so a generous window is fine; 60s is also the host's hard
/// ceiling on `command::run` (there is no unlimited timeout).
pub(crate) const ACTION_TIMEOUT: Duration = Duration::from_secs(60);

// =========================================================
// AppleScript
// =========================================================

/// Query the current session without launching Amphetamine.
///
/// The `application "Amphetamine" is running` guard is a plain
/// process check that does not start the app; only inside the
/// guard do we send Apple events (which would otherwise launch
/// it). The single returned line is either `not-running` or
/// `<active>|<remaining>|<displayAllowed>`, where `<active>`
/// and `<displayAllowed>` are `true`/`false` and `<remaining>`
/// is the session seconds remaining (see [`parse_status_output`]
/// for the sentinel encoding). Indentation is intentionally
/// omitted so the string carries no tab/space ambiguity across
/// the Rust constant and the manifest regex.
pub(crate) const STATUS_SCRIPT: &str = "\
if application \"Amphetamine\" is running then\n\
tell application \"Amphetamine\"\n\
set isActive to session is active\n\
set remaining to session time remaining\n\
set displayAllowed to display sleep allowed\n\
end tell\n\
return isActive as text & \"|\" & remaining as text & \"|\" & displayAllowed as text\n\
else\n\
return \"not-running\"\n\
end if";

/// End the current session. A no-op when nothing is running.
pub(crate) const STOP_SCRIPT: &str = "tell application \"Amphetamine\" to end session";

/// Build the AppleScript that starts a session.
///
/// Amphetamine's `start new session` takes a duration and an
/// interval unit. A timed session passes the minute count as
/// the duration with the `minutes` interval. An infinite
/// session is encoded as `duration:0, interval:0`;
/// `session time remaining` then reports `0`, which
/// [`parse_status_output`] maps back to [`SessionKind::Infinite`].
pub(crate) fn start_script(minutes: Option<u32>, display_sleep_allowed: bool) -> String {
    let (duration, interval) = match minutes {
        Some(m) => (m, "minutes"),
        None => (0, "0"),
    };
    format!(
        "tell application \"Amphetamine\" to start new session with options \
         {{duration:{duration}, interval:{interval}, displaySleepAllowed:{display_sleep_allowed}}}"
    )
}

// =========================================================
// Status parsing
// =========================================================

/// Interpret the [`STATUS_SCRIPT`] stdout into a
/// [`SessionStatus`].
///
/// The line is `not-running`, or three `|`-separated fields:
/// the active flag, the seconds remaining, and the
/// display-sleep flag. `session time remaining` overloads its
/// sign to encode the session kind: `0` is an infinite manual
/// session, a positive value is the countdown of a timed
/// session, and `-1`/`-2` mark a session Amphetamine manages
/// itself (Trigger / app / date based). When the active flag
/// is false the remaining value is not interpreted.
pub(crate) fn parse_status_output(stdout: &str) -> Result<SessionStatus, String> {
    let trimmed = stdout.trim();

    if trimmed == "not-running" {
        return Ok(SessionStatus::AppNotRunning);
    }

    let fields: Vec<&str> = trimmed.split('|').collect();
    if fields.len() != 3 {
        return Err(format!("unexpected Amphetamine status output: {trimmed:?}"));
    }

    let active = parse_bool(fields[0])
        .ok_or_else(|| format!("unexpected active flag in status output: {:?}", fields[0]))?;
    let remaining: i64 = fields[1]
        .parse()
        .map_err(|_| format!("unexpected remaining value in status output: {:?}", fields[1]))?;
    let display_sleep_allowed = parse_bool(fields[2])
        .ok_or_else(|| format!("unexpected display flag in status output: {:?}", fields[2]))?;

    if !active {
        return Ok(SessionStatus::Inactive);
    }

    let kind = match remaining {
        0 => SessionKind::Infinite,
        secs if secs > 0 => {
            let remaining_secs = u32::try_from(secs)
                .map_err(|_| format!("remaining seconds out of range in status output: {secs}"))?;
            SessionKind::Timed { remaining_secs }
        }
        -1 | -2 => SessionKind::External,
        // `-3` conventionally means "no session"; combined with
        // an active flag it is contradictory, as is any other
        // negative sentinel we don't recognize.
        other => {
            return Err(format!(
                "contradictory Amphetamine status: active session with remaining {other}"
            ));
        }
    };

    Ok(SessionStatus::Active {
        kind,
        display_sleep_allowed,
    })
}

/// Parse the exact `true`/`false` AppleScript boolean text.
fn parse_bool(value: &str) -> Option<bool> {
    match value {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

// =========================================================
// Invocation
// =========================================================

/// The Amphetamine backend. Stateless: every method drives a
/// single `osascript` call and all session state lives in
/// Amphetamine itself.
pub(crate) struct AmphetamineBackend;

impl KeepAwakeBackend for AmphetamineBackend {
    fn status(&self) -> Result<SessionStatus, String> {
        let result = run_osascript(STATUS_SCRIPT, STATUS_TIMEOUT)?;
        parse_status_output(&String::from_utf8_lossy(&result.stdout))
    }

    fn start(&self, req: &SessionRequest) -> Result<(), String> {
        let script = start_script(req.minutes, req.display_sleep_allowed);
        run_osascript(&script, ACTION_TIMEOUT).map(|_| ())
    }

    fn stop(&self) -> Result<(), String> {
        run_osascript(STOP_SCRIPT, ACTION_TIMEOUT).map(|_| ())
    }
}

/// Run one `osascript -e <script>` call and return the result
/// only on a clean exit. Failures are turned into
/// launcher-subtitle-friendly messages.
fn run_osascript(script: &str, timeout: Duration) -> Result<CommandResult, String> {
    let result = command::run("osascript")
        .arg("-e")
        .arg(script)
        .timeout(timeout)
        .invoke()
        .map_err(describe_command_error)?;

    if result.timed_out {
        return Err("Amphetamine did not respond in time".to_string());
    }

    match result.exit_code {
        Some(0) => Ok(result),
        _ => Err(describe_exit_failure(&result)),
    }
}

/// Map a host `command::run` error to a user-facing message.
fn describe_command_error(error: CommandError) -> String {
    match error {
        CommandError::PermissionDenied(detail) => format!("osascript is not permitted: {detail}"),
        CommandError::SpawnFailed(detail) => format!("could not launch osascript: {detail}"),
        CommandError::Timeout => "Amphetamine did not respond in time".to_string(),
        CommandError::OutputTooLarge(_) => {
            "Amphetamine returned an unexpectedly large response".to_string()
        }
    }
}

/// Build a message for a spawn that ran but exited non-zero or
/// was signaled. Prefers Amphetamine's own stderr when present.
fn describe_exit_failure(result: &CommandResult) -> String {
    let stderr = String::from_utf8_lossy(&result.stderr);
    let stderr = stderr.trim();
    if !stderr.is_empty() {
        return format!("Amphetamine command failed: {stderr}");
    }
    match (result.exit_code, &result.signal) {
        (Some(code), _) => format!("Amphetamine command failed (exit code {code})"),
        (None, Some(signal)) => format!("Amphetamine command was terminated by signal {signal}"),
        (None, None) => "Amphetamine command failed".to_string(),
    }
}

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ----- start_script --------------------------------------

    #[test]
    fn start_script_timed_display_disallowed() {
        assert_eq!(
            start_script(Some(30), false),
            "tell application \"Amphetamine\" to start new session with options \
             {duration:30, interval:minutes, displaySleepAllowed:false}"
        );
    }

    #[test]
    fn start_script_timed_display_allowed() {
        assert_eq!(
            start_script(Some(30), true),
            "tell application \"Amphetamine\" to start new session with options \
             {duration:30, interval:minutes, displaySleepAllowed:true}"
        );
    }

    #[test]
    fn start_script_infinite_display_disallowed() {
        assert_eq!(
            start_script(None, false),
            "tell application \"Amphetamine\" to start new session with options \
             {duration:0, interval:0, displaySleepAllowed:false}"
        );
    }

    #[test]
    fn start_script_infinite_display_allowed() {
        assert_eq!(
            start_script(None, true),
            "tell application \"Amphetamine\" to start new session with options \
             {duration:0, interval:0, displaySleepAllowed:true}"
        );
    }

    #[test]
    fn start_script_large_minutes() {
        assert_eq!(
            start_script(Some(u32::MAX), false),
            format!(
                "tell application \"Amphetamine\" to start new session with options \
                 {{duration:{}, interval:minutes, displaySleepAllowed:false}}",
                u32::MAX
            )
        );
    }

    // ----- parse_status_output -------------------------------

    #[test]
    fn parse_not_running_without_trailing_newline() {
        assert_eq!(
            parse_status_output("not-running"),
            Ok(SessionStatus::AppNotRunning)
        );
    }

    #[test]
    fn parse_not_running_with_trailing_newline() {
        assert_eq!(
            parse_status_output("not-running\n"),
            Ok(SessionStatus::AppNotRunning)
        );
    }

    #[test]
    fn parse_inactive() {
        assert_eq!(
            parse_status_output("false|-3|false"),
            Ok(SessionStatus::Inactive)
        );
    }

    #[test]
    fn parse_inactive_ignores_remaining_and_display() {
        // With no session, Amphetamine still reports a numeric
        // remaining and a display flag; neither is interpreted.
        assert_eq!(
            parse_status_output("false|0|true"),
            Ok(SessionStatus::Inactive)
        );
    }

    #[test]
    fn parse_active_infinite() {
        assert_eq!(
            parse_status_output("true|0|false"),
            Ok(SessionStatus::Active {
                kind: SessionKind::Infinite,
                display_sleep_allowed: false,
            })
        );
    }

    #[test]
    fn parse_active_timed() {
        assert_eq!(
            parse_status_output("true|119|false"),
            Ok(SessionStatus::Active {
                kind: SessionKind::Timed { remaining_secs: 119 },
                display_sleep_allowed: false,
            })
        );
    }

    #[test]
    fn parse_active_timed_with_trailing_newline() {
        assert_eq!(
            parse_status_output("true|3847|false\n"),
            Ok(SessionStatus::Active {
                kind: SessionKind::Timed { remaining_secs: 3847 },
                display_sleep_allowed: false,
            })
        );
    }

    #[test]
    fn parse_display_flag_both_values() {
        assert_eq!(
            parse_status_output("true|60|true"),
            Ok(SessionStatus::Active {
                kind: SessionKind::Timed { remaining_secs: 60 },
                display_sleep_allowed: true,
            })
        );
        assert_eq!(
            parse_status_output("true|60|false"),
            Ok(SessionStatus::Active {
                kind: SessionKind::Timed { remaining_secs: 60 },
                display_sleep_allowed: false,
            })
        );
    }

    #[test]
    fn parse_external_minus_one() {
        assert_eq!(
            parse_status_output("true|-1|false"),
            Ok(SessionStatus::Active {
                kind: SessionKind::External,
                display_sleep_allowed: false,
            })
        );
    }

    #[test]
    fn parse_external_minus_two() {
        assert_eq!(
            parse_status_output("true|-2|true"),
            Ok(SessionStatus::Active {
                kind: SessionKind::External,
                display_sleep_allowed: true,
            })
        );
    }

    #[test]
    fn parse_contradictory_active_minus_three_is_error() {
        assert!(parse_status_output("true|-3|false").is_err());
    }

    #[test]
    fn parse_other_negative_remaining_is_error() {
        assert!(parse_status_output("true|-9|false").is_err());
    }

    #[test]
    fn parse_empty_is_error() {
        assert!(parse_status_output("").is_err());
        assert!(parse_status_output("\n").is_err());
    }

    #[test]
    fn parse_wrong_field_count_is_error() {
        assert!(parse_status_output("true|0").is_err());
        assert!(parse_status_output("true").is_err());
        assert!(parse_status_output("true|0|false|extra").is_err());
    }

    #[test]
    fn parse_non_numeric_remaining_is_error() {
        assert!(parse_status_output("true|abc|false").is_err());
        assert!(parse_status_output("true|1.5|false").is_err());
    }

    #[test]
    fn parse_garbage_booleans_are_errors() {
        assert!(parse_status_output("yes|0|false").is_err());
        assert!(parse_status_output("true|0|maybe").is_err());
        assert!(parse_status_output("1|0|0").is_err());
    }

    // ----- timeouts ------------------------------------------

    #[test]
    fn status_timeout_within_search_budget() {
        assert!(STATUS_TIMEOUT <= Duration::from_secs(5));
    }

    #[test]
    fn action_timeout_is_host_ceiling() {
        assert_eq!(ACTION_TIMEOUT, Duration::from_secs(60));
    }

    // ----- error mapping -------------------------------------

    fn result_with(exit_code: Option<i32>, signal: Option<&str>, stderr: &str) -> CommandResult {
        CommandResult {
            exit_code,
            signal: signal.map(str::to_string),
            timed_out: false,
            stdout: Vec::new(),
            stderr: stderr.as_bytes().to_vec(),
        }
    }

    #[test]
    fn exit_failure_prefers_stderr() {
        let msg = describe_exit_failure(&result_with(Some(1), None, "1:1: syntax error\n"));
        assert!(msg.contains("1:1: syntax error"), "got: {msg}");
    }

    #[test]
    fn exit_failure_falls_back_to_exit_code() {
        let msg = describe_exit_failure(&result_with(Some(2), None, ""));
        assert!(msg.contains("exit code 2"), "got: {msg}");
    }

    #[test]
    fn exit_failure_reports_signal_when_no_code() {
        let msg = describe_exit_failure(&result_with(None, Some("KILL"), ""));
        assert!(msg.contains("KILL"), "got: {msg}");
    }

    #[test]
    fn command_error_messages_are_nonempty() {
        assert!(!describe_command_error(CommandError::Timeout).is_empty());
        assert!(!describe_command_error(CommandError::PermissionDenied("x".into())).is_empty());
        assert!(!describe_command_error(CommandError::SpawnFailed("x".into())).is_empty());
        assert!(
            !describe_command_error(CommandError::OutputTooLarge((Vec::new(), Vec::new())))
                .is_empty()
        );
    }

    // ----- manifest consistency ------------------------------
    //
    // The manifest's `osascript` command rule(s) must accept
    // exactly the scripts this module emits and nothing else.
    // These tests compile the manifest constraints the way the
    // host does and check that every emitted script matches
    // exactly one rule while a hostile script matches none.
    mod manifest {
        use regex::Regex;

        use super::super::{STATUS_SCRIPT, STOP_SCRIPT, start_script};

        const MANIFEST: &str = include_str!("../../manifest.toml");

        /// The subset of argv constraint kinds this gadget uses,
        /// compiled the way the host compiles them: `literal` is
        /// byte equality, `enum` is membership, `regex` is an
        /// anchored full-string match (`^(?:pattern)$`).
        enum Constraint {
            Literal(String),
            Enum(Vec<String>),
            Regex(Regex),
        }

        impl Constraint {
            fn matches(&self, arg: &str) -> bool {
                match self {
                    Constraint::Literal(value) => value == arg,
                    Constraint::Enum(values) => values.iter().any(|v| v == arg),
                    Constraint::Regex(re) => re.is_match(arg),
                }
            }
        }

        struct Rule {
            argv: Vec<Constraint>,
        }

        impl Rule {
            fn accepts(&self, argv: &[&str]) -> bool {
                self.argv.len() == argv.len()
                    && self.argv.iter().zip(argv).all(|(c, a)| c.matches(a))
            }
        }

        /// Parse every `[[permissions.command]]` rule for
        /// `osascript` out of the manifest.
        fn osascript_rules() -> Vec<Rule> {
            let doc: toml::Value = toml::from_str(MANIFEST).expect("manifest parses");
            let rules = doc
                .get("permissions")
                .and_then(|p| p.get("command"))
                .and_then(|c| c.as_array())
                .cloned()
                .unwrap_or_default();

            rules
                .into_iter()
                .filter(|rule| {
                    rule.get("binary").and_then(|b| b.as_str()) == Some("osascript")
                })
                .map(|rule| {
                    let argv = rule
                        .get("argv")
                        .and_then(|a| a.as_array())
                        .cloned()
                        .unwrap_or_default();
                    let argv = argv.into_iter().map(compile_constraint).collect();
                    Rule { argv }
                })
                .collect()
        }

        fn compile_constraint(constraint: toml::Value) -> Constraint {
            let kind = constraint
                .get("kind")
                .and_then(|k| k.as_str())
                .expect("constraint has a kind");
            match kind {
                "literal" => Constraint::Literal(
                    constraint
                        .get("value")
                        .and_then(|v| v.as_str())
                        .expect("literal has a value")
                        .to_string(),
                ),
                "enum" => Constraint::Enum(
                    constraint
                        .get("values")
                        .and_then(|v| v.as_array())
                        .expect("enum has values")
                        .iter()
                        .map(|v| v.as_str().expect("enum value is a string").to_string())
                        .collect(),
                ),
                "regex" => {
                    let pattern = constraint
                        .get("pattern")
                        .and_then(|p| p.as_str())
                        .expect("regex has a pattern");
                    Constraint::Regex(
                        Regex::new(&format!("^(?:{pattern})$")).expect("manifest regex compiles"),
                    )
                }
                other => panic!("unexpected constraint kind in manifest: {other}"),
            }
        }

        fn match_count(rules: &[Rule], argv: &[&str]) -> usize {
            rules.iter().filter(|r| r.accepts(argv)).count()
        }

        fn assert_exactly_one(script: &str) {
            let rules = osascript_rules();
            assert_eq!(
                match_count(&rules, &["-e", script]),
                1,
                "script must be accepted by exactly one rule:\n{script}"
            );
        }

        #[test]
        fn status_script_matches_exactly_one_rule() {
            assert_exactly_one(STATUS_SCRIPT);
        }

        #[test]
        fn stop_script_matches_exactly_one_rule() {
            assert_exactly_one(STOP_SCRIPT);
        }

        #[test]
        fn start_scripts_match_exactly_one_rule() {
            for minutes in [Some(1u32), Some(30), Some(u32::MAX), None] {
                for display in [true, false] {
                    assert_exactly_one(&start_script(minutes, display));
                }
            }
        }

        #[test]
        fn malicious_script_matches_no_rule() {
            let rules = osascript_rules();
            assert_eq!(
                match_count(&rules, &["-e", "do shell script \"rm -rf ~\""]),
                0,
                "a hostile script must not be accepted by any rule"
            );
        }

        #[test]
        fn wrong_flag_matches_no_rule() {
            // The `-e` literal at position 0 is load-bearing:
            // any other flag must be rejected.
            let rules = osascript_rules();
            assert_eq!(match_count(&rules, &["-l", STOP_SCRIPT]), 0);
        }

        #[test]
        fn extra_argument_matches_no_rule() {
            let rules = osascript_rules();
            assert_eq!(match_count(&rules, &["-e", STOP_SCRIPT, "extra"]), 0);
        }
    }
}
