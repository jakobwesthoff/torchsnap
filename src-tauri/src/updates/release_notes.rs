// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The release notes the update window shows.
//!
//! The feed carries a `releases` array next to the fields the plugin
//! reads (ADR 0053): version, date and Markdown notes of every stable
//! release. Only the downloaded archive is signed, not the feed, so
//! this input is untrusted: entries that do not parse are skipped, and
//! the number of entries and the length of each text are capped.

use semver::Version;
use serde::Serialize;
use serde_json::Value;

/// More than every release this project will ever have between two
/// installs; anything beyond is dropped.
const MAX_RELEASES: usize = 100;
/// A CHANGELOG section is a few kilobytes; longer notes are cut.
const MAX_NOTES_BYTES: usize = 32 * 1024;
const MAX_DATE_BYTES: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReleaseNotes {
    pub version: String,
    pub date: Option<String>,
    pub notes: String,
}

/// The releases newer than `installed` and not newer than `offered`,
/// newest first.
///
/// When the feed has no usable `releases`, the offered version's own
/// `notes` stand in, so the window never shows nothing.
pub fn notes_between(
    feed: &Value,
    installed: &Version,
    offered: &Version,
    offered_notes: Option<&str>,
) -> Vec<ReleaseNotes> {
    let mut releases: Vec<(Version, ReleaseNotes)> = feed
        .get("releases")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .take(MAX_RELEASES)
                .filter_map(parse_entry)
                .filter(|(version, _)| version > installed && version <= offered)
                .collect()
        })
        .unwrap_or_default();

    if releases.is_empty() {
        return vec![ReleaseNotes {
            version: offered.to_string(),
            date: None,
            notes: truncate(offered_notes.unwrap_or_default(), MAX_NOTES_BYTES),
        }];
    }

    releases.sort_by(|(a, _), (b, _)| b.cmp(a));
    releases.dedup_by(|(a, _), (b, _)| a == b);
    releases.into_iter().map(|(_, notes)| notes).collect()
}

fn parse_entry(entry: &Value) -> Option<(Version, ReleaseNotes)> {
    let version = Version::parse(entry.get("version")?.as_str()?).ok()?;
    let notes = entry.get("notes")?.as_str()?;
    let date = entry
        .get("date")
        .and_then(Value::as_str)
        .map(|date| truncate(date, MAX_DATE_BYTES));
    Some((
        version.clone(),
        ReleaseNotes {
            version: version.to_string(),
            date,
            notes: truncate(notes, MAX_NOTES_BYTES),
        },
    ))
}

/// Cut `text` to at most `max` bytes on a character boundary.
fn truncate(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn v(text: &str) -> Version {
        Version::parse(text).expect("test versions are valid")
    }

    fn versions(notes: &[ReleaseNotes]) -> Vec<&str> {
        notes.iter().map(|n| n.version.as_str()).collect()
    }

    #[test]
    fn keeps_versions_after_the_installed_one_up_to_the_offered_one() {
        let feed = json!({ "releases": [
            { "version": "0.13.0", "date": "2026-11-01", "notes": "c" },
            { "version": "0.12.1", "date": "2026-10-10", "notes": "b" },
            { "version": "0.12.0", "date": "2026-10-02", "notes": "a" },
            { "version": "0.11.1", "date": "2026-09-24", "notes": "old" },
        ]});
        let notes = notes_between(&feed, &v("0.12.0"), &v("0.12.1"), None);
        assert_eq!(versions(&notes), ["0.12.1"]);
        assert_eq!(notes[0].date.as_deref(), Some("2026-10-10"));
    }

    #[test]
    fn sorts_newest_first_and_drops_duplicates() {
        let feed = json!({ "releases": [
            { "version": "0.12.0", "notes": "a" },
            { "version": "0.12.1", "notes": "b" },
            { "version": "0.12.1", "notes": "b again" },
        ]});
        let notes = notes_between(&feed, &v("0.11.1"), &v("0.12.1"), None);
        assert_eq!(versions(&notes), ["0.12.1", "0.12.0"]);
    }

    #[test]
    fn skips_entries_that_do_not_parse() {
        let feed = json!({ "releases": [
            { "version": "not-a-version", "notes": "x" },
            { "version": "0.12.0" },
            { "version": 12, "notes": "x" },
            "just a string",
            { "version": "0.12.0", "notes": "ok" },
        ]});
        let notes = notes_between(&feed, &v("0.11.1"), &v("0.12.0"), None);
        assert_eq!(versions(&notes), ["0.12.0"]);
        assert_eq!(notes[0].notes, "ok");
    }

    #[test]
    fn falls_back_to_the_offered_notes() {
        let feed = json!({ "version": "0.12.0" });
        let notes = notes_between(&feed, &v("0.11.1"), &v("0.12.0"), Some("plugin notes"));
        assert_eq!(
            notes,
            [ReleaseNotes {
                version: "0.12.0".into(),
                date: None,
                notes: "plugin notes".into()
            }]
        );
    }

    #[test]
    fn caps_the_length_of_notes_on_a_character_boundary() {
        let long = "ä".repeat(MAX_NOTES_BYTES);
        let feed = json!({ "releases": [{ "version": "0.12.0", "notes": long }] });
        let notes = notes_between(&feed, &v("0.11.1"), &v("0.12.0"), None);
        assert!(notes[0].notes.len() <= MAX_NOTES_BYTES);
        assert!(notes[0].notes.chars().all(|c| c == 'ä'));
    }

    #[test]
    fn caps_the_number_of_entries() {
        let entries: Vec<Value> = (0..MAX_RELEASES + 50)
            .map(|minor| json!({ "version": format!("1.{minor}.0"), "notes": "n" }))
            .collect();
        let feed = json!({ "releases": entries });
        let notes = notes_between(&feed, &v("0.1.0"), &v("9.0.0"), None);
        assert_eq!(notes.len(), MAX_RELEASES);
    }
}
