---
kind: improvement
status: deferred
---

# Dynamic launcher window sizing

## Idea

Instead of sizing the window to the maximum possible content height,
resize it dynamically to match the actual content. Since the extra
area is transparent, resizing should be visually invisible.

This would reduce WebKit backing-store allocation further when showing
shorter views (emoji grid ~340px vs result list ~448px content area).

## Challenge

React renders are synchronous but window resizing goes through IPC to
native (async). When content grows, there's at least one frame where
the card is taller than the window and gets clipped at the bottom.
Shrinking is fine — transparent space appears, then window catches up.

### Possible solutions for the grow case

- **Predictive**: compute target height before rendering, resize
  window first (wait for IPC), then update React state. Inverts
  normal data flow.
- **Pre-sized slots**: each content mode (results, gadget) declares
  its height upfront. Launcher picks the right size before rendering.
- **Two-phase**: grow → resize window first (adds transparent space),
  then render. Shrink → render first, then resize.
- **ResizeObserver + accept brief clip**: let content render, observe
  size change, resize window. 1-frame clip on grow that may or may
  not be noticeable.

## Decision

Deferred. The memory savings (max ~110px of window height difference)
don't justify the complexity right now. Revisit if more gadget views
with varying heights are added or if the backing-store cost becomes
a concern again.
