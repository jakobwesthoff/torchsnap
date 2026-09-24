---
kind: bug
severity: medium
status: open
area: [docs/api/gadget-development.md]
tags: [docs]
---

# gadget-development.md code samples no longer match the WIT (execute, enable, ScoredEntry)

## Problem

The guide declares the WIT the source of truth
(`docs/api/gadget-development.md:8-12`) but its code samples have
drifted from it in three compile-breaking ways.

### 1. `execute` takes the full entry, not an id string

WIT (`gadgets/gadget-sdk/wit/torchsnap-gadget.wit:780`):

```wit
execute: func(entry: scored-entry, action-id: action-id) -> result<post-action, string>;
```

The doc consistently shows the old id-based signature:

- Trait table (`gadget-development.md:188`): "`entries`, `search`,
  `execute`" with skeleton
  `fn execute(_entry_id: String, _action_id: ActionId)`
  (`gadget-development.md:143`).
- Actions section example (`gadget-development.md:376-386`):
  `fn execute(entry_id: String, action_id: ActionId)` matching on
  `entry_id.as_str()`.

### 2. `ScoredEntry` is missing the `data` field

WIT (`torchsnap-gadget.wit:736-739`) added an opaque round-trip
payload:

```wit
/// Opaque gadget-defined payload round-tripped by the host.
/// Attached in `search()`, passed back in `execute()`. The
/// host never inspects the contents.
data: option<string>,
```

The doc's `ScoredEntry` listing (`gadget-development.md:308-318`)
has no `data` field and nothing in the guide mentions the
round-trip mechanism, which is exactly the facility a gadget
author needs for stateless `execute` dispatch under the new
signature.

### 3. `enable()` returns a `Result`

WIT (`torchsnap-gadget.wit:638-643`):

```wit
/// Returning `err(string)` signals the host that initialization
/// failed — the gadget will be disabled and no further calls
/// dispatched to it.
enable: func() -> result<_, string>;
```

The skeleton (`gadget-development.md:130-136`), the lifecycle
section (`gadget-development.md:207-218`), and the calculator
worked example (`gadget-development.md:1204-1220`) all show
`fn enable()` with no return value, and the "error disables the
gadget" contract is undocumented.

## Impact

A gadget author following the guide writes trait impls that fail
to compile against the current SDK bindings, and never learns
about the `data` round-trip or the enable-failure contract.

## Suggested fix

Update the skeleton, the actions section, the `ScoredEntry`
listing, and the worked example to the current WIT signatures
(compare against `gadgets/calculator/src/lib.rs` /
`gadgets/template/src/lib.rs`, which build in CI). Add a short
paragraph on `ScoredEntry.data` as the intended state carrier
between `search()` and `execute()`, and one on
`enable() -> Result` semantics.
