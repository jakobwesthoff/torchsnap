-- ZeroTier gadget storage — initial schema.
--
-- Persists "remembered networks": every network the
-- daemon has reported plus everything imported from the
-- macOS UI's `saved_networks.json`. Joined networks are
-- upserted on every observation (live state win for
-- `last_seen` / `last_status`); known-but-not-currently-
-- joined entries survive `DELETE /network/{id}` calls so
-- the launcher can surface them as "Stored" candidates
-- for re-Connect.

CREATE TABLE networks (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    first_seen    INTEGER NOT NULL,  -- unix ms
    last_seen     INTEGER NOT NULL,  -- unix ms
    last_status   TEXT,              -- last observed daemon status (`OK`, ...)
    last_snapshot TEXT                -- last full Network JSON, nullable
);

CREATE INDEX idx_networks_last_seen ON networks(last_seen DESC);
