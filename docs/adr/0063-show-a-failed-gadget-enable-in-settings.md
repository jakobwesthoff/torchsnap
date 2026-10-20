# 63. Show a failed gadget enable in Settings

Date: 2026-10-20

## Status

Accepted

Amends [25. Host-managed plugin enable/disable lifecycle](0025-host-managed-plugin-enable-disable-lifecycle.md) (the stored setting and the runtime flag can differ)

Amended by [64. Retry a failed gadget enable from Settings](0064-retry-a-failed-gadget-enable-from-settings.md)

## Context

When a gadget's `enable()` returned `Err`, the host set the gadget's
runtime flag to `false`, so search, shortcuts and messages skipped it,
and dropped the error. The stored `enabled.<id>` setting stayed `true`,
and Settings read only that setting, so the gadget showed as enabled
while it did nothing. A gadget that failed at startup also kept its
global shortcut, since shortcuts are registered before the gadgets
start.

Writing `false` to the stored setting on a failure would switch the
gadget off across restarts, even when the cause was passing.

## Decision

The stored `enabled.<id>` setting is the user's choice. A failed
`enable()` does not change it.

When `enable()` fails, the host deactivates the gadget, logs the error
and keeps its text (`{e:#}`) per gadget id. A gadget that failed at
startup has its shortcuts registered again without it. The host serves
the kept errors through the `gadget_enable_failures` command and emits
`gadget-enable-failures-changed` whenever one is kept or removed.
Settings show the error under the gadget's switch while the switch is
on.

Turning the gadget off removes its error. Turning it on again calls
`enable()` again; the earlier error is removed when that call starts. A
failure is kept only while the gadget is still meant to run: when the
user turns it off while `enable()` is running, no error is kept.

## Consequences

ADR 25's statement that no inconsistency is possible between the stored
setting and the runtime flag no longer holds: a gadget that failed to
start has the setting on and the flag off, and Settings show that state.
