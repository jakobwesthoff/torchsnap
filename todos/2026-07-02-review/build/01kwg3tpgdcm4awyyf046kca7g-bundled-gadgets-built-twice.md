# `just build` builds every bundled gadget twice

**Kind:** improvement
**Severity:** low
**Area:** just/build.just, just/gadgets.just

## Problem

The top-level build recipe (`just/build.just:16-19`) runs:

```
build profile="debug":
    just stage-bundled-gadgets
    just build-gadgets
    bun run tauri build ...
```

`stage-bundled-gadgets` already invokes `just build-gadget "$id"`
for every gadget listed in `gadgets/bundled.toml`
(`just/gadgets.just:186-190`). `build-gadgets` then loops over
*all* gadget directories and calls `just build-gadget` again
(`just/gadgets.just:17-24`) — including the five bundled ones that
were just built.

Per duplicated gadget that repeats: `bun install` + `bun run
build` for the frontend (`gadgets.just:37-40`), a cargo invocation
(incremental, cheap), and a full re-zip of the archive
(`package-gadget`). With five bundled gadgets, `just build` runs
ten gadget build cycles for five distinct gadgets. The comment in
`build.just:8-12` explains why `build-gadgets` runs at all ("so
every gadget under `gadgets/` — bundled or not — has an up-to-date
committed `.torchsnap`"), but not the overlap.

## Impact

Release builds take roughly twice the gadget-build time they need
to; no correctness issue (the second build is idempotent).

## Suggested fix

Invert the order and make staging a pure copy step: run
`build-gadgets` first (builds *everything* once), then have
`stage-bundled-gadgets` only validate the list and `cp` the
already-produced archives instead of calling `build-gadget`
again. That preserves the standalone behavior of
`just stage-bundled-gadgets` less strictly — if standalone
freshness matters, keep the build call there and have
`build-gadgets` accept an exclude list instead.
