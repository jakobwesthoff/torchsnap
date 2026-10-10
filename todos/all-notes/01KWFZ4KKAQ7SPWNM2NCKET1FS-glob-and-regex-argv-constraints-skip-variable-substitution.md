---
title: "Glob and regex argv constraints skip variable substitution"
kind: bug
component: gadget-host
tags: [error-handling]
---
# Glob and regex argv constraints skip variable substitution

Permission rules with `${...}` variables in `glob` or `regex` argv constraints never match, and nothing reports it.

## Problem

Read in the code, not reproduced.

`compile_constraint` in `src-tauri/src/wasm/argv_matcher.rs` (around lines 146 to 165 and 191 to 198) substitutes `${...}` path variables for `literal`, `enum` and `path-under` constraints via `resolver.substitute_variables(...)`. It does NOT substitute them for `glob` (around lines 167 to 174) or `regex` (around lines 176 to 189), which are compiled from the raw manifest string.

A gadget author who writes a pattern containing a variable, for example

```toml
argv = [{ glob = "${gadget-data}/exports/*" }]
```

gets a rule that compiles successfully but can never match. The runtime argv contains the resolved path, while the pattern still contains the literal `${gadget-data}` text. Worse, in globset syntax `{...}` is alternation, so `${gadget-data}` parses as `$` followed by the single alternative `gadget-data`. That compiles cleanly and hides the mistake. There is no manifest-time diagnostic for it.

## Impact

Permission rules containing variables in glob or regex patterns are silently dead. The gadget's `command::run` calls fail with `NoMatchingRule` at runtime, with no hint that the pattern text was never substituted. Debugging requires reading host source.

## Suggested fix

Pick one and document it:

1. Substitute variables in glob and regex patterns too (before compilation), consistent with the other constraint kinds; or
2. Reject `${` sequences in glob and regex patterns at manifest validation time, with an error pointing authors to `path-under` or `literal`.

Option 2 avoids questions about escaping substituted paths inside regex and glob metacharacter contexts. Either way, add a manifest validator test covering a variable inside each constraint kind.
