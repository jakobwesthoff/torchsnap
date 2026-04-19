# Add `plugin-assets` host WIT interface

## Context

WASM plugins can only access data they bring into the WASM binary itself
(via `include_bytes!` / `include_str!`). Plugins with large static assets
(emoji JSON data, icon sets, bundled config files) must embed those assets
directly in the binary, inflating its size. A `plugin-assets` host interface
would let plugins read files from their own `.torchsnap` archive at runtime.

This was identified while designing the emoji-picker WASM conversion. The
emoji plugin embeds ~2.3 MB of emojibase JSON at compile time; moving it to
a runtime read would keep the binary small and allow data updates without
recompiling the plugin.

## Proposed WIT sketch

```wit
interface plugin-assets {
    /// Read a file from the plugin's own archive/directory by its
    /// manifest-relative path.  Returns the raw bytes on success,
    /// or an error string if the path is not found or cannot be read.
    read-file: func(path: string) -> result<list<u8>, string>;
}
```

Add `import plugin-assets;` to `world torchsnap-plugin`.

## Host-side implementation

- The host already knows the plugin's source (archive path or directory
  path) at enable time via `PluginState` / the loader.
- `read-file` resolves `path` relative to the plugin root, enforces that
  the path cannot escape the plugin directory (no `../`), then reads and
  returns the bytes.
- For `ArchiveSource` (`.torchsnap` zip): read the entry from the in-memory
  zip without re-opening the archive (keep an `Arc<ZipArchive>` or index).
- For `DirectorySource` (development): plain `fs::read`.

## Security note

Path traversal must be rejected — the host must normalise the path and
verify it stays within the plugin root before reading. No symlink following.

## Open Questions

- Should `read-file` be callable at any time, or only during `enable()`?
  Restricting to enable-time simplifies the host's archive-handle lifetime.
- Should the interface also expose `list-files(prefix)` for plugins that
  need to discover asset paths dynamically?
- Cache strategy: read-once into linear memory (guest side), or allow
  repeated host calls?

## References

- `src-tauri/src/wasm/host/` — existing host import implementations
- `src-tauri/src/wasm/runtime.rs` — linker wiring pattern
- `wit/torchsnap-plugin.wit` — WIT world to extend
- `plugins/emoji-picker/` — first plugin that would benefit
