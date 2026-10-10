---
title: "Path variable name list is maintained in three places"
kind: refactor
component: gadget-host
impact: low
tags: [capabilities]
---
# Path variable name list is maintained in three places

The valid `${...}` variable names are spelled out separately in three resolvers in `src-tauri/src/paths.rs`, and the tests repeat the list a fourth time. Adding or renaming a variable in one place without the others makes manifests fail with a confusing "unknown variable" error. The lists are in sync today, so this is a dual-maintenance hazard, not a live bug.

## Problem
The set of valid `${...}` variable names is spelled out
independently three times in `src-tauri/src/paths.rs`:

- `PlatformPaths::is_recognized`: `home | xdg-config |
  xdg-data` (`paths.rs:117-119`), plus the matching `resolve`
  arms (`paths.rs:121-128`);
- `GadgetPaths::is_recognized`: `gadget-data | gadget-archive`
  plus platform delegation (`paths.rs:146-148`), plus `resolve`
  (`paths.rs:150-156`);
- `ParseTimeResolver::is_recognized`: the full five-name union
  repeated literally (`paths.rs:170-175`).

`ParseTimeResolver` is the parse-time gate for manifests
(validators run before a `GadgetPaths` exists). Adding a new
variable to `GadgetPaths` without updating `ParseTimeResolver`
makes every manifest using it fail validation at parse time,
even though the runtime could resolve it. The reverse drift
(a name in `ParseTimeResolver` only) admits manifests that then
fail at provisioning. The unit tests hardcode the same
five-name list a fourth time (`paths.rs:334`, `:452`), so they
verify consistency only as long as someone also updates the
test list.

## Impact
Pure dual-maintenance hazard today, since the lists are currently in
sync. The failure mode when they drift is a confusing
"unknown variable" error pointing at a manifest that follows
the documented variable set.

## Suggested fix
Define the name list once (a `const` slice, or an enum with
`name()` and `from_name()`), derive `is_recognized` for all three
resolvers from it, and let `ParseTimeResolver` be "recognizes
everything the enum defines". A test asserting that
`GadgetPaths`-recognized names are a subset of `ParseTimeResolver`-recognized names
would lock the invariant without hardcoding names.
