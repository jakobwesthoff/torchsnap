---
title: "Set up frontend tests for the gadget contexts and add a host path alias to the SDK testing entry"
kind: chore
component: frontend
impact: low
origin: review
tags: [testing, tooling]
---
# Set up frontend tests for the gadget contexts and add a host path alias to the SDK testing entry

Two test-scaffolding items surfaced by the post-migration validation review of 2026-04-08 (seven parallel review agents covering D1-D6 and the calculator port). The critical and important findings of that review were fixed in 11 follow-up commits. These items are lower-priority work that was deliberately deferred. Neither blocks current functionality. They land as one PR because the workspace tsconfig alias (#25) is a prerequisite for the testing entry points, and the context tests (#24) are the first real users of `MockGadgetContextProvider`. This is the biggest of the review's polish slices because it requires setting up Vitest and React Testing Library from scratch on the host frontend. Each item keeps the original review section number so back-references stay intact.

## Scope

### #24: Context test scaffolding

**Where:** `src/contexts/__tests__/` (the directory does not exist yet, checked 2026-10-21).

**The thing:** the entire `src/contexts/` gadget component infrastructure ships without unit tests. The migration plan called for "extensive test coverage" of every new behaviour. This is the largest gap.

**Concrete tests to add:**

- **`useLauncher.test.tsx`**: `useLauncher()` throws with the exact expected message when called (a) outside any provider and (b) inside a `GadgetContextProvider` with no `launcher` slice. Use `renderHook` and `expect(() => ...).toThrow(...)`.
- **`useGadgetSetting.test.tsx`**: `useGadgetSetting("foo")` inside a provider with `id: "calculator"` reads `gadgets.calculator.foo` from the underlying store. This verifies the namespace prefixing invariant the entire settings contract depends on.
- **`GadgetContextProvider.test.tsx`**: a child calling the legacy `useLogger()` inside `GadgetContextProvider` receives the same `Logger` instance as `useGadgetRuntime().logger`. This guards the dual-drive coupling that was deliberately kept when the new contract was built.
- **`Launcher.test.tsx`** (a new file, or folded into an existing Launcher test): the `useOptionalGadgetEnabled` path with `customGadgetView == null` resolves to `false` without reading any real `enabled.<gadget-id>` key.

**Infrastructure:** Vitest, React Testing Library and `src/test/` helpers are in place (`vitest.config.ts`, `bun run test`), and `packages/gadget-sdk/src/testing/MockGadgetContextProvider.test.tsx` is the only test of the SDK testing entry so far. `MockGadgetContextProvider` is the basis for the context tests.

### #25: Workspace tsconfig path alias for `@torchsnap/host/*`

**Where:** `packages/gadget-sdk/tsconfig.json` (does not exist yet), `packages/gadget-sdk/src/testing/setup.ts`, `packages/gadget-sdk/src/testing/MockGadgetContextProvider.tsx`.

**The thing:** the testing entry point imports host source through relative paths:

```ts
import { initGadgetSdk } from "../../../../src/lib/sdk";
import { GadgetContext } from "../../../../src/contexts/GadgetContext";
```

`MockGadgetContextProvider.tsx` also imports `KeyBindingProvider` from `src/keybindings` this way. The paths are four levels up (`../../../../src/...`). They work inside the monorepo but break as soon as the SDK is published to npm or moved out of the workspace. `setup.ts` documents the deferral, but the relative paths are still spread across two files.

**Fix:** add a `packages/gadget-sdk/tsconfig.json` with a `paths` mapping `"@torchsnap/host/*": ["../../src/*"]` (relative to `packages/gadget-sdk/`), and rewrite the host imports in both files to `import ... from "@torchsnap/host/lib/sdk"` and `"@torchsnap/host/contexts/GadgetContext"`. The intent ("reaching into host source") becomes structural rather than visual.

When the SDK is published, swap the path alias for a real `@torchsnap/host` peer dependency or a separate `@torchsnap/host-fixtures` package. Only the tsconfig entry changes, not every file that uses it.

## Done when

- The `@torchsnap/host/*` alias is in place and both testing-entry imports use it.
- The four test files above exist and pass.
