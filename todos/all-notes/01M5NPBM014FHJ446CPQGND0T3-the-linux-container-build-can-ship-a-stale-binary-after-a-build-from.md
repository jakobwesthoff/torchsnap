---
title: "The Linux container build can ship a stale binary after a build from another checkout"
kind: bug
component: project
impact: high
origin: investigation
tags: [linux, testing, tooling]
---
# The Linux container build can ship a stale binary after a build from another checkout

`just testbed-build` (the container path, branch `linux-testbed`) can build and deploy a binary that lacks a source change, without any error. Measurements on the Linux testbed then run against the wrong code.

## Problem

`tools/linux-build/in-container` copies the checkout into one shared work volume (`torchsnap-linux-build-work`) with `rsync -a`, which keeps each file's mtime, so cargo rebuilds only what changed. Cargo decides that from mtimes: a crate is rebuilt when a source file is newer than its last build output. When a second checkout (another worktree) is built after a first one, a file the second checkout changed before the first checkout's build has an older mtime than the build outputs in the volume, and cargo does not rebuild it.

Seen on 2026-10-24 during the Linux memory review: a worktree whose `src-tauri/src/lib.rs` was edited before another worktree's container build produced a debug binary without that edit. `touch src-tauri/src/lib.rs` and a second build fixed it. An earlier worktree in the same night escaped only because it also changed `src-tauri/Cargo.toml`, which makes cargo rebuild the crate regardless of mtimes.

## Suggested fix

Either of:

- copy into the work volume without keeping mtimes for changed files (rsync `--checksum` without `--times`, so a file whose content differs gets the copy time and unchanged files keep theirs), and pin that with a test that builds from two checkouts in turn;
- a work volume per checkout, as `linux-build` already keeps for lint (`torchsnap-linux-build-lint-<hash>`).

Add a self-test item that changes a file with an mtime older than the last build and checks the deployed binary has the change.
