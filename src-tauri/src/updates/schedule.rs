// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! When the next automatic update check is due.
//!
//! The scheduler wakes up on a short tick and asks [`check_is_due`]
//! instead of sleeping for the whole interval. A sleep of 24 hours
//! would drift with the Mac's sleep and clock changes; comparing wall
//! clock timestamps on every tick does not.
//!
//! All times are Unix seconds, as stored in the settings.

use std::time::Duration;

/// Time between two successful automatic checks.
pub const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

/// Minimum time between a failed check and the next attempt, so an
/// offline Mac retries within the hour instead of waiting a whole day,
/// without asking the feed on every tick.
pub const RETRY_AFTER_FAILURE: Duration = Duration::from_secs(60 * 60);

/// Whether an automatic check should run now.
///
/// `last_check` is the last successful check, persisted across
/// restarts. `last_failure` is the last failed attempt since this
/// process started; failures do not advance `last_check`.
///
/// A `last_check` in the future (the clock was set back) counts as
/// due, otherwise the check would stay silent until the clock caught
/// up again.
pub fn check_is_due(
    now: i64,
    last_check: Option<i64>,
    last_failure: Option<i64>,
    interval: Duration,
) -> bool {
    if let Some(failed_at) = last_failure
        && failed_at <= now
        && now - failed_at < RETRY_AFTER_FAILURE.as_secs() as i64
    {
        return false;
    }
    match last_check {
        None => true,
        Some(checked_at) if checked_at > now => true,
        Some(checked_at) => now - checked_at >= interval.as_secs() as i64,
    }
}

/// Environment variable that shortens or lengthens [`CHECK_INTERVAL`]
/// for testing, in seconds.
pub const INTERVAL_OVERRIDE_VAR: &str = "TORCHSNAP_UPDATE_INTERVAL";

/// Parse the interval override. Unset or blank means the default; a
/// value that is not a positive number of seconds is an error.
pub fn parse_interval_override(value: Option<&str>) -> anyhow::Result<Option<Duration>> {
    match value.map(str::trim) {
        None | Some("") => Ok(None),
        Some(raw) => match raw.parse::<u64>() {
            Ok(seconds) if seconds > 0 => Ok(Some(Duration::from_secs(seconds))),
            _ => anyhow::bail!(
                "{INTERVAL_OVERRIDE_VAR} must be a positive number of seconds, not '{raw}'"
            ),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 24 * 60 * 60;
    const HOUR: i64 = 60 * 60;
    const NOW: i64 = 1_790_000_000;

    #[test]
    fn never_checked_is_due() {
        assert!(check_is_due(NOW, None, None, CHECK_INTERVAL));
    }

    #[test]
    fn due_once_the_interval_has_passed() {
        assert!(!check_is_due(
            NOW,
            Some(NOW - DAY + 1),
            None,
            CHECK_INTERVAL
        ));
        assert!(check_is_due(NOW, Some(NOW - DAY), None, CHECK_INTERVAL));
    }

    #[test]
    fn last_check_in_the_future_is_due() {
        assert!(check_is_due(NOW, Some(NOW + HOUR), None, CHECK_INTERVAL));
    }

    #[test]
    fn a_failure_holds_the_next_attempt_back_for_an_hour() {
        let failed_at = NOW;
        // Failed at 09:00: not again at 09:15, again at 10:00.
        assert!(!check_is_due(
            failed_at + 15 * 60,
            None,
            Some(failed_at),
            CHECK_INTERVAL
        ));
        assert!(check_is_due(
            failed_at + HOUR,
            None,
            Some(failed_at),
            CHECK_INTERVAL
        ));
    }

    #[test]
    fn a_failure_does_not_make_a_recent_check_due() {
        let checked_at = NOW - HOUR * 2;
        assert!(!check_is_due(
            NOW,
            Some(checked_at),
            Some(NOW - HOUR * 3),
            CHECK_INTERVAL
        ));
    }

    #[test]
    fn a_failure_in_the_future_does_not_block() {
        assert!(check_is_due(NOW, None, Some(NOW + HOUR), CHECK_INTERVAL));
    }

    #[test]
    fn interval_override() {
        assert_eq!(parse_interval_override(None).unwrap(), None);
        assert_eq!(parse_interval_override(Some(" ")).unwrap(), None);
        assert_eq!(
            parse_interval_override(Some("120")).unwrap(),
            Some(Duration::from_secs(120))
        );
        assert!(parse_interval_override(Some("0")).is_err());
        assert!(parse_interval_override(Some("a day")).is_err());
    }
}
