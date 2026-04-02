# Favicon fetching and caching for result entries

## Motivation

Several plugins would benefit from showing service favicons instead of generic
Heroicons in their result entries:

- **Bangs plugin**: currently uses `arrow-top-right-on-square` for all bang
  results. Showing the actual service favicon (e.g. Google's "G", YouTube's
  play button) would make results instantly recognizable.
- **Open URL plugin** (`01kn1yv2d03fjccpdpn3t6rjs2`): will need favicons when
  showing URL results for detected links.

## Scope

A shared favicon service that:

1. Fetches favicons given a domain (e.g. via `https://<domain>/favicon.ico` or
   a favicon resolution service like Google's `t1.gstatic.com/faviconV2`).
2. Caches fetched favicons on disk (likely in the app cache dir) to avoid
   repeated network requests.
3. Exposes fetched favicons as `EntryIcon::AssetIcon` or `EntryIcon::DataUrl`
   for use in `ScoredEntry`.
4. Handles failures gracefully — fall back to a generic icon when the favicon
   cannot be fetched.

## Considerations

- The bangs database includes a `domain` field per bang, which is the natural
  key for favicon lookups.
- Favicon resolution can be async/lazy — show the generic icon initially and
  swap in the favicon once fetched.
- Consider cache eviction (LRU or time-based) to avoid unbounded disk growth.
- Some domains return SVG favicons, others ICO/PNG — need to handle multiple
  formats or normalize to a single format.
