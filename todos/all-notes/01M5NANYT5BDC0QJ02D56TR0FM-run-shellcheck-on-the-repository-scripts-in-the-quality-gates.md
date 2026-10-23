---
title: "Run shellcheck on the repository scripts in the quality gates"
kind: chore
component: project
origin: implementation
tags: [tooling, ci]
---
# Run shellcheck on the repository scripts in the quality gates

No recipe runs shellcheck: `just doctor` only checks that it is installed (`just/doctor.just:85`), and `just fullcycle` does not lint shell scripts. Each agent runs shellcheck by hand on the scripts it changes. Found on 2026-10-23; the Fedora howto said shellcheck gated the shell recipes, which was not true (corrected the same day).

## Suggested fix

Add a `lint-shell` recipe that runs `shellcheck -x` on the shell scripts under `tools/` (and the shebang-less ones it can identify), include it in `just lint`, and fix what it reports first. Shell recipes inside `.just` files are not covered by shellcheck; say so where the gate is documented.
