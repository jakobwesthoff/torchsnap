---
kind: improvement
severity: low
status: open
area: [gadgets/bangs/src/lib.rs]
---

# Bangs first-launch enable() blocks up to 30 s on the network before falling back to bundled data

## Problem

On the first enable after install (empty `bangs` table),
`enable()` calls `import_from_best_source`
(`gadgets/bangs/src/lib.rs:64-75,541-563`), which tries the
network *first*:

```rust
timeout_ms: Some(30_000),
max_body_size: Some(10 * 1024 * 1024),
```

(`try_import_from_network`, `lib.rs:505-530`). Only when the fetch
fails does it import the bundled `assets/bang.json`. On a slow or
captive-portal network the guest sits inside `enable()` for up to
30 seconds; until it returns, the gadget contributes nothing to
searches (`!g ...` queries return `Nothing` because `lookup_bang`
finds an empty table), and host-side lifecycle steps that wait on
enable completion are held up. The per-gadget store lock also
serializes any other host call into this gadget during that
window.

The bundled fallback data is always available and is exactly what
the user gets anyway when the network fails — the network import
only provides *fresher* data, and freshness is already serviceable
on demand via the settings UI's `refresh` message
(`lib.rs:213-230`).

## Suggested fix

Invert the order on first launch: import the bundled
`assets/bang.json` synchronously (fast, local), then refresh from
the network lazily — e.g. through a `[[tasks]]` cron entry, or by
keeping network import exclusively behind the settings-UI
"Refresh" button. If network-first is deliberately kept, lower
the enable-path timeout well below 30 s (the fallback makes a
failed fast attempt cheap). Update the module header comment
(`lib.rs:14-19`) to match whichever order is chosen.
