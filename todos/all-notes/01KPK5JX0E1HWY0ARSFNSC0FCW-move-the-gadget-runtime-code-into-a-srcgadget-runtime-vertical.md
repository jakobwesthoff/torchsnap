---
title: "Move the gadget runtime code into a src/gadget-runtime vertical"
kind: refactor
component: frontend
---
# Move the gadget runtime code into a src/gadget-runtime vertical

Host-side gadget runtime infrastructure is scattered across `src/lib/`, `src/contexts/`, `src/hooks/` and the top level of `src/gadgets/`. Consolidate it into `src/gadget-runtime/`. This is the largest of the vertical moves, so it is medium-high priority and should follow the mascot and logger verticals.

The vertical is distinct from `src/gadgets/`, which holds per-gadget UI implementations. It covers the machinery that loads, mounts, communicates with and provides context for all gadgets. These files exist for the same reason, enabling the gadget runtime on the frontend, and they change together. Any WIT interface update touches the loader, the bridge and the context at once. Scattered across four folders, that coupling is hard to see and the blast radius of interface changes is hard to judge.

## Files to move

| Current path | Target path |
|---|---|
| `src/lib/gadgetComponent.tsx` | `src/gadget-runtime/gadgetComponent.tsx` |
| `src/lib/gadgetCss.ts` | `src/gadget-runtime/gadgetCss.ts` |
| `src/lib/gadgetMessage.ts` | `src/gadget-runtime/gadgetMessage.ts` |
| `src/lib/sdk.ts` | `src/gadget-runtime/sdk.ts` |
| `src/gadgets/wasmGadgetLoader.ts` | `src/gadget-runtime/wasmGadgetLoader.ts` |
| `src/gadgets/registry.ts` | `src/gadget-runtime/registry.ts` |
| `src/gadgets/types.ts` | `src/gadget-runtime/types.ts` |
| `src/contexts/GadgetContext.tsx` | `src/gadget-runtime/GadgetContext.tsx` |
| `src/contexts/GadgetContextProvider.tsx` | `src/gadget-runtime/GadgetContextProvider.tsx` |
| `src/contexts/useGadgetInfo.ts` | `src/gadget-runtime/useGadgetInfo.ts` |
| `src/contexts/useGadgetRuntime.ts` | `src/gadget-runtime/useGadgetRuntime.ts` |
| `src/contexts/useGadgetSetting.ts` | `src/gadget-runtime/useGadgetSetting.ts` |
| `src/hooks/useGadgetSetting.ts` | merge with the contexts version, or deduplicate |
| `src/hooks/useGadgetStream.ts` | `src/gadget-runtime/useGadgetStream.ts` |

There appear to be two `useGadgetSetting.ts` files, one in `contexts/` and one in `hooks/`. Investigate and deduplicate them before moving.

## Barrel export

Add `src/gadget-runtime/index.ts` exporting:
- `GadgetContextProvider` (mounted in the launcher `main.tsx`)
- `useGadgetInfo`, `useGadgetRuntime`, `useGadgetSetting`, `useGadgetStream`
- `gadgetComponent` (used by the WASM gadget mount sites)
- Gadget types (`GadgetInfo` and others)

Internal: `wasmGadgetLoader`, `gadgetCss`, `gadgetMessage`, `sdk`, `registry` and `GadgetContext` are implementation details and stay unexposed outside this vertical.

## Relationship to `src/gadgets/`

After the move, `src/gadgets/` contains only per-gadget UI folders:

```
src/gadgets/
├── bangs/
├── clipboard/
└── emoji/
```

These per-gadget UI implementations import shared types and hooks from `src/gadget-runtime/`, but the runtime machinery itself lives in `src/gadget-runtime/`.

## Known callers to update

- `src/launcher/main.tsx` mounts `GadgetContextProvider`.
- `src/launcher/hooks/useControlChannel.ts` likely uses the gadget runtime.
- `src/settings/GadgetSettingsWrapper.tsx` uses the gadget context.
- `src/settings/sections/GadgetsManagementPanel.tsx` uses the gadget info hooks.
- All per-gadget UI components in `src/gadgets/`.

## Suggested approach

1. Investigate the `useGadgetSetting` duplication first, and merge or clarify it.
2. Create `src/gadget-runtime/` and move the files with `git mv`.
3. Fix the imports incrementally, keeping the build green.
4. Add the barrel `index.ts`.
5. Clean up the locations that are now empty in `src/contexts/`, `src/hooks/` and `src/lib/`.
