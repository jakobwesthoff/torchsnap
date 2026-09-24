---
kind: improvement
severity: low
status: open
area: [src-tauri/src/wasm/interface_gate.rs]
---

# Interface gate lists are hand-synced with the WIT file

## Problem
The gate's knowledge of interfaces exists in four hand-maintained
copies that must stay in sync with
`gadgets/gadget-sdk/wit/torchsnap-gadget.wit`:

1. `UNGATED` (`src-tauri/src/wasm/interface_gate.rs:37`)
2. the `is_provisioned` match arms (`:110-124`)
3. the hardcoded gated-name lists repeated in three tests
   (`:263-277`, `:290-303`, `:322-333`)
4. the WIT interface names themselves

The failure mode is benign but confusing: adding a new gated WIT
interface plus its manifest permission, and forgetting the
`is_provisioned` arm, makes the gate reject the gadget with
"imports interfaces without matching permissions" even though the
manifest declares the permission and provisioning succeeded. The
fallback `_ => false` is the right fail-safe default, but nothing
tells the developer the gate itself is the stale piece.

## Impact
Each new capability requires touching this file in two places
plus its tests; a miss surfaces as a misleading load error at
runtime rather than a compile-time or test failure.

## Suggested fix
Options, smallest first:
- Add a test that parses the WIT file (it lives in-repo) and
  asserts every `torchsnap:gadget/*` interface is either in
  `UNGATED` or has an `is_provisioned` arm, failing with a
  message naming the stale list.
- Alternatively derive both lists from a single
  `&[(&str, fn(&ProvisionedCaps) -> bool)]` table so there is
  one place to extend.
