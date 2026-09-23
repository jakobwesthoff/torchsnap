# Command-rule validation gaps: uncompiled globs, zero ceilings

**Kind:** possible-bug
**Severity:** low
**Area:** src-tauri/src/wasm/manifest/permissions/command.rs

## Problem
Two parse-time validation gaps in `[[permissions.command]]`
rules:

1. Glob patterns are checked for non-emptiness only
   (`src-tauri/src/wasm/manifest/permissions/command.rs:378-381`);
   the doc comment says "full glob compilation happens in the
   matcher" (`:350-354`). Regex constraints, by contrast, are
   compile-tested at parse time (`:383-391`). A syntactically
   invalid glob (e.g. `pattern = "[unclosed"`) therefore passes
   manifest validation and only fails later when
   `argv_matcher::compile_rule` runs
   (`wasm/argv_matcher.rs:167-174`), turning an authoring error
   into a late load/enable failure instead of a parse error.
2. The numeric ceilings accept degenerate values:
   `timeout-ms-max = 0`, `max-output-bytes = 0`, and
   `max-stdin-bytes = 0` all parse (`:56-69`). A zero timeout
   ceiling clamps every invocation to 0 ms; a zero output cap
   truncates all output. Both are almost certainly authoring
   mistakes and could be rejected with a pointed message.

## Impact
Bad globs and zero ceilings load "successfully" and misbehave at
runtime, costing gadget authors a debugging round-trip that
parse-time validation is specifically there to prevent.

## Suggested fix
Compile-test glob patterns in `validate_argv_constraint` exactly
like regex (compile with the same builder options the matcher
uses, discard the result). Reject zero for the three ceilings.
Add parse tests for both.
