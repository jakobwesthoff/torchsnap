---
kind: bug
severity: medium
status: open
area: [docs/api/gadget-development.md]
tags: [docs]
---

# gadget-development.md skeleton and calculator sample omit enable()'s Result return

## Problem

The WIT `enable` export returns a result
(`gadgets/gadget-sdk/wit/torchsnap-gadget.wit:647`):

```wit
enable: func() -> result<_, string>;
```

and both shipped gadgets implement it that way
(`gadgets/calculator/src/lib.rs:88`, `gadgets/template/src/lib.rs:52`:
`fn enable() -> Result<(), String>`). Two of the doc's three
`enable()` examples still show the old no-return signature:

- The `src/lib.rs` skeleton
  (`docs/api/gadget-development.md:139`): `fn enable() { ... }`.
- The calculator worked example
  (`docs/api/gadget-development.md:1392`): `fn enable() { ... }`.

The `enable()` reference section
(`docs/api/gadget-development.md:249-260`) also doesn't mention
that a returned `Err` disables the gadget and stops further
dispatch, the contract the WIT doc-comment spells out at
`torchsnap-gadget.wit:645-646`.

(The doc's `execute`/`ScoredEntry` samples this todo originally
flagged are now correct: `execute(command: Command)` and
`ScoredEntry<C>` both match the current WIT. The ADR 55/56
rewrite fixed those.)

## Impact

A gadget author copying the skeleton or the calculator example
writes an `enable()` that fails to satisfy the `LifecycleGuest`
trait, and never learns that `enable()` failure disables the
gadget.

## Suggested fix

Change both examples to `fn enable() -> Result<(), String> { ... Ok(()) }`
(compare `gadgets/calculator/src/lib.rs:88-96`). Add a sentence to
the `enable()` reference section on the disable-on-error contract.
