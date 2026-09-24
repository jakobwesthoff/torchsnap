---
kind: feature
status: open
---

# Auto-updater

Users need a way to receive updates without manually downloading
new versions. Tauri provides `tauri-plugin-updater` for this.

## Tauri updater overview

- `tauri-plugin-updater` checks a remote endpoint for new versions
- Supports differential updates (only downloads changed files)
- Signed updates for security (Ed25519)
- Can use GitHub Releases as the update server (free, simple)
- Or a custom endpoint returning a JSON manifest

## What needs to happen

1. Add `tauri-plugin-updater` to Rust and JS dependencies
2. Generate signing keys for update verification
3. Configure update endpoint in `tauri.conf.json`
4. Set up GitHub Releases as the update server (or evaluate
   alternatives)
5. Build UI for update notifications:
   - Check for updates on startup (configurable interval)
   - Tray menu item: "Check for Updates..."
   - Settings section showing current version and update status
   - Notification when update is available
   - Option: auto-install on next launch vs manual trigger

## Platform considerations

- **macOS**: DMG replacement or in-place update. Notarization
  required for the update artifact too.
- **Linux**: AppImage supports in-place update. Deb/RPM would
  need a different strategy (apt repo?).
- **Windows**: NSIS/MSI installer update. May need elevation.

## Security

- All updates must be signed with a key controlled by us
- The public key is embedded in the app binary at build time
- Never download or execute unsigned updates
