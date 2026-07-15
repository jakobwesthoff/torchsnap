# App discovery: validate removals before replacing the cache

A background refresh (`src-tauri/src/gadgets/app_launcher.rs:82-123`)
replaces the in-memory app cache wholesale with whatever
`discover()` returns. When Spotlight silently drops entries (see the
2026-07-15 incident in
`01kxk8kxsaed0ah2q0z6eamhg2-app-discovery-fallback-directory-scan.md`),
previously known apps vanish from the launcher even though they are
still installed.

## Strategy (discussed 2026-07-15)

Distinguishing a genuine uninstall from an index dropout is possible
with a cheap filesystem check, because `DiscoveredApp` carries the
bundle path (`src-tauri/src/platform/app_discovery.rs:27-50`):

1. Diff the current cache against the fresh discovery result.
2. For every entry that disappeared, `stat` its bundle path.
   - Path gone → genuine uninstall (or move), drop the entry.
   - Path still exists → index dropout; retain the entry and log a
     warning through the structured logging pipeline
     (`01kxk8kxsbgmzs1sz3sa1mwj3e-app-discovery-diagnostics-to-structured-logging.md`).

An app that was moved or renamed loses its old path and is correctly
dropped; its new location is only found once the index (or a fallback
scan) picks it up.
