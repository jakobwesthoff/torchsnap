---
title: "just install rewrites the tracked tray icon in a fresh checkout"
kind: bug
component: project
origin: implementation
tags: [tooling]
---
# just install rewrites the tracked tray icon in a fresh checkout

In a fresh checkout or worktree, `just install` regenerates the tracked `src-tauri/icons/tray-icon-template.png`, and the local ImageMagick writes different bytes of the same size (2001 B), so `git status` shows the file modified. Found on 2026-10-23 by several agents working in new worktrees; each restored the file with `git checkout --`.

## Problem

`asset-tray-icon` in `just/assets.just` (recipe at line 129) skips the work only when the output is newer than its source:

```bash
if [ -f "$tray" ] && [ "$tray" -nt "$src" ]; then
```

A fresh checkout writes both `src-tauri/icons/tray-icon-template.png` and `assets/snappy-tray-template-3-44px.png` with about the same mtime, so `-nt` is false and the recipe regenerates the tracked file with `magick` and `tools/png-to-alpha-mask`. That the byte difference comes from the ImageMagick version is not verified.

The same `-nt` pattern appears again at `just/assets.just:217`.

## Impact

Every new worktree starts dirty, which trips agents' "status must be clean" checks and invites committing a regenerated icon by accident.

## Suggested fix

Decide by content, not by timestamps, for tracked outputs: skip when the tracked file exists and is committed unchanged, or compare a hash of the source recorded next to the output. ADR 61 says outputs of unpinned tools (ImageMagick) are tracked with their sources; regenerating them belongs to an explicit recipe, not to `just install`.
