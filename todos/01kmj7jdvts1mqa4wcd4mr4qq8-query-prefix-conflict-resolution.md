# Query prefix conflict resolution

When multiple plugins register the same query prefix (e.g. two plugins
both claiming `=`), we need a strategy to resolve the conflict.

## Current decision

First-come-first-serve — whichever plugin registers first wins. No
conflict detection or user notification.

## Future considerations

- **Specificity-based**: Longest prefix match wins (`=hex` beats `=`)
- **User-configurable priority**: Settings UI to reorder or disable
  plugin prefix claims
- **Conflict notification**: Warn the user when two plugins claim the
  same prefix, let them choose
- **Namespace enforcement**: Require unique prefixes in plugin manifests,
  reject duplicates at registration time
