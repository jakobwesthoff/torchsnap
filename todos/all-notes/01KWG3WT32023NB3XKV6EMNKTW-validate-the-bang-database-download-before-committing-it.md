---
title: "Validate the bang database download before committing it"
kind: bug
component: project
impact: low
---
# Validate the bang database download before committing it

`asset-bang-data` writes whatever DuckDuckGo returns into the bundled bang asset, without checking that the response is JSON. A failed download can therefore replace the offline fallback with a non-JSON body, and first-launch users without network then get an empty bang database.

## Problem

The recipe pipes the download straight into the bundled asset (`just/bangs.just:13-20`):

```
curl -sL 'https://duckduckgo.com/bang.js' -o "$dst"
```

`curl -sL` follows redirects and writes whatever body arrives, including CDN error pages, captive-portal HTML, or rate-limit responses. It runs without a `--fail` flag (non-2xx still writes the body and exits 0 for most cases without `-f`) and without JSON validation. A corrupted `gadgets/bangs/assets/bang.json` defeats the exact purpose of the file: it is the *offline fallback* the bangs gadget imports when the runtime network fetch fails (`gadgets/bangs/src/lib.rs:532-539`). A bad commit here means first-launch-without-network users get an empty bang database and only a devtools log entry.

## Suggested fix

Add `--fail` to curl and validate before moving into place:

```
curl -sSfL 'https://duckduckgo.com/bang.js' -o "$tmp"
bun -e "JSON.parse(require('fs').readFileSync('$tmp','utf8'))"
mv "$tmp" "$dst"
```

Any JSON validator available in the toolchain works; `bun` is already a hard dependency.
