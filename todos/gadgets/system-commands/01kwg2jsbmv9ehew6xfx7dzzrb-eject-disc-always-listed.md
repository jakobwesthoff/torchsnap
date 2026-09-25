---
kind: improvement
severity: low
status: open
area: [src-tauri/src/gadgets/system_commands/macos_commands/utilities.rs]
---

# Eject Disc is always listed although availability is checkable

## Problem

`EjectDisc::is_available` returns `true` unconditionally
(`utilities.rs:104-106`). Modern Macs have not shipped an optical
drive since 2012ish; for virtually all users this entry is
permanent noise in the result list ("eject", "disk" are common
search fragments) and executing it runs `drutil eject` against
nothing (which also fails silently — see the separate todo on
ignored exit statuses,
`01kwg2ftae87zzd75qgcpa6tv0-system-command-exit-status-ignored.md`).

The `SystemCommand` trait explicitly designed `is_available` for
this: "Whether this command should appear in the launcher right
now. Called on every search keystroke — implementations should be
cheap (no I/O, or cached I/O)"
(`src-tauri/src/gadgets/system_commands/mod.rs:35-38`).

## Suggested fix

Detect optical-drive presence once at construction (e.g.
`drutil status` exit/output, or IOKit lookup) and cache the bool;
return it from `is_available`. Per the trait doc the check must
not run per keystroke.
