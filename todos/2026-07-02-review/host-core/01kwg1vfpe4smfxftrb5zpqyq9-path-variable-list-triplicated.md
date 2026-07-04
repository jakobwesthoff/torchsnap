# Path-variable name list is maintained in three places

**Kind:** refactor
**Severity:** low
**Area:** src-tauri/src/paths.rs

## Problem
The set of valid `${...}` variable names is spelled out
independently three times in `src-tauri/src/paths.rs`:

- `PlatformPaths::is_recognized` — `home | xdg-config |
  xdg-data` (`paths.rs:117-119`), plus the matching `resolve`
  arms (`paths.rs:121-128`);
- `GadgetPaths::is_recognized` — `gadget-data | gadget-archive`
  + platform delegation (`paths.rs:146-148`), plus `resolve`
  (`paths.rs:150-156`);
- `ParseTimeResolver::is_recognized` — the full five-name union
  repeated literally (`paths.rs:170-175`).

`ParseTimeResolver` is the parse-time gate for manifests
(validators run before a `GadgetPaths` exists). Adding a new
variable to `GadgetPaths` without updating `ParseTimeResolver`
makes every manifest using it fail validation at parse time
even though the runtime could resolve it; the reverse drift
(name in `ParseTimeResolver` only) admits manifests that then
fail at provisioning. The unit tests hardcode the same
five-name list a fourth time (`paths.rs:334`, `:452`), so they
verify consistency only as long as someone also updates the
test list.

## Impact
Pure dual-maintenance hazard today (the lists are currently in
sync). The failure mode when they drift is a confusing
"unknown variable" error pointing at a manifest that follows
the documented variable set.

## Suggested fix
Define the name list once (a `const` slice or an enum with
`name()`/`from_name()`), derive `is_recognized` for all three
resolvers from it, and let `ParseTimeResolver` be "recognizes
everything the enum defines". A test asserting
`GadgetPaths`-recognized ⊆ `ParseTimeResolver`-recognized would
lock the invariant without hardcoding names.
