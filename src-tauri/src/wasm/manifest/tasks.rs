// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use serde::{Deserialize, Serialize};

// =========================================================
// Scheduled tasks
// =========================================================

/// `[[tasks]]` entry — a single scheduled background task.
///
/// `schedule` is a 5-field POSIX cron expression
/// (`minute hour day month weekday`). The host parses and
/// validates it at manifest load time and stores the parsed
/// `cron::Schedule` separately in the bridge — this struct
/// only carries the raw user-facing fields so that
/// (de)serialization stays straightforward.
///
/// Sub-minute scheduling is rejected — `cron`'s
/// underlying syntax is 6/7-field, but gadgets use the
/// stricter 5-field POSIX form so the schedule space is
/// predictable and there's no chance of accidentally
/// scheduling a task at the second-resolution.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskDef {
    /// Unique task identifier within the gadget. Passed
    /// back to the guest via `tasks::run-task(task-id)`
    /// when the cron schedule fires.
    pub id: String,

    /// 5-field POSIX cron expression
    /// (`minute hour day month weekday`).
    pub schedule: String,
}

// =========================================================
// Task definition validation
// =========================================================

/// Verify that every `[[tasks]]` entry parses as a valid
/// 5-field POSIX cron expression and that no two tasks
/// share the same id.
///
/// Both checks happen at manifest load time so that broken
/// schedules surface as clean gadget-load errors instead of
/// crashing the scheduler later.
pub(crate) fn validate_task_definitions(tasks: &[TaskDef]) -> anyhow::Result<()> {
    let mut seen_ids: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for task in tasks {
        if !seen_ids.insert(task.id.as_str()) {
            anyhow::bail!("duplicate scheduled task id `{}`", task.id);
        }
        parse_cron_schedule(&task.schedule)
            .map_err(|e| anyhow::anyhow!("invalid schedule for task `{}`: {e}", task.id))?;
    }
    Ok(())
}

/// Parse a 5-field POSIX cron expression
/// (`minute hour day month weekday`) into a
/// `cron::Schedule`.
///
/// The `cron` crate uses Quartz-style 6/7-field syntax
/// (`sec min hour day month dow [year]`), so we wrap the
/// user's 5 fields with `0` for seconds and `*` for year.
/// A pre-check on the field count catches the most common
/// authoring errors — wrong number of fields, Quartz macros
/// like `@daily` — with a friendly message before delegating
/// to `cron` for full validation.
pub(crate) fn parse_cron_schedule(schedule: &str) -> anyhow::Result<cron::Schedule> {
    use std::str::FromStr;

    // Pre-check the field count so gadget authors who pass
    // a 4/6/7-field expression get a clear "expected
    // 5-field POSIX cron" message instead of an opaque
    // Quartz-internal error from the `cron` crate. The
    // wrapping below is still the actual validation
    // mechanism — this check just catches the common
    // failure modes early with a friendlier explanation.
    let field_count = schedule.split_whitespace().count();
    if field_count != 5 {
        anyhow::bail!(
            "expected 5-field POSIX cron `minute hour day month weekday`, got {field_count} field(s) — sub-minute scheduling and 6/7-field Quartz syntax are not supported"
        );
    }

    // The `cron` crate uses Quartz-style 6/7-field syntax
    // (`sec min hour day month dow [year]`), so we wrap the
    // user's 5 fields with `0` for seconds and `*` for year.
    // This wrapping is also a defensive validation: a valid
    // 5-field POSIX expression becomes a valid 7-field
    // Quartz expression that `cron::Schedule::from_str`
    // accepts; a malformed expression that somehow has 5
    // tokens but isn't valid POSIX cron becomes a malformed
    // 7-field Quartz expression that the parser rejects
    // with its own error.
    let normalized = format!("0 {schedule} *");
    cron::Schedule::from_str(&normalized).map_err(|e| anyhow::anyhow!("{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str, schedule: &str) -> TaskDef {
        TaskDef {
            id: id.to_string(),
            schedule: schedule.to_string(),
        }
    }

    #[test]
    fn empty_task_list_is_ok() {
        validate_task_definitions(&[]).unwrap();
    }

    #[test]
    fn valid_5_field_cron_accepted() {
        validate_task_definitions(&[task("cleanup", "*/30 * * * *")]).unwrap();
        validate_task_definitions(&[task("daily", "0 4 * * *")]).unwrap();
    }

    #[test]
    fn six_field_cron_rejected() {
        // Quartz-style 6-field input — explicitly out of
        // scope so gadgets don't accidentally schedule at
        // second resolution. The cron crate's parser
        // rejects the resulting 8-field intermediate.
        let err = validate_task_definitions(&[task("bad", "0 */30 * * * *")]).unwrap_err();
        assert!(err.to_string().contains("schedule"), "{err}");
    }

    #[test]
    fn malformed_cron_rejected() {
        let err = validate_task_definitions(&[task("bad", "not a cron")]).unwrap_err();
        assert!(err.to_string().contains("schedule"), "{err}");
    }

    #[test]
    fn duplicate_task_ids_rejected() {
        let err = validate_task_definitions(&[
            task("cleanup", "*/30 * * * *"),
            task("cleanup", "0 0 * * *"),
        ])
        .unwrap_err();
        assert!(err.to_string().contains("duplicate"), "{err}");
    }

    #[test]
    fn empty_schedule_rejected() {
        let err = validate_task_definitions(&[task("bad", "")]).unwrap_err();
        assert!(err.to_string().contains("5-field"), "{err}");
    }

    #[test]
    fn four_field_schedule_rejected() {
        let err = validate_task_definitions(&[task("bad", "* * * *")]).unwrap_err();
        assert!(err.to_string().contains("5-field"), "{err}");
    }

    #[test]
    fn seven_field_schedule_rejected() {
        let err = validate_task_definitions(&[task("bad", "0 */30 * * * * *")]).unwrap_err();
        assert!(err.to_string().contains("5-field"), "{err}");
    }

    #[test]
    fn all_wildcards_5_field_accepted() {
        // The simplest legal POSIX cron expression — fires
        // every minute. Confirms the base case parses.
        validate_task_definitions(&[task("ok", "* * * * *")]).unwrap();
    }

    #[test]
    fn quartz_macro_at_daily_rejected() {
        // `@daily` is a Quartz alias for `0 0 * * *`, but
        // it's a single-token whole-expression macro. After
        // wrapping it becomes `0 @daily *` which the cron
        // crate rejects. The friendlier error wrapper catches
        // it as a 1-field input first.
        let err = validate_task_definitions(&[task("bad", "@daily")]).unwrap_err();
        assert!(err.to_string().contains("5-field"), "{err}");
    }
}
