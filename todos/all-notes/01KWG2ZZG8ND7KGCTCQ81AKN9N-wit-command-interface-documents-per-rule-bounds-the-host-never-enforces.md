---
title: "WIT command interface documents per-rule bounds the host never enforces"
kind: docs
component: gadget-sdk
status: needs-discussion
---
# WIT command interface documents per-rule bounds the host never enforces

The `command-options` record in the WIT contract, which gadget authors read, promises per-rule limits that the host does not enforce. The docs for `stdin`, `timeout-ms` and `max-output-bytes` each claim a per-rule override the host ignores. Per the host-side todo, the per-rule fields are parsed and then dropped, and stdin has no size limit at all. The docs must match whichever way that host todo is resolved.

## Problem

The `command-options` record documentation in the WIT contract
promises per-rule enforcement
(the `command-options` record in `gadgets/gadget-sdk/wit/torchsnap-gadget.wit`):

- `stdin`: "Bounded by the host `max-stdin-bytes` ceiling and the
  per-rule `max-stdin-bytes` override if declared."
- `timeout-ms`: "Bounded by the host default and the per-rule
  `timeout-ms-max` override if declared."
- `max-output-bytes`: "Bounded by the host default and per-rule
  `max-output-bytes`."

None of this is true today. The confirmed host-side finding, the todo
Per-rule command limits for timeout, output and stdin are parsed but never enforced,
establishes that per-rule `timeout-ms-max` /
`max-output-bytes` / `max-stdin-bytes` are parsed but dropped at
rule compilation, that only global ceilings apply, and that stdin
has *no* size limit at all (there is also no host
`max-stdin-bytes` ceiling, contrary to the WIT text).

The WIT file is the contract gadget authors read (it also feeds
the SDK's rustdoc via wit-bindgen), so the inaccuracy propagates
further than the manifest-parser comments already covered by the
host-core todo.

Verified-accurate claims in the same doc block (no action
needed): the default cwd is an empty directory created per call
(`SpawnDirs` in `src-tauri/src/caps/command.rs`), and manifest-time rule
overlap rejection exists
(`detect_command_rule_overlap` in `src-tauri/src/wasm/manifest/permissions/command.rs`).

## Suggested fix

This todo is the WIT/SDK side of the host-side todo in Relations.
Whichever way that one is resolved (enforce the per-rule fields
or delete them), update the three field docs here in the same
change. If enforcement is chosen, also document the actual global
ceilings (60 s / 64 MiB) in the WIT so the contract is complete
in one place.

## Relations

- Relates to: [Per-rule command limits for timeout, output and stdin are parsed but never enforced](01KWG1AJRVFSMXYJMM7BVCS04B-per-rule-command-limits-for-timeout-output-and-stdin-are-parsed-but.md), the host side. The decision to enforce or delete those fields sets the wording here.
