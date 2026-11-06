---
title: "Correct the gadget logger example in logging-system.md"
kind: docs
component: project
impact: low
---
# Correct the gadget logger example in logging-system.md

`docs/api/logging-system.md` shows gadget views receiving `logger` and `sendMessage` as props. That contract predates ADR 0028, so a reader who copies the example gets `undefined` for both.

## Problem

The "Acquiring a logger" section (`docs/api/logging-system.md`)
shows:

```typescript
function ClipboardView({ logger, sendMessage, ...props }: GadgetViewProps) {
  logger.info("Clipboard view mounted");
}
```

That contract predates ADR 0028. The current `GadgetViewProps`
(`packages/gadget-sdk/src/types/gadget.ts`) carries only
per-render data (`results`, `data`, `query`, `matchedPrefix`);
the header comment of `gadget.ts` states explicitly that
`sendMessage`, `logger`, identity, and launcher actions flow
through the context hooks. The real `ClipboardView`
(`src/gadgets/clipboard/ClipboardView.tsx`) does:

```typescript
export default function ClipboardView({ query }: GadgetViewProps) {
  const { sendMessage, logger } = useGadgetRuntime();
```

The follow-up paragraph ("Deeper components in a gadget tree can
use the `useLogger()` hook", the same section of `logging-system.md`, with
`import { useLogger } from "../hooks/useLogger"`) is also
misleading for gadget authors: `useLogger` at
`src/hooks/useLogger.ts` is host-internal and not exported from
`@torchsnap/gadget-sdk/hooks` (which exports exactly
`useGadgetInfo`, `useGadgetRuntime`, `useLauncher`,
`useGadgetSetting`: `packages/gadget-sdk/src/shims/hooks.ts`).
A gadget bundle cannot import it by relative path.

The rest of the document checked out against the code
(constants, batching window, command table, span semantics,
worker local-ID mapping).

## Impact

A reader copies the props destructuring and gets `undefined` for
`logger`/`sendMessage` (TypeScript users get a type error;
JavaScript users get a runtime crash on `logger.info`).

## Suggested fix

Rewrite the "Acquiring a logger" section around
`useGadgetRuntime().logger` for gadget components, keeping
`useLogger()`/`LoggerProvider` only in a clearly host-internal
subsection (or dropping it from this gadget-facing doc). Align
the example with the real `ClipboardView` code.
