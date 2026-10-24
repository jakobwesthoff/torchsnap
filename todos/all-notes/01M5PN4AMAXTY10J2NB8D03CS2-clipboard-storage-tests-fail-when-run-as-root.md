---
title: "Clipboard storage tests fail when run as root"
kind: bug
component: gadgets
impact: low
origin: implementation
tags: [testing, linux]
---
# Clipboard storage tests fail when run as root

Two tests in `src-tauri/src/gadgets/clipboard/storage.rs` fail in the Linux arm64 build container (`tools/linux-build`, branch `linux-testbed`), with and without other changes: `opening_the_state_survives_a_sweep_that_fails` and `a_file_that_cannot_be_deleted_does_not_keep_its_entry`. Found on 2026-10-24 in a full run of the host crate's lib tests there (1530 passed, 6 failed; the other four are the control `info` tests of their own todo). On macOS both pass.

## Problem

Both tests make a file undeletable by setting its shard directory to mode `0o555` (`ReadOnlyDir`, around line 1036 to 1057) and expect the sweep or delete to fail. The cause is a hypothesis, not checked: the container runs the tests as root, and root deletes files in a read-only directory, so the expected failure never happens.

## Impact

The host crate's lib tests cannot pass in the container, so a Linux run there always shows these two failures next to real ones.

## Suggested fix

First confirm the container's user (`id -u` inside `tools/linux-build`). Then either make the failure independent of permissions (for example a path where a directory stands instead of the expected file), skip the two tests when the effective user is root with a message saying why, or run the container's tests as an unprivileged user.
