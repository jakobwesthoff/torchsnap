-- Calculator plugin — initial schema.
--
-- Stores evaluated math expressions and their results so the
-- prefix-mode UI can show a history list. Each row carries a
-- ULID primary key, the original expression, the formatted
-- result, a small type tag for frontend styling, the
-- compute timestamp (used for sort order and retention
-- cleanup), and a content hash so deduplication is a single
-- UNIQUE-index lookup instead of a string comparison.

CREATE TABLE calc_history (
    id           TEXT PRIMARY KEY,
    expression   TEXT NOT NULL,
    result       TEXT NOT NULL,
    result_type  TEXT NOT NULL,
    computed_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    content_hash TEXT NOT NULL UNIQUE
);

-- Most queries sort by `computed_at DESC LIMIT N`; the index
-- makes that an O(log N) lookup instead of a full scan.
CREATE INDEX idx_history_computed_at ON calc_history(computed_at DESC);
