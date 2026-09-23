# `asset-bang-data` commits whatever DDG returns without validating it is JSON

**Kind:** improvement
**Severity:** low
**Area:** just/bangs.just

## Problem

The recipe pipes the download straight into the bundled asset
(`just/bangs.just:13-20`):

```
curl -sL 'https://duckduckgo.com/bang.js' -o "$dst"
```

`curl -sL` follows redirects and writes whatever body arrives —
including CDN error pages, captive-portal HTML, or rate-limit
responses — with no `--fail` flag (non-2xx still writes the body
and exits 0 for most cases without `-f`) and no JSON validation.
A corrupted `gadgets/bangs/assets/bang.json` defeats the exact
purpose of the file: it is the *offline fallback* the bangs gadget
imports when the runtime network fetch fails
(`gadgets/bangs/src/lib.rs:532-539`), so a bad commit here means
first-launch-without-network users get an empty bang database and
only a devtools log entry.

## Suggested fix

Add `--fail` to curl and validate before moving into place:

```
curl -sSfL 'https://duckduckgo.com/bang.js' -o "$tmp"
bun -e "JSON.parse(require('fs').readFileSync('$tmp','utf8'))"
mv "$tmp" "$dst"
```

(any JSON validator available in the toolchain works; `bun` is
already a hard dependency).
