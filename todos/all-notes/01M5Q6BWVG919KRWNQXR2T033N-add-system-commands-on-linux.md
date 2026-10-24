---
title: "Add system commands on Linux"
kind: feature
component: gadgets
status: needs-discussion
origin: implementation
tags: [linux]
---
# Add system commands on Linux

The system-commands gadget offers lock, sleep, restart, shut down, log out and further commands on macOS. On Linux it registers with no commands: the non-macOS `system_commands()` factory in `src-tauri/src/gadgets/system_commands/` returns `vec![]`. Its shared helper `run_on_enter` therefore has no caller in the Linux build, which is one of the warnings there. Found on 2026-10-24 while analysing the warnings of the Linux build.

## Proposal

A `linux_commands` module next to `macos_commands`, selected by `system_commands()`, built from the shared `SystemCommand` trait and `run_on_enter`. Candidates, not checked on the testbed: lock (`loginctl lock-session`), suspend, restart and shut down (`systemctl suspend|reboot|poweroff`), log out (desktop-specific, for example `gnome-session-quit --logout`).

## Open questions

- Which commands Linux gets, and in which desktop order (GNOME first).
- Whether the destructive ones need a confirmation first; see the existing todo on confirming destructive system commands.
