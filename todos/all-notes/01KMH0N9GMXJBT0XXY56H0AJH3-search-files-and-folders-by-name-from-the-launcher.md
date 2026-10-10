---
title: "Search files and folders by name from the launcher"
kind: feature
component: gadgets
status: needs-discussion
horizon: someday
---

# Search files and folders by name from the launcher

File search by name is the table-stakes feature that makes a launcher
feel like a real system tool. The open questions are the search
strategy (OS index or own index) and the icon approach, so the design
needs a decision before the work starts.

## Scope

- Search files and folders by name with fuzzy matching
- Show file icon, name, path, and size
- Actions: open file, reveal in file manager, copy path
- Configurable search roots (home dir, project dirs, etc.)
- Exclude patterns (node_modules, .git, build artifacts)

## Platform considerations

- **macOS**: `mdfind` (Spotlight metadata) for indexed search,
  fall back to filesystem walk. Spotlight is fast but only indexes
  certain locations.
- **Linux**: `locate` / `plocate` if available, otherwise `fd` or
  direct filesystem walk. No universal index exists.
- **Windows**: Windows Search API (`ISearchQueryHelper`), or
  Everything SDK (third-party but much faster).

## Performance

- Must return results within ~100ms for responsive feel
- Two strategies:
  1. Delegate to OS search index (mdfind, Windows Search): fast
     but limited to indexed locations
  2. Build own index at startup with filesystem walk + inotify/
     FSEvents watcher: more control but memory/startup cost
- Hybrid approach: OS index for broad search, own index for
  configured project directories
- Consider `nucleo` or `skim` crates for fuzzy matching

## File icons

- **macOS**: `NSWorkspace.icon(forFile:)` returns the actual
  file type icon
- **Linux**: Freedesktop icon theme spec, look up by MIME type
- **Windows**: `SHGetFileInfo` API
- Icons need caching: extracting per-file on every search is too
  slow

## Revisit when

No trigger known; reconsider at the next sweep.

## Relations

- Relates to: [Evaluate further gadget ideas beyond the initial set](01KMH0DSPAR1GGA0AHH1PYP356-evaluate-further-gadget-ideas-beyond-the-initial-set.md), which lists file search among the ideas to evaluate
