# Variable substitution skipped for glob/regex argv constraints

**Kind:** possible-bug
**Severity:** medium
**Area:** src-tauri/src/wasm/argv_matcher.rs

## Problem
`compile_constraint` substitutes `${...}` path variables for
`literal`, `enum`, and `path-under` constraints via
`resolver.substitute_variables(...)`
(`src-tauri/src/wasm/argv_matcher.rs:146-165` and `191-198`), but
NOT for `glob` (`:167-174`) and `regex` (`:176-189`), which are
compiled from the raw manifest string.

A gadget author who writes a pattern containing a variable, e.g.

```toml
argv = [{ glob = "${gadget-data}/exports/*" }]
```

gets a rule that compiles successfully but can never match: the
runtime argv contains the real resolved path while the pattern
still contains the literal `${gadget-data}` text. Worse, in
globset syntax `{...}` is alternation, so `${gadget-data}` is
parsed as `$` followed by the single alternative `gadget-data`,
which compiles cleanly and hides the mistake. There is no
manifest-time diagnostic for this.

## Impact
Permission rules containing variables in glob/regex patterns are
silently dead. The gadget's `command::run` calls fail with
`NoMatchingRule` at runtime with no hint that the pattern text
was never substituted. Debugging this requires reading host
source.

## Suggested fix
Pick one and document it:
1. Substitute variables in glob/regex patterns too (before
   compilation), consistent with the other constraint kinds; or
2. Reject `${` sequences in glob/regex patterns at manifest
   validation time with an error pointing authors to
   `path-under`/`literal`.
Option 2 avoids questions about escaping substituted paths inside
regex/glob metacharacter contexts. Either way, add a manifest
validator test covering a variable inside each constraint kind.
