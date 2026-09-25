# 54. Open gadget settings through a post-action

Date: 2026-09-25

## Status

Accepted

Amends [41. ZeroTier plugin architecture](0041-zerotier-plugin.md)

Amended by [55. Dispatch entry actions through gadget commands in fixed slots](0055-dispatch-entry-actions-through-gadget-commands-in-fixed-slots.md)

## Context

`ActionId::OpenSettings` was intercepted in `GadgetHost::execute`
before the gadget saw it. The host emitted an `open-gadget-settings`
event and dismissed the launcher. Nothing listened for that event, so
the action opened no window. The ZeroTier gadget puts this action on
its "token not configured" and "authentication failed" entries.

## Decision

`ActionId::OpenSettings` reaches the gadget's `execute()` like every
other action.

The WIT `post-action` enum gains `open-settings`, and the host
`PostAction` enum gains `OpenSettings`. When `execute()` returns it,
the host dismisses the launcher and opens the Settings window on the
executing gadget's section. A gadget without a settings section of its
own gets the Gadgets page.

The WIT package keeps the version `torchsnap:gadget@0.1.0`.

## Consequences

A gadget that shows an `OpenSettings` action answers it with
`open-settings` from `execute()`; the ZeroTier gadget does.

The requested section reaches the Settings window through the same
one-time start section that "Restart now" uses. An open window is
told to take it by the `settings-start-section` event.
