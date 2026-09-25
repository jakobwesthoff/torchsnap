// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Global shortcut planning
//
// Before touching the OS registration, the host decides which
// shortcuts it can register at all. Every shortcut is checked on its
// own, so one missing, invalid or duplicate combo costs only that
// shortcut. The problems found here, plus the ones the OS reports
// while registering, are kept per settings key so Settings can show
// them next to the shortcut they belong to.
// =========================================================

use std::collections::BTreeMap;
use std::sync::Mutex;

use serde::Serialize;
use tauri_plugin_global_shortcut::Shortcut;

/// Settings key of the launcher toggle shortcut.
pub const LAUNCHER_SETTINGS_KEY: &str = "globalShortcut";

/// Emitted to every window after the shortcuts were registered again.
pub const SHORTCUT_PROBLEMS_CHANGED: &str = "shortcut-problems-changed";

/// A shortcut the host wants registered.
pub struct ShortcutRequest<T> {
    /// Where the combo is stored, which is also how Settings finds
    /// the problem for its row.
    pub settings_key: String,
    /// Name shown to the user when another shortcut collides with
    /// this one.
    pub label: String,
    /// The stored combo, or `None` when nothing is stored.
    pub combo: Option<String>,
    /// What pressing the shortcut does, passed through untouched.
    pub target: T,
}

/// Why a shortcut is not registered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ShortcutProblem {
    /// No combo is stored for the shortcut.
    Missing,
    /// The stored combo is not a valid accelerator.
    Invalid { combo: String },
    /// A shortcut earlier in the order already has the same combo.
    TakenBy { label: String },
    /// The OS or the shortcut plugin refused to register the combo.
    Rejected { reason: String },
}

/// The outcome of planning: shortcuts to register in order, and
/// problems keyed by settings key.
pub struct ShortcutPlan<T> {
    pub accepted: Vec<(Shortcut, ShortcutRequest<T>)>,
    pub problems: BTreeMap<String, ShortcutProblem>,
}

/// Decide which requests can be registered. Requests are taken in the
/// given order, so an earlier request keeps a combo that a later one
/// repeats; the caller puts the launcher first, then gadgets in their
/// registration order.
pub fn plan_shortcuts<T>(requests: Vec<ShortcutRequest<T>>) -> ShortcutPlan<T> {
    let mut accepted: Vec<(Shortcut, ShortcutRequest<T>)> = Vec::new();
    let mut problems = BTreeMap::new();

    for request in requests {
        let Some(combo) = request.combo.as_deref() else {
            problems.insert(request.settings_key, ShortcutProblem::Missing);
            continue;
        };
        let Ok(shortcut) = combo.parse::<Shortcut>() else {
            let combo = combo.to_string();
            problems.insert(request.settings_key, ShortcutProblem::Invalid { combo });
            continue;
        };
        // Collisions are found on the parsed shortcut, so different
        // spellings of one combo collide too.
        if let Some((_, holder)) = accepted.iter().find(|(taken, _)| *taken == shortcut) {
            let label = holder.label.clone();
            problems.insert(request.settings_key, ShortcutProblem::TakenBy { label });
            continue;
        }
        accepted.push((shortcut, request));
    }

    ShortcutPlan { accepted, problems }
}

/// The problems of the latest registration, read by Settings.
#[derive(Default)]
pub struct ShortcutProblems(Mutex<BTreeMap<String, ShortcutProblem>>);

impl ShortcutProblems {
    pub fn replace(&self, problems: BTreeMap<String, ShortcutProblem>) {
        *self
            .0
            .lock()
            .expect("shortcut problems lock is never poisoned") = problems;
    }

    pub fn snapshot(&self) -> BTreeMap<String, ShortcutProblem> {
        self.0
            .lock()
            .expect("shortcut problems lock is never poisoned")
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(
        settings_key: &str,
        label: &str,
        combo: Option<&str>,
    ) -> ShortcutRequest<&'static str> {
        ShortcutRequest {
            settings_key: settings_key.to_string(),
            label: label.to_string(),
            combo: combo.map(str::to_string),
            target: "target",
        }
    }

    fn launcher(combo: Option<&str>) -> ShortcutRequest<&'static str> {
        request(LAUNCHER_SETTINGS_KEY, "Global Shortcut", combo)
    }

    fn accepted_keys<T>(plan: &ShortcutPlan<T>) -> Vec<&str> {
        plan.accepted
            .iter()
            .map(|(_, r)| r.settings_key.as_str())
            .collect()
    }

    #[test]
    fn valid_distinct_shortcuts_are_all_accepted_in_order() {
        let plan = plan_shortcuts(vec![
            launcher(Some("CommandOrControl+Shift+Space")),
            request(
                "gadgets.clipboard-manager.shortcut.open-clipboard",
                "Open Clipboard History",
                Some("Shift+Super+V"),
            ),
            request(
                "gadgets.notes.shortcut.new",
                "New Note",
                Some("Control+Alt+N"),
            ),
        ]);

        assert_eq!(
            accepted_keys(&plan),
            vec![
                LAUNCHER_SETTINGS_KEY,
                "gadgets.clipboard-manager.shortcut.open-clipboard",
                "gadgets.notes.shortcut.new",
            ]
        );
        assert!(plan.problems.is_empty());
        assert_eq!(
            plan.accepted[2].0,
            "Control+Alt+N".parse::<Shortcut>().expect("parses")
        );
    }

    #[test]
    fn a_missing_launcher_shortcut_is_skipped_and_reported() {
        let plan = plan_shortcuts(vec![
            launcher(None),
            request(
                "gadgets.notes.shortcut.new",
                "New Note",
                Some("Control+Alt+N"),
            ),
        ]);

        assert_eq!(accepted_keys(&plan), vec!["gadgets.notes.shortcut.new"]);
        assert_eq!(
            plan.problems.get(LAUNCHER_SETTINGS_KEY),
            Some(&ShortcutProblem::Missing)
        );
    }

    #[test]
    fn an_invalid_shortcut_is_skipped_and_the_others_still_register() {
        let plan = plan_shortcuts(vec![
            launcher(Some("Shift+NotAKey")),
            request(
                "gadgets.notes.shortcut.new",
                "New Note",
                Some("Control+Alt+N"),
            ),
            request("gadgets.todo.shortcut.add", "Add Todo", Some("Control++")),
        ]);

        assert_eq!(accepted_keys(&plan), vec!["gadgets.notes.shortcut.new"]);
        assert_eq!(
            plan.problems.get(LAUNCHER_SETTINGS_KEY),
            Some(&ShortcutProblem::Invalid {
                combo: "Shift+NotAKey".to_string()
            })
        );
        assert_eq!(
            plan.problems.get("gadgets.todo.shortcut.add"),
            Some(&ShortcutProblem::Invalid {
                combo: "Control++".to_string()
            })
        );
    }

    #[test]
    fn the_launcher_keeps_a_combo_a_gadget_repeats() {
        let plan = plan_shortcuts(vec![
            launcher(Some("Control+Space")),
            request(
                "gadgets.notes.shortcut.new",
                "New Note",
                Some("Control+Space"),
            ),
        ]);

        assert_eq!(accepted_keys(&plan), vec![LAUNCHER_SETTINGS_KEY]);
        assert_eq!(
            plan.problems.get("gadgets.notes.shortcut.new"),
            Some(&ShortcutProblem::TakenBy {
                label: "Global Shortcut".to_string()
            })
        );
    }

    #[test]
    fn the_first_gadget_keeps_a_combo_a_later_gadget_repeats() {
        let plan = plan_shortcuts(vec![
            launcher(Some("Control+Space")),
            request(
                "gadgets.notes.shortcut.new",
                "New Note",
                Some("Control+Alt+N"),
            ),
            request(
                "gadgets.news.shortcut.open",
                "Open News",
                Some("Control+Alt+N"),
            ),
        ]);

        assert_eq!(
            accepted_keys(&plan),
            vec![LAUNCHER_SETTINGS_KEY, "gadgets.notes.shortcut.new"]
        );
        assert_eq!(
            plan.problems.get("gadgets.news.shortcut.open"),
            Some(&ShortcutProblem::TakenBy {
                label: "New Note".to_string()
            })
        );
    }

    // Duplicates are found on the parsed shortcut, so different
    // spellings of one combo still collide.
    #[test]
    fn different_spellings_of_one_combo_collide() {
        let plan = plan_shortcuts(vec![
            launcher(Some("Control+Alt+K")),
            request(
                "gadgets.notes.shortcut.new",
                "New Note",
                Some("alt+ctrl+KeyK"),
            ),
        ]);

        assert_eq!(accepted_keys(&plan), vec![LAUNCHER_SETTINGS_KEY]);
        assert!(matches!(
            plan.problems.get("gadgets.notes.shortcut.new"),
            Some(ShortcutProblem::TakenBy { .. })
        ));
    }

    // A shortcut that was skipped keeps no combo, so a later request
    // with the same combo takes it.
    #[test]
    fn a_combo_left_by_a_skipped_shortcut_is_free() {
        let plan = plan_shortcuts(vec![
            launcher(None),
            request(
                "gadgets.notes.shortcut.new",
                "New Note",
                Some("Control+Space"),
            ),
        ]);

        assert_eq!(accepted_keys(&plan), vec!["gadgets.notes.shortcut.new"]);
    }

    #[test]
    fn problems_serialize_with_a_kind_tag() {
        let cases = [
            (
                ShortcutProblem::Missing,
                serde_json::json!({ "kind": "missing" }),
            ),
            (
                ShortcutProblem::Invalid {
                    combo: "Shift+NotAKey".into(),
                },
                serde_json::json!({ "kind": "invalid", "combo": "Shift+NotAKey" }),
            ),
            (
                ShortcutProblem::TakenBy {
                    label: "New Note".into(),
                },
                serde_json::json!({ "kind": "takenBy", "label": "New Note" }),
            ),
            (
                ShortcutProblem::Rejected {
                    reason: "HotKey already registered".into(),
                },
                serde_json::json!({ "kind": "rejected", "reason": "HotKey already registered" }),
            ),
        ];
        for (problem, expected) in cases {
            assert_eq!(
                serde_json::to_value(&problem).expect("serializes"),
                expected
            );
        }
    }

    #[test]
    fn the_problem_store_hands_out_the_latest_problems() {
        let store = ShortcutProblems::default();
        assert!(store.snapshot().is_empty());

        store.replace(BTreeMap::from([(
            LAUNCHER_SETTINGS_KEY.to_string(),
            ShortcutProblem::Missing,
        )]));
        assert_eq!(
            store.snapshot().get(LAUNCHER_SETTINGS_KEY),
            Some(&ShortcutProblem::Missing)
        );

        store.replace(BTreeMap::new());
        assert!(store.snapshot().is_empty());
    }
}
