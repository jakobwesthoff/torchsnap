# HTTP origin validation accepts opaque origins and drops paths silently

**Kind:** possible-bug
**Severity:** medium
**Area:** src-tauri/src/wasm/manifest/permissions/http.rs

## Problem
`HttpPermissionsDef::validate` normalizes each declared origin
via `url::Url::parse(...).origin().ascii_serialization()`
(`src-tauri/src/wasm/manifest/permissions/http.rs:64-78`). Two
inputs slip through in surprising ways:

1. **Opaque origins normalize to the literal string `"null"`.**
   `url::Origin` for non-tuple schemes (`file:`, `data:`,
   `mailto:`, anything non-http(s)/ws(s)/ftp) is opaque, and
   `ascii_serialization()` of an opaque origin is `"null"`.
   So `origins = ["file:///etc"]` validates successfully and
   stores the origin `"null"`. Whatever the runtime equality
   check compares against, the author's declared intent is
   gone, and multiple distinct opaque origins collapse into the
   same `"null"` entry.
2. **Path components are silently discarded.**
   `origins = ["https://api.example.com/v1/only"]` parses,
   normalizes to `https://api.example.com`, and thereby grants
   the whole host. An author who believed they were
   path-scoping the permission granted more than intended, with
   no warning.

No test covers either input.

## Impact
Case 1 stores a meaningless grant (dead at best, ambiguous at
worst; the runtime-equality side belongs to the deferred
security pass). Case 2 widens a grant beyond the author's
visible intent.

## Suggested fix
In `validate()`: reject origins whose parsed `Origin` is opaque
("origin must be a scheme+host, got `file:///etc`"), and reject
inputs whose URL has a non-`/` path, query, or fragment, telling
the author origins are host-granular. Add tests for both.
