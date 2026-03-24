# Plugin: Clipboard manager

Track clipboard history and allow pasting previous entries from the
launcher.

## Scope

- Monitor clipboard changes in the background
- Store history (text, images, rich text) with timestamps
- Search/filter clipboard history by content
- Select an entry to paste it (copy to clipboard + optional
  simulated Cmd+V)
- Configurable history size limit
- Option to pin frequently used entries

## Platform considerations

- **macOS**: `NSPasteboard` change count polling or
  `NSPasteboard.general` observation
- **Linux**: X11 clipboard / Wayland clipboard protocols
- **Windows**: `AddClipboardFormatListener` API

## Privacy

- Option to exclude sensitive content (e.g. password manager
  entries that set a "concealed" pasteboard type)
- Auto-expire entries after configurable time
- Clear history command
