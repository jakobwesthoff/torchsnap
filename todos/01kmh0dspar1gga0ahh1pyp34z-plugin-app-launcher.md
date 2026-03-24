# Plugin: Application launcher

List installed applications on the system and launch them from the
search bar.

## Scope

- Index installed applications (name, icon, path)
- Fuzzy match against the search query
- Execute the selected application
- Show app icons in the result rows

## Platform considerations

- **macOS**: Scan `/Applications`, `~/Applications`, and Spotlight
  metadata (`mdfind kMDItemContentType=com.apple.application-bundle`)
- **Linux**: Parse `.desktop` files from XDG data dirs
  (`/usr/share/applications`, `~/.local/share/applications`)
- **Windows**: Scan Start Menu shortcuts, `shell:AppsFolder`

## Performance

- Build an index at startup, watch for changes via filesystem events
- Index should be in-memory for sub-millisecond fuzzy search
- App icons need to be extracted and cached (platform-specific APIs)
