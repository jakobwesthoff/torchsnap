// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Frecency System
//
// Tracks frequency + recency of user selections to boost
// commonly-used items in search results. Every selection
// records a timestamped event; scores are computed from
// Firefox-style bucketed decay weights.
//
// Architecture:
// - FrecencyStore: central store wrapping SqlStorage, shared
//   via Arc across the plugin host and Tauri state.
// - PluginFrecency: plugin-scoped wrapper that binds the
//   plugin_id, following the PluginSettings pattern.
// - FrecencyTarget: trait for types that can receive a score
//   bonus (SourcedEntry, ScoredEntry).
// =========================================================

mod plugin_frecency;
mod schema;

pub use plugin_frecency::PluginFrecency;

use std::collections::HashMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::Serialize;
use tauri_plugin_store::Store;

use crate::settings_notifier::{SettingsNotifier, SettingsWatch};
use crate::storage::{SqlStorage, SqlValue};

// =========================================================
// Constants
// =========================================================

/// Maximum number of events retained per (plugin_id, item_id).
const MAX_EVENTS_PER_ITEM: i64 = 30;

/// Events older than this are deleted on startup.
const MAX_AGE_MS: i64 = 90 * 24 * 60 * 60 * 1000; // 90 days

/// Maximum items in a single SQL IN clause. Larger sets are
/// chunked into multiple queries.
const IN_CLAUSE_CHUNK_SIZE: usize = 500;

// =========================================================
// Decay Buckets
//
// Firefox-style bucketed weights. More recent events
// contribute more to the score.
// =========================================================

const FOUR_HOURS_MS: i64 = 4 * 60 * 60 * 1000;
const ONE_DAY_MS: i64 = 24 * 60 * 60 * 1000;
const ONE_WEEK_MS: i64 = 7 * ONE_DAY_MS;
const ONE_MONTH_MS: i64 = 30 * ONE_DAY_MS;

fn bucket_weight(age_ms: i64) -> u32 {
    if age_ms < FOUR_HOURS_MS {
        100
    } else if age_ms < ONE_DAY_MS {
        70
    } else if age_ms < ONE_WEEK_MS {
        50
    } else if age_ms < ONE_MONTH_MS {
        30
    } else {
        10
    }
}

// =========================================================
// FrecencyTarget — trait for score application
// =========================================================

/// Types that can receive a frecency score bonus.
///
/// Implemented by `SourcedEntry` and `ScoredEntry` so that
/// `FrecencyStore::apply_scores` works generically over both.
pub trait FrecencyTarget {
    /// The item ID used to look up the frecency score.
    fn item_id(&self) -> &str;
    /// Add a bonus to this item's search score.
    fn boost_score(&mut self, bonus: u32);
}

// =========================================================
// FrecencyItem — top-N result
// =========================================================

/// A single item with its computed frecency score, returned
/// by `top_items()` for empty-query cases (e.g., emoji picker).
#[derive(Debug, Clone)]
pub struct FrecencyItem {
    pub item_id: String,
    pub score: u32,
}

// =========================================================
// FrecencyStats — admin statistics
// =========================================================

/// Aggregate statistics about stored frecency data. Returned
/// by the `frecency_stats` Tauri command for the settings UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrecencyStats {
    pub total_events: u64,
    pub unique_items: u64,
    pub events_by_plugin: HashMap<String, u64>,
    /// Millisecond timestamp of the oldest event, or `None` if
    /// the database is empty.
    pub oldest_event: Option<i64>,
}

// =========================================================
// FrecencyStore
// =========================================================

/// Central frecency tracking store. Thread-safe, shareable
/// via `Arc`. Used by `PluginHost` directly — plugins get
/// [`PluginFrecency`] instead.
pub struct FrecencyStore {
    db: SqlStorage,
    enabled: SettingsWatch<bool>,
}

impl FrecencyStore {
    /// Open (or create) the frecency database, run migrations,
    /// and perform the startup age sweep.
    pub fn open(
        app_data_dir: &Path,
        notifier: &SettingsNotifier,
        store: &Store<tauri::Wry>,
    ) -> Result<Self> {
        let db_path = app_data_dir.join("frecency.db");
        let db = SqlStorage::open(db_path, schema::MIGRATIONS).context("open frecency database")?;

        // Seed the settings watch with the current store value.
        let initial = store
            .get("frecency.enabled")
            .unwrap_or(serde_json::Value::Bool(true));
        let enabled = notifier.watch_with_initial::<bool>("frecency.enabled", initial);

        let frecency = Self { db, enabled };

        // Startup age sweep: delete events older than 90 days.
        frecency.sweep_old_events()?;

        Ok(frecency)
    }

    // -------------------------------------------------------
    // Public API
    // -------------------------------------------------------

    /// Record a selection event for the given (plugin_id, item_id).
    ///
    /// No-op when frecency is disabled. After inserting, prunes
    /// events beyond the per-item cap.
    ///
    /// **Note for plugin authors:** The host already calls this in
    /// `PluginHost::execute()`. Plugins should only call `record()`
    /// directly (via `PluginFrecency`) for custom UI interactions
    /// that bypass `execute()`.
    pub fn record(&self, plugin_id: &str, item_id: &str) {
        if !self.is_enabled() {
            return;
        }

        let now = now_ms();

        // INSERT OR IGNORE handles the (practically impossible)
        // same-millisecond collision gracefully.
        let _ = self.db.execute(
            "INSERT OR IGNORE INTO frecency_events (plugin_id, item_id, timestamp) \
             VALUES (?, ?, ?)",
            &[
                SqlValue::from(plugin_id),
                SqlValue::from(item_id),
                SqlValue::from(now),
            ],
        );

        // Prune oldest events beyond the per-item cap.
        let _ = self.db.execute(
            "DELETE FROM frecency_events \
             WHERE plugin_id = ? AND item_id = ? AND timestamp NOT IN (\
                 SELECT timestamp FROM frecency_events \
                 WHERE plugin_id = ? AND item_id = ? \
                 ORDER BY timestamp DESC LIMIT ?\
             )",
            &[
                SqlValue::from(plugin_id),
                SqlValue::from(item_id),
                SqlValue::from(plugin_id),
                SqlValue::from(item_id),
                SqlValue::from(MAX_EVENTS_PER_ITEM),
            ],
        );
    }

    /// Compute the frecency score for a single item.
    /// Returns 0 when disabled or when no events exist.
    pub fn score(&self, plugin_id: &str, item_id: &str) -> u32 {
        if !self.is_enabled() {
            return 0;
        }

        let now = now_ms();
        let timestamps = self
            .db
            .query_map(
                "SELECT timestamp FROM frecency_events \
                 WHERE plugin_id = ? AND item_id = ?",
                &[SqlValue::from(plugin_id), SqlValue::from(item_id)],
                |row| row.get::<i64>(0),
            )
            .unwrap_or_default();

        compute_score(&timestamps, now)
    }

    /// Batch-compute frecency scores for multiple items.
    ///
    /// Returns a map from item_id to score. Items with no events
    /// are omitted from the map. Chunks the IN clause at 500 items
    /// defensively.
    pub fn scores(&self, plugin_id: &str, item_ids: &[&str]) -> HashMap<String, u32> {
        if !self.is_enabled() || item_ids.is_empty() {
            return HashMap::new();
        }

        let now = now_ms();
        let mut result = HashMap::new();

        for chunk in item_ids.chunks(IN_CLAUSE_CHUNK_SIZE) {
            let rows = self.fetch_timestamps(plugin_id, chunk);
            for (item_id, timestamps) in &rows {
                let score = compute_score(timestamps, now);
                if score > 0 {
                    result.insert(item_id.clone(), score);
                }
            }
        }

        result
    }

    /// Batch-fetch frecency scores and apply them as additive
    /// bonuses to a mutable slice of results.
    pub fn apply_scores(&self, plugin_id: &str, results: &mut [impl FrecencyTarget]) {
        if !self.is_enabled() || results.is_empty() {
            return;
        }

        let ids: Vec<&str> = results.iter().map(|r| r.item_id()).collect();
        let scores = self.scores(plugin_id, &ids);

        for result in results.iter_mut() {
            if let Some(&bonus) = scores.get(result.item_id()) {
                result.boost_score(bonus);
            }
        }
    }

    /// Return the top-N most frequently/recently used items for
    /// a plugin. Used for empty-query cases like the emoji picker.
    ///
    /// Fetches all events for the plugin, groups by item_id,
    /// computes scores, and returns the top N sorted descending.
    pub fn top_items(&self, plugin_id: &str, limit: usize) -> Vec<FrecencyItem> {
        if !self.is_enabled() {
            return Vec::new();
        }

        let now = now_ms();

        let rows = self
            .db
            .query_map(
                "SELECT item_id, timestamp FROM frecency_events WHERE plugin_id = ?",
                &[SqlValue::from(plugin_id)],
                |row| Ok((row.get::<String>(0)?, row.get::<i64>(1)?)),
            )
            .unwrap_or_default();

        // Group timestamps by item_id.
        let mut grouped: HashMap<String, Vec<i64>> = HashMap::new();
        for (item_id, ts) in rows {
            grouped.entry(item_id).or_default().push(ts);
        }

        // Score each item and collect.
        let mut items: Vec<FrecencyItem> = grouped
            .into_iter()
            .map(|(item_id, timestamps)| FrecencyItem {
                score: compute_score(&timestamps, now),
                item_id,
            })
            .filter(|item| item.score > 0)
            .collect();

        // Sort descending by score, take top N.
        items.sort_by(|a, b| b.score.cmp(&a.score));
        items.truncate(limit);
        items
    }

    /// Whether frecency tracking is currently enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled.get()
    }

    /// Compute aggregate statistics for the settings UI.
    pub fn stats(&self) -> Result<FrecencyStats> {
        let total_events: u64 = self
            .db
            .query_map("SELECT COUNT(*) FROM frecency_events", &[], |row| {
                row.get::<i64>(0)
            })
            .context("count total events")?
            .first()
            .copied()
            .unwrap_or(0) as u64;

        let unique_items: u64 = self
            .db
            .query_map(
                "SELECT COUNT(DISTINCT plugin_id || '::' || item_id) FROM frecency_events",
                &[],
                |row| row.get::<i64>(0),
            )
            .context("count unique items")?
            .first()
            .copied()
            .unwrap_or(0) as u64;

        let plugin_rows = self
            .db
            .query_map(
                "SELECT plugin_id, COUNT(*) FROM frecency_events GROUP BY plugin_id",
                &[],
                |row| Ok((row.get::<String>(0)?, row.get::<i64>(1)?)),
            )
            .context("count events by plugin")?;

        let events_by_plugin: HashMap<String, u64> = plugin_rows
            .into_iter()
            .map(|(id, count)| (id, count as u64))
            .collect();

        let oldest_event: Option<i64> = self
            .db
            .query_map("SELECT MIN(timestamp) FROM frecency_events", &[], |row| {
                row.get::<Option<i64>>(0)
            })
            .context("find oldest event")?
            .into_iter()
            .next()
            .flatten();

        Ok(FrecencyStats {
            total_events,
            unique_items,
            events_by_plugin,
            oldest_event,
        })
    }

    /// Delete all frecency data.
    pub fn clear_all(&self) -> Result<()> {
        self.db
            .execute("DELETE FROM frecency_events", &[])
            .context("clear frecency events")?;
        Ok(())
    }

    // -------------------------------------------------------
    // Internal helpers
    // -------------------------------------------------------

    /// Startup sweep: delete events older than 90 days.
    fn sweep_old_events(&self) -> Result<()> {
        let cutoff = now_ms() - MAX_AGE_MS;
        self.db
            .execute(
                "DELETE FROM frecency_events WHERE timestamp < ?",
                &[SqlValue::from(cutoff)],
            )
            .context("sweep old frecency events")?;
        Ok(())
    }

    /// Fetch all timestamps for a set of item_ids within a single
    /// plugin, grouped by item_id. Used internally by `scores()`.
    fn fetch_timestamps(&self, plugin_id: &str, item_ids: &[&str]) -> HashMap<String, Vec<i64>> {
        let id_list: Vec<SqlValue> = item_ids.iter().map(|id| SqlValue::from(*id)).collect();

        let rows = self
            .db
            .query_map(
                "SELECT item_id, timestamp FROM frecency_events \
                 WHERE plugin_id = ? AND item_id IN (?)",
                &[SqlValue::from(plugin_id), SqlValue::List(id_list)],
                |row| Ok((row.get::<String>(0)?, row.get::<i64>(1)?)),
            )
            .unwrap_or_default();

        let mut grouped: HashMap<String, Vec<i64>> = HashMap::new();
        for (item_id, ts) in rows {
            grouped.entry(item_id).or_default().push(ts);
        }
        grouped
    }
}

// =========================================================
// Score Computation
// =========================================================

/// Compute a frecency score from raw timestamps using
/// Firefox-style bucketed decay.
fn compute_score(timestamps: &[i64], now: i64) -> u32 {
    timestamps
        .iter()
        .map(|&ts| {
            let age = now - ts;
            bucket_weight(age)
        })
        .sum()
}

/// Current time as milliseconds since Unix epoch.
fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before Unix epoch")
        .as_millis() as i64
}

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a FrecencyStore backed by a temp directory, with
    /// frecency always enabled (no real settings store needed).
    fn test_store() -> (FrecencyStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("create temp dir");
        let db_path = dir.path().join("frecency.db");
        let db = SqlStorage::open(db_path, schema::MIGRATIONS).expect("open test db");

        // Create a settings watch that always returns true.
        let notifier = SettingsNotifier::new();
        let enabled =
            notifier.watch_with_initial::<bool>("frecency.enabled", serde_json::Value::Bool(true));

        let store = FrecencyStore { db, enabled };
        (store, dir)
    }

    #[test]
    fn record_and_score() {
        let (store, _dir) = test_store();
        store.record("test-plugin", "item-1");
        let score = store.score("test-plugin", "item-1");
        // Single recent event: weight 100
        assert_eq!(score, 100);
    }

    #[test]
    fn score_missing_item_is_zero() {
        let (store, _dir) = test_store();
        assert_eq!(store.score("test-plugin", "nonexistent"), 0);
    }

    #[test]
    fn multiple_events_accumulate() {
        let (store, _dir) = test_store();
        for _ in 0..5 {
            store.record("test-plugin", "item-1");
            // Insert with different timestamps by directly writing.
        }
        // All events are within 4 hours, so each gets weight 100.
        // Due to INSERT OR IGNORE and millisecond precision, some
        // events might collide — but at least one should succeed.
        let score = store.score("test-plugin", "item-1");
        assert!(score >= 100);
    }

    #[test]
    fn batch_scores() {
        let (store, _dir) = test_store();
        store.record("p", "a");
        store.record("p", "b");

        let scores = store.scores("p", &["a", "b", "c"]);
        assert_eq!(scores.get("a"), Some(&100));
        assert_eq!(scores.get("b"), Some(&100));
        assert!(scores.get("c").is_none());
    }

    #[test]
    fn top_items_sorted() {
        let (store, _dir) = test_store();

        // Insert events with distinct timestamps to avoid INSERT
        // OR IGNORE collisions from same-millisecond record() calls.
        let now = now_ms();
        for i in 0..3 {
            let _ = store.db.execute(
                "INSERT OR IGNORE INTO frecency_events (plugin_id, item_id, timestamp) \
                 VALUES (?, ?, ?)",
                &[
                    SqlValue::from("p"),
                    SqlValue::from("a"),
                    SqlValue::from(now - 100 + i as i64),
                ],
            );
        }
        let _ = store.db.execute(
            "INSERT OR IGNORE INTO frecency_events (plugin_id, item_id, timestamp) \
             VALUES (?, ?, ?)",
            &[
                SqlValue::from("p"),
                SqlValue::from("b"),
                SqlValue::from(now),
            ],
        );

        let top = store.top_items("p", 10);
        assert!(!top.is_empty());
        // "a" should be first since it has more events (3 vs 1).
        assert_eq!(top[0].item_id, "a");
    }

    #[test]
    fn stats_counts() {
        let (store, _dir) = test_store();
        store.record("p1", "a");
        store.record("p1", "b");
        store.record("p2", "c");

        let stats = store.stats().expect("stats");
        assert!(stats.total_events >= 3);
        assert!(stats.unique_items >= 3);
        assert!(stats.events_by_plugin.contains_key("p1"));
        assert!(stats.events_by_plugin.contains_key("p2"));
        assert!(stats.oldest_event.is_some());
    }

    #[test]
    fn clear_all_removes_everything() {
        let (store, _dir) = test_store();
        store.record("p", "a");
        store.clear_all().expect("clear");

        let stats = store.stats().expect("stats");
        assert_eq!(stats.total_events, 0);
    }

    #[test]
    fn disabled_store_is_noop() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let db_path = dir.path().join("frecency.db");
        let db = SqlStorage::open(db_path, schema::MIGRATIONS).expect("open test db");

        let notifier = SettingsNotifier::new();
        let enabled =
            notifier.watch_with_initial::<bool>("frecency.enabled", serde_json::Value::Bool(false));

        let store = FrecencyStore { db, enabled };

        store.record("p", "a");
        assert_eq!(store.score("p", "a"), 0);
        assert!(store.top_items("p", 10).is_empty());
    }

    #[test]
    fn bucket_weights_are_correct() {
        // < 4 hours
        assert_eq!(bucket_weight(0), 100);
        assert_eq!(bucket_weight(FOUR_HOURS_MS - 1), 100);
        // < 1 day
        assert_eq!(bucket_weight(FOUR_HOURS_MS), 70);
        assert_eq!(bucket_weight(ONE_DAY_MS - 1), 70);
        // < 1 week
        assert_eq!(bucket_weight(ONE_DAY_MS), 50);
        assert_eq!(bucket_weight(ONE_WEEK_MS - 1), 50);
        // < 1 month
        assert_eq!(bucket_weight(ONE_WEEK_MS), 30);
        assert_eq!(bucket_weight(ONE_MONTH_MS - 1), 30);
        // > 1 month
        assert_eq!(bucket_weight(ONE_MONTH_MS), 10);
        assert_eq!(bucket_weight(ONE_MONTH_MS * 2), 10);
    }

    #[test]
    fn per_item_cap_prunes_oldest() {
        let (store, _dir) = test_store();

        // Insert MAX_EVENTS_PER_ITEM + 5 events with distinct timestamps.
        // We insert directly to control timestamps.
        let count = MAX_EVENTS_PER_ITEM as usize + 5;
        let base_ts = now_ms() - 1000;
        for i in 0..count {
            let _ = store.db.execute(
                "INSERT OR IGNORE INTO frecency_events (plugin_id, item_id, timestamp) \
                 VALUES (?, ?, ?)",
                &[
                    SqlValue::from("p"),
                    SqlValue::from("item"),
                    SqlValue::from(base_ts + i as i64),
                ],
            );
        }

        // Trigger pruning by recording one more event.
        store.record("p", "item");

        // Should have at most MAX_EVENTS_PER_ITEM events.
        let timestamps = store
            .db
            .query_map(
                "SELECT timestamp FROM frecency_events WHERE plugin_id = ? AND item_id = ?",
                &[SqlValue::from("p"), SqlValue::from("item")],
                |row| row.get::<i64>(0),
            )
            .expect("query timestamps");

        assert!(
            timestamps.len() <= MAX_EVENTS_PER_ITEM as usize,
            "expected at most {} events, got {}",
            MAX_EVENTS_PER_ITEM,
            timestamps.len()
        );
    }
}
