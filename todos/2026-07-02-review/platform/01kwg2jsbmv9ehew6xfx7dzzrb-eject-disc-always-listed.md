# Eject Disc is always listed although availability is checkable

**Kind:** improvement
**Severity:** low
**Area:** src-tauri/src/gadgets/system_commands/macos_commands/utilities.rs

## Problem

`EjectDisc::is_available` returns `true` unconditionally
(`utilities.rs:109-111`). Modern Macs have not shipped an optical
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
(`src-tauri/src/gadgets/system_commands/mod.rs:34-37`).

## Suggested fix

Detect optical-drive presence once at construction (e.g.
`drutil status` exit/output, or IOKit lookup) and cache the bool;
return it from `is_available`. Per the trait doc the check must
not run per keystroke.
