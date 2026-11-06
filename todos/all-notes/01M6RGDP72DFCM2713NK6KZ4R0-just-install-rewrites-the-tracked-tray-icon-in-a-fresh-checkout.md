---
title: "just install rewrites the tracked tray icon in a fresh checkout"
kind: bug
component: project
impact: low
origin: implementation
tags: [tooling]
---
# just install rewrites the tracked tray icon in a fresh checkout

Found on 2026-11-06 while working in new git worktrees. `just install`
changes the tracked `src-tauri/icons/tray-icon-template.png` there, so the
worktree is dirty right after setup, and a careless `git add` commits a
regenerated icon.

## Problem

Seen by an agent in a new worktree on 2026-11-06, and read in the code.

`asset-tray-icon` in `just/assets.just` regenerates the icon unless it is
newer than `assets/snappy-tray-template-3-44px.png`. A fresh checkout gives
both files about the same time, so the check fails and the recipe rewrites
the icon. The output differs from the tracked file.

`just release-build` runs `just install` and then refuses a tree with
changed tracked files. The main checkout passes today because its icon is
newer on disk than its source. A release from a fresh clone would stop
there.

## Suggested fix

Decide by content, not by timestamp, or leave the tracked file alone: the
repository's asset rule (ADR 61) tracks outputs of unpinned tools, and
ImageMagick is one, so the recipe may only need to run on request, as the
app icon's does.

## Done when

- `just install` in a fresh checkout leaves `git status` clean.
