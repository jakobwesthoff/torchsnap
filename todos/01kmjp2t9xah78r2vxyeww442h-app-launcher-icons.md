# App Launcher: Real Application Icons

Extract and display real macOS application icons in the launcher
result rows instead of the generic placeholder.

## Context

The app launcher plugin (plugin-app-launcher) initially ships with a
generic icon for all applications. This todo covers adding real icon
extraction and caching.

## Icon extraction options evaluated

1. **`sips` (built-in macOS)**: `sips -s format png icon.icns --out
   icon.png`. Zero dependencies, spawns one process per icon.
2. **`icns` Rust crate**: Pure Rust `.icns` parser. Extracts
   individual resolutions. No process spawning.
3. **`NSWorkspace` via objc2**: `NSWorkspace.shared().icon(forFile:)`
   — system-composited icon with overlays. Most correct but needs
   objc bindings.
4. **Data URL pipeline**: Convert to small PNG → base64 → `DataUrl`
   `EntryIcon` variant. Simple.

## Approach

- Extract icon path from each app's `Info.plist` (`CFBundleIconFile`
  / `CFBundleIconName`)
- Convert `.icns` → PNG at 32×32 and 64×64 (@2x)
- Cache converted PNGs on disk (e.g., `~/.cache/torchsnap/icons/`)
- Serve as `DataUrl` or via a Tauri asset protocol endpoint
- Invalidate cache when app bundle modification date changes

## Platform considerations

- macOS: `.icns` files inside `.app/Contents/Resources/`
- Linux: Icons from `.desktop` file `Icon=` field, resolved via icon
  theme spec
- Windows: Extract from PE resources or shortcut targets
