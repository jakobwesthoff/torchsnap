---
title: "Size the launcher window to its content height"
kind: improvement
component: frontend
horizon: someday
tags: [ux, memory]
---
# Size the launcher window to its content height

The launcher window is sized to the maximum possible content height. Resizing it to the actual content would reduce WebKit backing-store allocation further when a shorter view is shown, such as the emoji grid (about 340px of content area) against the result list (about 448px). The extra area is transparent, so the resize should be visually invisible.

## Proposal

Resize the window dynamically to match the content. The challenge is the grow case. React renders are synchronous, but window resizing goes through IPC to native code, which is async. When content grows, at least one frame shows the card taller than the window, and the card is clipped at the bottom. Shrinking is fine: transparent space appears, then the window catches up.

## Open questions

Options for the grow case:

- **Predictive.** Compute the target height before rendering, resize the window first (wait for IPC), then update React state. This inverts the normal data flow.
- **Pre-sized slots.** Each content mode (results, gadget) declares its height upfront, and the launcher picks the right size before rendering.
- **Two-phase.** Grow: resize the window first (adds transparent space), then render. Shrink: render first, then resize.
- **ResizeObserver and accept a brief clip.** Let the content render, observe the size change, then resize the window. This leaves a one-frame clip on grow that may or may not be noticeable.

## Decision

Deferred. The memory savings (at most about 110px of window height difference) do not justify the complexity right now.

## Revisit when

Revisit if more gadget views with varying heights are added, or if the backing-store cost becomes a concern again.
