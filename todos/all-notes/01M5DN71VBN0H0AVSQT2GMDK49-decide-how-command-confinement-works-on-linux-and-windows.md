---
title: "Decide how command confinement works on Linux and Windows"
kind: feature
component: gadget-host
status: needs-discussion
origin: implementation
tags: [security, capabilities, linux, windows]
---
# Decide how command confinement works on Linux and Windows

The confinement of gadget command grants (decided 2026-10-20 in the cwd, env and config-reading todos, implemented on macOS) is built from macOS-specific values. Linux and Windows need their own values, or a decision that the command capability is not offered there until they exist. Whether a port is worth doing at all, and how, is open; this todo records what has to be decided. Code comments at each macOS-specific place cite this todo.

## What is macOS-specific

- The fixed `PATH` for spawned programs: `/usr/bin:/bin:/usr/sbin:/sbin:/usr/local/bin:/opt/homebrew/bin` (Homebrew's prefix, the BSD layout).
- The inherited locale set, including `__CF_USER_TEXT_ENCODING`, which only exists on macOS.
- The hard-rejected env keys: `DYLD_*` is the macOS loader; Linux uses `LD_*` (also rejected), Windows has neither but has its own hijack paths (`PATHEXT`, DLL search order, `COMSPEC`).
- The per-spawn `HOME`, `TMPDIR` and cwd directory: its location, the 0700 permissions and the chmod-then-remove cleanup are POSIX; Windows uses ACLs, `USERPROFILE`, `TEMP` and `TMP` instead of `HOME` and `TMPDIR`.
- The area the gadget may write (the app data directory and its layout) and the path checks: `/` separators, case-insensitive APFS comparison, symlinks; Windows adds drive letters, UNC paths, `\\?\` prefixes, short 8.3 names and junctions.
- Binary resolution: a bare `binary` name is looked up on macOS paths; Windows also resolves extensions through `PATHEXT`.

## Open questions

- Port the confinement per platform, or leave the command capability unavailable on Linux and Windows until a port exists (today neither platform ships a release, ADR 0048).
- If ported: one platform module per OS behind a trait, or per-platform constants in the existing code.
- Which tests can run per platform in CI.

## Relations

- Relates to: [Make the launcher display work on Linux](01KMH12N6QN7N3MTE2ABQP9ZQ9-make-the-launcher-display-work-on-linux.md), the other Linux port work
- Relates to: [Make the launcher display work on Windows](01KMH12N6R0MQ94RWAV32ECZDM-make-the-launcher-display-work-on-windows.md), the other Windows port work
