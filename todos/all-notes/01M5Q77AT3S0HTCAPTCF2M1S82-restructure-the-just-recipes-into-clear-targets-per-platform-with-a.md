---
title: "Restructure the just recipes into clear targets per platform, with a Linux fullcycle"
kind: chore
component: project
status: needs-discussion
horizon: someday
origin: discussion
tags: [tooling, linux]
---
# Restructure the just recipes into clear targets per platform, with a Linux fullcycle

`just fullcycle` checks the macOS build only. The Linux build is checked by separate recipes that run in a Docker container on the Mac (`linux-build`, `linux-build-lint`, `linux-build-check`, `linux-build-run-check`, `linux-build-image`, ...) and by the testbed recipes (`testbed-*`). Six warnings in the Linux build went unnoticed this way until 2026-10-24. Decided that day: the Linux lint stays a separate command for now, and the recipes get reworked later.

## Goal

Cleanly named, understandable targets for each platform Torchsnap builds on, ending in a fullcycle per platform (a Linux fullcycle among them). The platform's checks are expected to run on that system or in a VM of it, so the core recipes run directly there rather than calling out to a Docker container. Convenience wrappers for the Mac (container, testbed) can stay on top as developer comfort.

## Maintainer's note (2026-10-24)

"maybe additionally a todo to somehow restructure and streamline the different commands for different systems, so that we have cleanly named and properly understandable targets for each platform that we want to build on. which would mean we would have a linux fullcycle in the end. and as we are assuming to execute those on the corresponding system or vm most likely they would not call out to a docker container but run within it. syntactic sugar and dx additionals can be there of course, but i think we need to rethink the whole naming and structure here a little in the future"

## Open questions

- Naming scheme for platform targets and how the existing `linux-build-*` and `testbed-*` recipes map onto it.
- Which checks a Linux fullcycle runs, and where (on the Pi, in a VM, in CI).

## Revisit when

The Linux branch lands on main, or a second platform's checks are needed regularly.
