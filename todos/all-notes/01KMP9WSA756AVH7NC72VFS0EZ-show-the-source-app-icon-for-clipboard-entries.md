---
title: "Show the source app icon for clipboard entries"
kind: feature
component: gadgets
horizon: someday
---
# Show the source app icon for clipboard entries

The clipboard history list shows an icon per entry, but for the initial
implementation it reflects the content type (text, image, file and so on)
instead of the app the content came from. Showing the source app's icon
needs platform-specific code outside `clipboard-rs`, which is why it is
deferred.

## Why deferred

Reliable source app identification on macOS requires platform-specific
code outside `clipboard-rs`:

- `org.nspasteboard.source` is an opt-in convention. Most apps do not
  set it. `clipboard-rs` can read it via `get_buffer()` when present,
  but it is unreliable.
- The reliable approach is capturing `NSWorkspace.frontmostApplication`
  at clipboard-change time, then resolving the bundle ID to an app
  icon. This is racy (the frontmost app may not be the one that wrote
  the clipboard) and entirely macOS-specific.
- Needs a platform abstraction trait (similar to `AppDiscovery`) since
  source app identification works differently on each OS.

## Future implementation

- On clipboard change, capture the frontmost app's bundle ID via
  `NSWorkspace.shared.frontmostApplication.bundleIdentifier`
- Resolve bundle ID to `.app` path via
  `NSWorkspace.urlForApplication(withBundleIdentifier:)`
- Extract icon using the existing icon cache infrastructure
- Store the bundle ID alongside the clipboard entry in SqlStorage
- Platform trait to abstract this across macOS, Linux and Windows

## Current approach

Use heroicons in the clipboard history list based on content type:
- Text: document-text icon
- Image: photo icon
- Files: document icon
- Rich text / HTML: code-bracket icon
- URL: link icon

## Revisit when

No trigger known; reconsider at the next sweep.
