---
title: "Add pinning and favorites to the result list"
kind: feature
component: frontend
horizon: someday
---
# Add pinning and favorites to the result list

Allow users to pin items so they always appear at the top of the result
list, or mark favorites that surface prominently in the empty query
state. This is deferred: it is not essential for the initial
implementation, and it can be added later without architectural impact.

## Considerations

- Pin to top of global list, or per-gadget pinning?
- Visible pin indicator in the result row?
- Keyboard shortcut to pin/unpin from the action palette?
- Storage: settings store or separate data file?
- Interaction with frecency ranking: do pinned items override
  learned rankings entirely?

## Revisit when

No trigger known; reconsider at the next sweep.
