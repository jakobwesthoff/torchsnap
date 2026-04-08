-- Template plugin SQL storage — initial schema.
--
-- Keep migrations small, additive, and named in
-- numeric order (`001_…`, `002_…`). The host applies
-- them via `rusqlite_migration` on the first
-- `sql::open()` call within an enable lifetime, so
-- editing an already-applied migration after a release
-- breaks every existing user. Add a new
-- `002_…` file instead.
--
-- This example tracks how often the plugin has been
-- enabled. Real plugins use storage for whatever fits
-- their domain — history rows, cached metadata, indices,
-- and so on.

CREATE TABLE enable_log (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    enabled_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
