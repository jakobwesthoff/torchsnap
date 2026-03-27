# Clipboard: Source app identification and icon display

The clipboard manager UI shows an icon per entry indicating where the
content came from. For the initial implementation, we use heroicons
based on content type (text, image, file, etc.) instead of the actual
source app icon.

## Why deferred

Reliable source app identification on macOS requires platform-specific
code outside `clipboard-rs`:

- `org.nspasteboard.source` is an opt-in convention. Most apps don't
  set it. `clipboard-rs` can read it via `get_buffer()` when present,
  but it's unreliable.
- The reliable approach is capturing `NSWorkspace.frontmostApplication`
  at clipboard-change time, then resolving the bundle ID to an app
  icon. This is racy (the frontmost app may not be the one that wrote
  the clipboard) and entirely macOS-specific.
- Needs a platform abstraction trait (similar to `AppDiscovery`) since
  source app identification works differently on each OS.

## Future implementation

- On clipboard change, capture the frontmost app's bundle ID via
  `NSWorkspace.shared.frontmostApplication.bundleIdentifier`
- Resolve bundle ID → `.app` path via
  `NSWorkspace.urlForApplication(withBundleIdentifier:)`
- Extract icon using the existing icon cache infrastructure
- Store the bundle ID alongside the clipboard entry in SqlStorage
- Platform trait to abstract this across macOS/Linux/Windows

## Current approach

Use heroicons in the clipboard history list based on content type:
- Text → document-text icon
- Image → photo icon
- Files → document icon
- Rich text / HTML → code-bracket icon
- URL → link icon
