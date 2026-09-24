---
kind: bug
severity: high
status: open
area: [src-tauri/src/lib.rs]
---

# One invalid WASM gadget panics the whole app at startup

## Problem
The gadget loader promises isolation: "On any error — corrupt
archive, missing manifest, failing path guard — log and move on
so one broken gadget does not prevent the rest from loading"
(`src-tauri/src/lib.rs:1049-1052`). That holds for source-open
and cap-provisioning failures, but not for bridge construction.

`load_single_wasm_gadget` builds the bridge inside the
`register_with_caps` factory closure, whose signature is
infallible (`FnOnce(Arc<ProvisionedCaps>) -> G`,
`gadget_host.rs:373-385`), so the fallible
`WasmGadgetBridge::new` is unwrapped with a panic
(`src-tauri/src/lib.rs:1198-1208`):

```rust
|caps| {
    wasm::bridge::WasmGadgetBridge::new(...)
        .expect("bridge construction after cap provisioning")
},
```

`WasmGadgetBridge::new` is where the expensive work lives —
including the WASM compile (per the comment at
`lib.rs:1134-1138`, the id-collision check was deliberately
moved *before* "the expensive WASM compile in
`WasmGadgetBridge::new`"). A gadget whose manifest parses fine
but whose `.wasm` fails to compile/instantiate therefore panics
inside `setup`, and the whole app fails to launch.

The error-handling comment at `lib.rs:1117-1126` ("The
registration itself is already rolled back — ... the error
happens before it") documents only the `Err` paths and is
silent about the panic path, which suggests the expect was not
a considered trade-off.

## Impact
A user installs (or a developer drops into `gadgets/`) a gadget
with a corrupt or ABI-incompatible wasm binary → Torchsnap
crashes on every startup until the file is manually deleted.
This is the exact failure the loader's isolation design exists
to prevent, and user-installed archives make it reachable by
non-developers.

## Suggested fix
Make the factory path fallible: either change
`register_with_caps` to accept
`FnOnce(...) -> anyhow::Result<G>`, or split provisioning from
registration (build caps, construct the bridge outside the
closure, then register the ready gadget). Add a regression test
that loads a syntactically-valid-manifest/invalid-wasm gadget
and asserts the app continues loading the remaining gadgets.
