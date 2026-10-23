---
title: "Build each bundled gadget once in just build"
kind: chore
component: project
impact: low
---
# Build each bundled gadget once in just build

`just build` builds every bundled gadget twice: once through `stage-bundled-gadgets` and again through `build-gadgets`. Release builds take roughly twice the gadget-build time they need, and the fix belongs in the Justfile's recipe order.

## Problem

The top-level build recipe (the `build` recipe in `just/build.just`) runs:

```
build profile="debug" sign="" config="":
    just stage-bundled-gadgets
    just build-gadgets
    bun run tauri build ...
```

`stage-bundled-gadgets` already invokes `just build-gadget "$id"` for every gadget listed in `gadgets/bundled.toml` (`stage-bundled-gadgets` in `just/gadgets.just`). `build-gadgets` then loops over *all* gadget directories and calls `just build-gadget` again (`build-gadgets` in `just/gadgets.just`), including the five bundled ones that were just built.

Per duplicated gadget that repeats: `bun install` + `bun run build` for the frontend (`build-gadget` in `just/gadgets.just`), a cargo invocation (incremental, cheap), and a full re-zip of the archive (`package-gadget`). With five bundled gadgets, `just build` runs ten gadget build cycles for five distinct gadgets. The comment at the top of `just/build.just` explains why `build-gadgets` runs at all ("so every gadget under `gadgets/`, bundled or not, has an up-to-date committed `.torchsnap`"), but not the overlap.

## Impact

Release builds take roughly twice the gadget-build time they need to; no correctness issue (the second build is idempotent).

## Suggested fix

Invert the order and make staging a pure copy step: run `build-gadgets` first (builds *everything* once), then have `stage-bundled-gadgets` only validate the list and hand the already-produced archives to `tools/stage-bundled-archives` instead of calling `build-gadget` again. That preserves the standalone behavior of `just stage-bundled-gadgets` less strictly. If standalone freshness matters, keep the build call there and have `build-gadgets` accept an exclude list instead.
