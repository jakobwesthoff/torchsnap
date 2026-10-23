---
title: "memsnap sample without a duration fails under the macOS bash"
kind: bug
component: project
impact: low
origin: implementation
tags: [tooling, macos]
---
# memsnap sample without a duration fails under the macOS bash

`tools/memsnap sample` without `-d` expands an empty array under `set -u`, which the macOS system bash (3.2, `/bin/bash`) rejects as an unbound variable. `tools/bench-memory` always passes `-d`, so the benchmark is not affected. Found on 2026-10-23 while adding the Linux path to memsnap.

## Suggested fix

Expand the optional arguments with `${array[@]+"${array[@]}"}` or build the argument list without an empty array, and add a test that runs `memsnap sample` without `-d` under `/bin/bash`.
