-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at https://mozilla.org/MPL/2.0/.

-- Initial schema: bang lookup table and key-value metadata store.
--
-- `trigger` uses `COLLATE NOCASE` so lookups are case-insensitive
-- without needing `LOWER()` in every query — matching DuckDuckGo's
-- own behaviour.

CREATE TABLE bangs (
    trigger TEXT PRIMARY KEY COLLATE NOCASE,
    service_name TEXT NOT NULL,
    url_template TEXT NOT NULL,
    domain TEXT NOT NULL,
    category TEXT,
    subcategory TEXT,
    rank INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE metadata (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
