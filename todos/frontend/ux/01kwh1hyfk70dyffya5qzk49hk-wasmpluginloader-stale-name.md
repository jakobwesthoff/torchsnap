---
kind: improvement
severity: low
status: open
area: [src/gadgets/wasmPluginLoader.ts]
---

# `wasmPluginLoader.ts` kept its pre-rename name; docs already cite `wasmGadgetLoader.ts`

## Problem

ADR 0042 renamed "plugins" to "gadgets" project-wide. The loader
file was missed: it still lives at
`src/gadgets/wasmPluginLoader.ts` while everything inside it uses
gadget terminology (`registerWasmGadget` at `:38`,
`registerAllWasmGadgets` at `:115`, `WasmGadgetManifest`).

The architecture documentation already refers to the post-rename
name that does not exist:
`docs/Gadget-Architecture/04-frontend-reception.md:189` cites
"`registerAllWasmGadgets()` (`src/gadgets/wasmGadgetLoader.ts`)".
A reader following that path finds nothing.

## Impact

Broken doc pointer; grep for "plugin" in the frontend still hits
a load-bearing module name, diluting the ADR 0042 rename.

## Suggested fix

`git mv src/gadgets/wasmPluginLoader.ts src/gadgets/wasmGadgetLoader.ts`
and update the import sites (grep for `wasmPluginLoader`). The doc
reference then becomes correct as written.
