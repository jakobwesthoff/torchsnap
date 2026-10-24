---
title: "Run a release build on the Linux testbed with one recipe"
kind: improvement
component: project
impact: low
origin: investigation
tags: [linux, testing, tooling]
---
# Run a release build on the Linux testbed with one recipe

`just testbed-build` deploys a debug build only. The testbed lives on branch `linux-testbed` until it lands.

## Problem

For the Linux memory review of 2026-10-24, release builds had to be built with `just linux-build --release`, copied to the machine by hand with rsync and selected with `TORCHSNAP_TESTBED_APP_BINARY` on the command line. The debug build adds about 37 MiB of file pages to the UI process (binary mapping 56 against 24 MiB, the dev gadgets' compiled code 17 against 13 MiB); everything else measured the same. Memory work on Linux should measure what users run.

## Suggested fix

A `--release` option for `testbed-build`, recorded in the machine's build record so that `testbed-app-start` and `testbed-bench-memory` run the release binary without a variable.
