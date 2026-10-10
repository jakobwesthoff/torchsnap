---
title: "Rename wasmPluginLoader.ts to wasmGadgetLoader.ts"
kind: refactor
component: frontend
impact: low
tags: [ux]
---
# Rename wasmPluginLoader.ts to wasmGadgetLoader.ts

ADR 0042 renamed "plugins" to "gadgets" project-wide, but the loader file was missed. It still lives at `src/gadgets/wasmPluginLoader.ts`, while everything inside it uses gadget terminology: `registerWasmGadget` (around line 38), `registerAllWasmGadgets` (around line 115) and `WasmGadgetManifest`.

## Impact

The architecture documentation already cites the post-rename name. `docs/Gadget-Architecture/04-frontend-reception.md` (line 189) gives "`registerAllWasmGadgets()` (`src/gadgets/wasmGadgetLoader.ts`)", and a reader following that path finds nothing. Grepping for "plugin" in the frontend also still hits a load-bearing module name, which dilutes the ADR 0042 rename.

## Suggested fix

Move the file to `src/gadgets/wasmGadgetLoader.ts` with `git mv`, and update the import sites found by grepping for `wasmPluginLoader`. The doc reference then becomes correct as written.

## Relations

- Relates to: [Move the gadget runtime code into a src/gadget-runtime vertical](01KPK5JX0E1HWY0ARSFNSC0FCW-move-the-gadget-runtime-code-into-a-srcgadget-runtime-vertical.md), which moves the renamed loader.
