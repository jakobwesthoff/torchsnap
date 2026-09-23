# Gadget asset protocol does not percent-decode request paths

**Kind:** possible-bug
**Severity:** medium
**Area:** src-tauri/src/wasm/protocol.rs

## Problem
`serve_gadget_asset` takes the raw URI path and uses it directly
as gadget id and source-relative file path
(`src-tauri/src/wasm/protocol.rs:116-123`):

```rust
let path = request.uri().path();
let trimmed = path.trim_start_matches('/');
let (gadget_id, file_path) = trimmed.split_once('/').ok_or_else(...)?;
```

No percent-decoding happens anywhere before
`source.read_file(file_path)` (`:147`). The webview
percent-encodes URLs, so any asset whose real file name contains
a space, `#`, `%`, or non-ASCII character arrives as `%20`,
`%23`, `%25`, `%C3%A9`, … and will not match the entry name in
the archive/directory source. The request 404s even though the
file exists.

None of the tests exercise an encoded path; the whole suite uses
plain ASCII names, which is why this has not surfaced.

## Impact
Gadget frontend assets with spaces or non-ASCII names silently
fail to load (404 with "file not found"). Gadget authors have no
indication why; the file is visibly present in their bundle.

## Suggested fix
Split the raw path on the first `/` first (fixing the
gadget-id / file-path boundary before any decoding), then
percent-decode only the file-path segment once via
`percent_encoding::percent_decode_str(file_path)` and pass the
decoded path to `source.read_file`. The decoded path flows into
the source's own `validate_gadget_path` guard, so decoding must
happen before that validation, not after. Add tests with `%20`
and a UTF-8 encoded name.

## Security-pass assessment (2026-07-02)
There is **no live traversal bug today**: because nothing decodes,
an encoded traversal like
`%2e%2e/%2e%2e/etc/passwd` reaches `validate_gadget_path`
(`source.rs:319-377`) as literal `Component::Normal("%2e%2e")`
segments, so the depth counter only increases, validation
passes, and the lookup fails on a literal filename (404). At
runtime the platform webview's WHATWG URL parser normalizes
`%2e`/`%2E` dot-segments to `..` and collapses them *before*
issuing the request on all three platforms, so an encoded
traversal never even arrives encoded; the literal-`%2e%2e`-to-
validator path is reachable only from unit tests constructing
raw `http::Request`s. The handler-level guard must nonetheless
remain the authority.

The finding is therefore a **guard-rail on the future decoding
fix**, not a current vulnerability. When that fix lands it must
satisfy all of:

1. Decode in `protocol.rs` only, on the file-path segment only,
   after `split_once('/')`, before `source.read_file`. Never
   decode inside `source.rs`; never move validation out of the
   source layer.
2. Split the raw path first, then decode (this supersedes the
   original "decode before splitting" wording above). Splitting
   the undecoded path means a `%2F` can never move the
   gadget-id / file-path boundary.
3. Do not decode `gadget_id`. The id charset is `[a-z0-9-]`
   (`manifest/mod.rs:185-204`), so an encoded id simply 404s at
   registry lookup.
4. Decode exactly once, strict UTF-8:
   `percent_decode_str(file_path).decode_utf8()`, mapping `Err`
   to 400 (not `decode_utf8_lossy` — rejecting invalid UTF-8
   blocks overlong-encoding smuggling such as `%c0%ae`). Never
   decode a second time downstream, so `%252e%252e` decodes once
   to literal `%2e%2e` and 404s. This is path decoding, so `+`
   stays `+`.
5. `validate_gadget_path` itself needs no change: post-decode
   strings containing `/`, `\` (`%5C`), NUL (`%00`), or real
   `..` (`..%2F..`) are already rejected.

Regression tests to ship with the fix: positive `%20` and UTF-8
filename; negative `.../%2e%2e/%2e%2e/etc/passwd`, `..%2Fsecret`,
`%252e%252e%2Fx` (must 404, not traverse), `%00x`, `foo%5Cbar`,
`%c0%ae%c0%ae` (400); and one asserting the split happens on the
raw path (`/g%2Fx/file` → unknown gadget).

Severity of the standalone finding: informational / prospective
(no current vulnerability).
