---
kind: improvement
severity: medium
status: open
area: [gadgets/gadget-sdk/wit/torchsnap-gadget.wit]
tags: [docs]
---

# WIT command interface documents per-rule bounds the host never enforces

## Problem

The `command-options` record documentation in the WIT contract
promises per-rule enforcement
(`gadgets/gadget-sdk/wit/torchsnap-gadget.wit:523-532`):

- `stdin`: "Bounded by the host `max-stdin-bytes` ceiling and the
  per-rule `max-stdin-bytes` override if declared."
- `timeout-ms`: "Bounded by the host default and the per-rule
  `timeout-ms-max` override if declared."
- `max-output-bytes`: "Bounded by the host default and per-rule
  `max-output-bytes`."

None of this is true today. The confirmed host-side finding
`todos/gadget-host/caps/01kwg1ajrvfsmxyjmm7bvcs04b-command-per-rule-limits-unenforced.md`
establishes that per-rule `cwd` / `timeout-ms-max` /
`max-output-bytes` / `max-stdin-bytes` are parsed but dropped at
rule compilation, that only global ceilings apply, and that stdin
has *no* size limit at all (there is also no host
`max-stdin-bytes` ceiling, contrary to the WIT text).

The WIT file is the contract gadget authors read (it also feeds
the SDK's rustdoc via wit-bindgen), so the inaccuracy propagates
further than the manifest-parser comments already covered by the
host-core todo.

Verified-accurate claims in the same doc block (no action
needed): the `<gadget-data>/exec-cwd/` default cwd exists
(`src-tauri/src/caps/command.rs:180`), and manifest-time rule
overlap rejection exists
(`src-tauri/src/wasm/manifest/permissions/command.rs:198-227`).

## Suggested fix

This todo is the WIT/SDK side of the host-core todo above —
whichever way that one is resolved (enforce the per-rule fields
or delete them), update the three field docs here in the same
change. If enforcement is chosen, also document the actual global
ceilings (60 s / 64 MiB) in the WIT so the contract is complete
in one place.
