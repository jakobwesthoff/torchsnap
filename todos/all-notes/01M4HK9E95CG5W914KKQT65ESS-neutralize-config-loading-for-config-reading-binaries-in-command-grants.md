---
title: "Neutralize config loading for config-reading binaries in command grants"
kind: bug
component: gadget-host
status: needs-discussion
impact: high
tags: [security, capabilities]
---
# Neutralize config loading for config-reading binaries in command grants

A gadget with a command grant for a config-reading binary such as `git`, `make`, `npm` or `eslint` can write a malicious config inside its permitted root, including the `exec-cwd` scratch, and the binary then loads it. Confining the cwd does not prevent that, so these grants stay a path to running native code as the user even after the cwd fix. The cwd todo names this as a required follow-up.

## Problem

Binaries of this family read config and plugins from the working directory, and git also from its system and global config. A config of that kind can run arbitrary commands. Established by reading the command capability for the cwd todo, not reproduced.

## Impact

Any grant for a config-reading binary lets the gadget run arbitrary native code as the user, whatever the cwd policy.

## Suggested fix

Two directions, to be decided:

- Neutralize config loading per binary when the host spawns it. For git: `GIT_CONFIG_NOSYSTEM=1`, `GIT_CONFIG_GLOBAL=/dev/null` and an empty `HOME`. Each binary family needs its own set.
- Refuse grants for config-reading binaries.

## Relations

- Relates to: [Guest-supplied command cwd is not confined and per-rule cwd is never enforced](01KWH4J9BPTRAYF451YZD2145E-guest-supplied-command-cwd-is-not-confined-and-per-rule-cwd-is-never.md), whose cwd confinement leaves this case open
