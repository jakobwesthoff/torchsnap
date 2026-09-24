---
kind: bug
severity: low
status: open
area: [src/gadgets/clipboard/ClipboardSettings.tsx]
tags: [unconfirmed]
---

# ClipboardSettings "Clear All" disabled-condition uses `&&` where the semantics need `||`

## Problem

The initial "Clear All" button is disabled only when the gadget is
disabled *and* the history is empty
(`src/gadgets/clipboard/ClipboardSettings.tsx:196-203`):

```tsx
<button
  onClick={handleClearHistory}
  disabled={!enabled && stats?.totalEntries === 0}
```

Read literally, this enables the destructive button in both odd
states:

- gadget **disabled** with entries present — clearing "works" only
  because `ClipboardGadget::disable()` currently fails to release
  its state (see
  `todos/gadgets/clipboard/01kwg2s5na69epssnnvgwjyvy0-clipboard-enable-panics-state-never-cleared.md`);
  once that lifecycle bug is fixed, this click will produce an
  error (or a panic, pre-fix) instead of being unreachable.
- gadget **enabled** with zero entries — harmless no-op, but a
  live destructive button with nothing to destroy.

The confirm-state button (`ClipboardSettings.tsx:181-188`) has no
enabled/empty guard at all, only `disabled={clearing}` — consistent
with the same intent gap.

Note the "Refresh" stats button and the mount-time `stats` fetch
(`ClipboardSettings.tsx:76-85`) also call into the gadget while it
is disabled; harmless today for the same accidental reason, worth
deciding intentionally when the lifecycle todo is fixed.

## Suggested fix

Decide the intended matrix and encode it:

```tsx
disabled={!enabled || clearing || (stats?.totalEntries ?? 0) === 0}
```

(or keep clear-while-disabled as a deliberate feature, in which
case the lifecycle fix must keep `handle_message` functional for
disabled gadgets — record whichever is chosen).
