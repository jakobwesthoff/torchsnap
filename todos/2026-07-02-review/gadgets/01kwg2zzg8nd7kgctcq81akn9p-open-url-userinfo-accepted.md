# open-url: bare-domain detection accepts userinfo — email-shaped input becomes a URL entry

**Kind:** improvement
**Severity:** low
**Area:** gadgets/open-url/src/lib.rs

## Problem

The bare-domain path in `detect_url`
(`gadgets/open-url/src/lib.rs:93-121`) prepends `https://` and
validates only the parsed *host* against the PSL:

```rust
let candidate = format!("https://{trimmed}");
let parsed = url::Url::parse(&candidate).ok()?;
let host = parsed.host_str()?;
```

For input containing a `@`, the URL parser treats everything
before it as userinfo: typing `jane.doe@gmail.com` (an email
address) yields host `gmail.com`, which passes the PSL check, so
the launcher shows "Open https://jane.doe@gmail.com" as a result.
Anything email-shaped that a user types or pastes into the
launcher produces a spurious URL entry whose subtitle displays
the userinfo-bearing URL.

There is no security angle (the user typed the string themselves
and the browser resolves to the real host); it is result-list
noise plus a slightly confusing "Open https://user@host" label.
The test suite (`lib.rs:242-348`) has no case with `@` input.

## Suggested fix

In the bare-domain branch, reject candidates where
`parsed.username() != "" || parsed.password().is_some()` (keep the
explicit-scheme branch as-is — a user typing
`https://user:pass@host` committed to it). Add a
`detect_url("jane.doe@gmail.com")` → `None` test. If mailto
support is ever wanted, that is a separate feature, not this
code path.
