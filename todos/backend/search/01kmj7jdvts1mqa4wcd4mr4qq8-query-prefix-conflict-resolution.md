# Query prefix conflict resolution

When multiple gadgets register the same query prefix (e.g. two gadgets
both claiming `=`), we need a strategy to resolve the conflict.

## Current decision

First-come-first-serve — whichever gadget registers first wins. No
conflict detection or user notification.

## Future considerations

- **Specificity-based**: Longest prefix match wins (`=hex` beats `=`)
- **User-configurable priority**: Settings UI to reorder or disable
  gadget prefix claims
- **Conflict notification**: Warn the user when two gadgets claim the
  same prefix, let them choose
- **Namespace enforcement**: Require unique prefixes in gadget manifests,
  reject duplicates at registration time
