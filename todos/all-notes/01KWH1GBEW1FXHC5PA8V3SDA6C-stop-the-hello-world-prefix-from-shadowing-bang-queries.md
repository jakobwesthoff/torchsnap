---
title: "Stop the hello-world prefix from shadowing bang queries"
kind: bug
component: gadgets
impact: low
---
# Stop the hello-world prefix from shadowing bang queries

The hello-world demo gadget claims `!` as an exclusive prefix, so any query
starting with `!` skips the bangs gadget. That breaks leading-bang syntax
such as `!g rust wasm`, which the bangs gadget supports, while mid-query
bangs still work. Read in the code, not reproduced.

## Problem

Two in-tree gadgets claim the same trigger character with
incompatible mechanisms:

- `gadgets/hello-world/manifest.toml`, line 8, declares
  `prefixes = ["!"]`. Prefix routing is *exclusive* (ADR 0012):
  when an enabled gadget's declared prefix matches the start of
  the query, only that gadget's `search()` runs, and all other
  gadgets are bypassed for the keystroke
  (`GadgetHost::search` prefix branch, `src-tauri/src/gadget_host.rs`).
- The bangs gadget declares no prefix and detects `!bang` tokens
  anywhere in the query (`gadgets/bangs/src/lib.rs`, around lines 297-303,
  `find_bang_token`). Leading-bang syntax is an explicitly
  supported and tested case:
  `find_bang_token_leading` (`lib.rs`, around lines 613-618) asserts
  `"!g rust wasm"` resolves the `g` trigger.

With both gadgets enabled, any query beginning with `!` routes
exclusively to hello-world, so `!g rust wasm` renders
hello-world's echo view instead of a bang search. The
mid-query form (`rust !g wasm`) still works, which makes the
breakage look intermittent.

## Impact

- Release bundles are unaffected today: `gadgets/bundled.toml`
  ships calculator, emoji-picker, bangs, open-url, awake, and not
  hello-world.
- Every debug build is affected: the dev discovery root loads
  every directory under `gadgets/` (ADR 0035), so hello-world
  is present, and if enabled it starves bangs of its primary
  syntax. Confusing while developing or testing bangs.
- A user who manually installs hello-world as a `.torchsnap`
  archive reproduces the conflict in release builds.

The architecture has no prefix-collision handling between an
exclusive prefix and a prefix-free gadget's in-query trigger:
`find_prefix_match` only resolves collisions between *declared*
prefixes (longest wins).

## Suggested fix

Change hello-world's demo prefix to something unclaimed (e.g.
`hello:`, since the template gadget already uses `tpl:`). Optionally,
add a startup warning when an enabled gadget declares a prefix
equal to `"!"`-style single characters that another bundled
gadget consumes in-query, or document the reserved trigger
characters in `docs/api/gadget-development.md`.
