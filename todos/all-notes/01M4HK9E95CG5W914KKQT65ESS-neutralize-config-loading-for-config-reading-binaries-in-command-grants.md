---
title: "Neutralize config loading for config-reading binaries in command grants"
kind: bug
component: gadget-host
impact: high
tags: [security, capabilities]
---
# Neutralize config loading for config-reading binaries in command grants

A gadget with a command grant for a config-reading binary such as `git`, `make`, `npm` or `eslint` can write a malicious config inside its permitted root, including the `exec-cwd` scratch, and the binary then loads it. Confining the cwd does not prevent that, so these grants stay a path to running native code as the user even after the cwd fix. Decided on 2026-10-20: structural rules close it without a list of binaries (see Decisions).

## Problem

Binaries of this family read config and plugins from the working directory and its ancestors, from `HOME`, from paths named in env vars or argv, and git also from its system and global config. A config of that kind can run arbitrary commands. Established by reading the command capability for the cwd todo, not reproduced.

## Impact

Any grant for a config-reading binary lets the gadget run arbitrary native code as the user, whatever the cwd policy.

## Decisions

Decided by Jakob on 2026-10-20:

- **No list of config-reading binaries.** Such a list is always incomplete and gives false confidence. Neither neutralizing per binary family nor refusing a list of binaries is done.
- **Structural rule: nothing the gadget can write is visible to a spawned program as configuration.** The attack needs the gadget to place a file where the program looks; the rules remove every such place:
  - The manifest parser rejects a rule `cwd` root that lies in, or has an ancestor in, an area the gadget may write (filesystem write grants, its own data directory). The same holds for `path-under` argv roots, so `make -f <gadget file>` cannot be granted.
  - Without a rule `cwd`, and for `HOME` and `TMPDIR`, the host creates a fresh empty directory per spawn that the gadget cannot reach, and removes it afterwards. This replaces today's `${gadget-data}/exec-cwd/`, which the gadget can write.
  - Declared env values must not point into areas the gadget may write (the env todo carries the details).
  - The install review describes every command grant generically, as running a native program with the user's rights, and marks unconstrained argv. It names no binaries.
- **Accepted residual:** a config the user put into their own directory (their repo's `.git/config`) is still read; it gives the gadget nothing it did not have. A grant whose argv lets the program write files itself (`git config ...`, `tee`) can plant configuration in one run and use it in the next; the argv rules and the review's display of the grant are what bound that.

## Relations

- Relates to: [Guest-supplied command cwd is not confined and per-rule cwd is never enforced](01KWH4J9BPTRAYF451YZD2145E-guest-supplied-command-cwd-is-not-confined-and-per-rule-cwd-is-never.md), whose cwd confinement leaves this case open
- Relates to: [Guest env overrides bypass the credential filter, and the env denylist leaks credentials](01KWH4J9BPTRAYF451YZD2145F-guest-env-overrides-bypass-the-credential-filter-and-the-env-denylist.md), whose declared env values must not point into gadget-writable areas
