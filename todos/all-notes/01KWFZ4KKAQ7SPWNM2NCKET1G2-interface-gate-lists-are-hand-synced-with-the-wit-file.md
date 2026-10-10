---
title: "Interface gate lists are hand-synced with the WIT file"
kind: improvement
component: gadget-host
impact: low
---
# Interface gate lists are hand-synced with the WIT file

The interface gate keeps four hand-maintained copies of what it knows about the WIT interfaces in `gadgets/gadget-sdk/wit/torchsnap-gadget.wit`. Adding a gated interface and forgetting its `is_provisioned` arm makes the gate reject a gadget with "imports interfaces without matching permissions", although the manifest declares the permission and provisioning succeeded.

## Problem
The gate's knowledge of interfaces lives in four copies that must stay in sync with `gadgets/gadget-sdk/wit/torchsnap-gadget.wit`:
1. `UNGATED` (`src-tauri/src/wasm/interface_gate.rs`, around line 37)
2. the `is_provisioned` match arms (around lines 110 to 124)
3. the hardcoded gated-name lists repeated in three tests (around lines 263 to 277, 290 to 303 and 322 to 333)
4. the WIT interface names themselves

The failure mode is benign but confusing. The fallback `_ => false` is the right fail-safe default, but nothing tells the developer the gate itself is the stale piece.

## Impact
Each new capability requires touching this file in two places plus its tests. A miss surfaces as a misleading load error at runtime rather than a compile-time or test failure.

## Proposal
Options, smallest first:
- Add a test that parses the WIT file (it lives in-repo) and asserts every `torchsnap:gadget/*` interface is either in `UNGATED` or has an `is_provisioned` arm. The test fails with a message naming the stale list.
- Alternatively, derive both lists from a single `&[(&str, fn(&ProvisionedCaps) -> bool)]` table so there is one place to extend.
