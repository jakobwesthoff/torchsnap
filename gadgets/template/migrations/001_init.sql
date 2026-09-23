-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at https://mozilla.org/MPL/2.0/.

-- Template gadget SQL storage — initial schema.
--
-- Keep migrations small, additive, and named in
-- numeric order (`001_…`, `002_…`). The host applies
-- them via `rusqlite_migration` during `enable()`, so
-- editing an already-applied migration after a release
-- breaks every existing user. Add a new
-- `002_…` file instead.
--
-- This example tracks how often the gadget has been
-- enabled. Real gadgets use storage for whatever fits
-- their domain — history rows, cached metadata, indices,
-- and so on.

CREATE TABLE enable_log (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    enabled_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
