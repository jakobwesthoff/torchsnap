# Gadget runtime vertical (`src/gadget-runtime/`)

Consolidate all host-side gadget runtime infrastructure into
`src/gadget-runtime/`. Currently scattered across `src/lib/`,
`src/contexts/`, `src/hooks/`, and the top level of `src/gadgets/`.

Distinct from `src/gadgets/` which holds per-gadget UI implementations
(clipboard, bangs, emoji). This vertical is about the machinery that loads,
mounts, communicates with, and provides context for all gadgets.

## Why

These files all exist for the same reason: enabling the gadget runtime
on the frontend. They are tightly coupled to each other and change
together (e.g., any WIT interface update touches loader, bridge, and
context simultaneously). Scattering them across four folders obscures
this coupling and makes the blast radius of interface changes hard to see.

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
| `src/hooks/useGadgetSetting.ts` | merge with contexts version or deduplicate |
| `src/hooks/useGadgetStream.ts` | `src/gadget-runtime/useGadgetStream.ts` |

Note: there appear to be two `useGadgetSetting.ts` files — one in `contexts/`
and one in `hooks/`. Investigate and deduplicate before moving.

## Barrel export

Add `src/gadget-runtime/index.ts` exporting:
- `GadgetContextProvider` (mounted in launcher `main.tsx`)
- `useGadgetInfo`, `useGadgetRuntime`, `useGadgetSetting`, `useGadgetStream`
- `gadgetComponent` (used by WASM gadget mount sites)
- Gadget types (`GadgetInfo`, etc.)

Internal: `wasmGadgetLoader`, `gadgetCss`, `gadgetMessage`, `sdk`, `registry`,
`GadgetContext` — implementation details not exposed outside this vertical.

## Relationship to `src/gadgets/`

After the move, `src/gadgets/` contains only per-gadget UI folders:
```
src/gadgets/
├── bangs/
├── clipboard/
└── emoji/
```

These per-gadget UI implementations import from `src/gadget-runtime/` for
shared types and hooks, but the runtime machinery itself lives in
`src/gadget-runtime/`.

## Known callers to update

- `src/launcher/main.tsx` — mounts `GadgetContextProvider`
- `src/launcher/hooks/useControlChannel.ts` — likely uses gadget runtime
- `src/settings/GadgetSettingsWrapper.tsx` — uses gadget context
- `src/settings/sections/GadgetsManagementPanel.tsx` — uses gadget info hooks
- All per-gadget UI components in `src/gadgets/`

## Suggested approach

1. Investigate the `useGadgetSetting` duplication first — merge or clarify
2. Create `src/gadget-runtime/`, move files with `git mv`
3. Fix imports incrementally, keeping build green
4. Add barrel `index.ts`
5. Clean up now-empty locations in `src/contexts/`, `src/hooks/`, `src/lib/`

## Priority: MEDIUM-HIGH (largest scope, do after mascot + logger)
